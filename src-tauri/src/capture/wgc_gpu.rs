use crate::{
    audio, camera,
    recording::types::{CaptureKind, CaptureTarget, RecordingConfig},
};
use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, Stream, StreamConfig};
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use windows_capture::capture::{CaptureControl, Context as CaptureContext, GraphicsCaptureApiHandler};
use windows_capture::d3d11::SendDirectX;
use windows_capture::encoder::{
    AudioSettingsBuilder, ContainerSettingsBuilder, VideoEncoder, VideoSettingsBuilder,
    VideoSettingsSubType,
};
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    GraphicsCaptureItemType, MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};
use windows_capture::window::Window;
use windows_capture_windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;

const MIX_SAMPLE_RATE: u32 = 48_000;
const MIX_CHANNELS: u32 = 2;
const MIX_CHUNK_FRAMES: usize = 480; // 10 ms at 48 kHz.
const MIX_PREBUFFER_FRAMES: usize = 2_400; // 50 ms startup cushion.
const MAX_MIX_QUEUE_FRAMES: usize = 96_000; // Bound each source to two seconds.
const MIC_GAIN: f32 = 1.35;
const SYSTEM_GAIN: f32 = 0.68;
const SYSTEM_DUCK_GAIN: f32 = 0.42;
const MIC_DUCK_RMS: f32 = 0.018;

#[derive(Clone, Copy)]
struct AudioSpec {
    sample_rate: u32,
    channels: u32,
}

const MIX_AUDIO_SPEC: AudioSpec = AudioSpec {
    sample_rate: MIX_SAMPLE_RATE,
    channels: MIX_CHANNELS,
};

#[derive(Clone, Copy)]
enum NativeAudioSource {
    Microphone,
    System,
    Mixed,
}

impl NativeAudioSource {
    const fn label(self) -> &'static str {
        match self {
            Self::Microphone => "microphone",
            Self::System => "system audio",
            Self::Mixed => "mixed microphone/system audio",
        }
    }
}

struct AudioInput {
    device: Device,
    config: StreamConfig,
    sample_format: SampleFormat,
    spec: AudioSpec,
    source: NativeAudioSource,
}

type CameraFrameSlot = Arc<Mutex<Option<Vec<u8>>>>;
type AudioFrameQueue = Arc<Mutex<VecDeque<[f32; 2]>>>;

#[derive(Clone)]
struct GpuFlags {
    output_path: PathBuf,
    width: u32,
    height: u32,
    fps: u32,
    audio: Option<AudioSpec>,
    camera_frame: Option<CameraFrameSlot>,
}

struct GpuFrameEncoder {
    encoder: Option<VideoEncoder>,
    frame_interval_hns: i64,
    next_frame_hns: Option<i64>,
    camera_frame: Option<CameraFrameSlot>,
    camera_texture: Option<SendDirectX<ID3D11Texture2D>>,
}

impl GpuFrameEncoder {
    fn finish(&mut self) -> Result<(), String> {
        let Some(encoder) = self.encoder.take() else {
            return Ok(());
        };
        encoder
            .finish()
            .map_err(|error| format!("Unable to finalize native Windows H.264 encoder: {error}"))
    }

    fn send_audio_pcm(&mut self, pcm: &[u8], source: NativeAudioSource) -> Result<(), String> {
        self.encoder
            .as_mut()
            .ok_or_else(|| "Native Windows encoder is already finalized".to_string())?
            .send_audio_buffer(pcm, 0)
            .map_err(|error| {
                format!(
                    "Unable to submit {} PCM to native Windows encoder: {error}",
                    source.label()
                )
            })
    }

    fn ensure_camera_texture(&mut self, frame: &Frame) -> Result<(), String> {
        if self.camera_texture.is_some() {
            return Ok(());
        }

        let mut desc = *frame.desc();
        desc.Width = camera::OVERLAY_WIDTH;
        desc.Height = camera::OVERLAY_HEIGHT;
        desc.MipLevels = 1;
        desc.ArraySize = 1;
        desc.SampleDesc.Count = 1;
        desc.SampleDesc.Quality = 0;
        desc.BindFlags = 0;
        desc.CPUAccessFlags = 0;
        desc.MiscFlags = 0;

        let mut texture = None;
        unsafe {
            frame
                .device()
                .CreateTexture2D(&desc, None, Some(&mut texture))
                .map_err(|error| format!("Unable to create persistent D3D11 camera overlay texture: {error}"))?;
        }
        let texture = texture
            .ok_or_else(|| "D3D11 did not return a camera overlay texture".to_string())?;
        self.camera_texture = Some(SendDirectX::new(texture));
        Ok(())
    }

    fn overlay_camera(&mut self, frame: &mut Frame) -> Result<(), String> {
        let Some(slot) = self.camera_frame.as_ref() else {
            return Ok(());
        };

        // Copy the small webcam frame out of the shared slot first. This keeps the
        // camera reader free while D3D11 commands are submitted.
        let bytes = {
            let latest = slot.lock();
            let Some(bytes) = latest.as_ref() else {
                return Ok(());
            };
            bytes.clone()
        };

        let camera_width = camera::OVERLAY_WIDTH;
        let camera_height = camera::OVERLAY_HEIGHT;
        let expected = camera_width as usize * camera_height as usize * 4;
        if bytes.len() != expected {
            return Err(format!(
                "Unexpected camera frame size: got {}, expected {expected}",
                bytes.len()
            ));
        }

        if frame.width() <= camera_width + 48 || frame.height() <= camera_height + 48 {
            return Ok(());
        }

        // Reuse one GPU resource for the whole recording. The previous implementation
        // created and destroyed a D3D11 texture every encoded frame, which caused
        // intermittent camera-overlay stalls/flicker on some drivers.
        self.ensure_camera_texture(frame)?;
        let camera_texture = &self
            .camera_texture
            .as_ref()
            .ok_or_else(|| "Camera overlay texture was not initialized".to_string())?
            .0;

        let dst_x = frame.width() - camera_width - 24;
        let dst_y = frame.height() - camera_height - 24;
        unsafe {
            frame.device_context().UpdateSubresource(
                camera_texture,
                0,
                None,
                bytes.as_ptr().cast::<c_void>(),
                camera_width * 4,
                0,
            );
            frame.device_context().CopySubresourceRegion(
                frame.as_raw_texture(),
                0,
                dst_x,
                dst_y,
                0,
                camera_texture,
                0,
                None,
            );
            // The encoder consumes the WGC surface asynchronously. Flush submits the
            // ordered upload/copy before the surface is handed to Media Foundation.
            frame.device_context().Flush();
        }
        Ok(())
    }
}

impl GraphicsCaptureApiHandler for GpuFrameEncoder {
    type Flags = GpuFlags;
    type Error = String;

    fn new(ctx: CaptureContext<Self::Flags>) -> Result<Self, Self::Error> {
        let fps = ctx.flags.fps.clamp(1, 60);
        let pixels = u64::from(ctx.flags.width) * u64::from(ctx.flags.height);
        let mut bitrate: u32 = if pixels <= 1920 * 1200 {
            12_000_000
        } else if pixels <= 2560 * 1440 {
            20_000_000
        } else {
            35_000_000
        };
        if fps > 30 {
            bitrate = bitrate.saturating_mul(3) / 2;
        }

        let audio_settings = match ctx.flags.audio {
            Some(audio) => AudioSettingsBuilder::new()
                .sample_rate(audio.sample_rate)
                .channel_count(audio.channels)
                .bit_per_sample(16),
            None => AudioSettingsBuilder::new().disabled(true),
        };

        let encoder = VideoEncoder::new(
            VideoSettingsBuilder::new(ctx.flags.width, ctx.flags.height)
                .sub_type(VideoSettingsSubType::H264)
                .bitrate(bitrate)
                .frame_rate(fps),
            audio_settings,
            ContainerSettingsBuilder::new(),
            &ctx.flags.output_path,
        )
        .map_err(|error| format!("Unable to initialize native Windows H.264 encoder: {error}"))?;

        Ok(Self {
            encoder: Some(encoder),
            frame_interval_hns: (10_000_000i64 / i64::from(fps)).max(1),
            next_frame_hns: None,
            camera_frame: ctx.flags.camera_frame,
            camera_texture: None,
        })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        _capture_control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        let timestamp = frame
            .timestamp()
            .map_err(|error| format!("Unable to read WGC frame timestamp: {error}"))?
            .Duration;

        // WGC can deliver at desktop refresh rate. Decimate before camera upload or
        // encoder work so 120/144 Hz monitors remain bounded at the requested FPS.
        if let Some(next) = self.next_frame_hns {
            if timestamp < next {
                return Ok(());
            }
            let mut following = next;
            while following <= timestamp {
                following = following.saturating_add(self.frame_interval_hns);
            }
            self.next_frame_hns = Some(following);
        } else {
            self.next_frame_hns = Some(timestamp.saturating_add(self.frame_interval_hns));
        }

        self.overlay_camera(frame)?;

        self.encoder
            .as_mut()
            .ok_or_else(|| "Native Windows encoder is already finalized".to_string())?
            .send_frame(frame)
            .map_err(|error| format!("Unable to submit D3D11 frame to native Windows encoder: {error}"))
    }

    fn on_closed(&mut self) -> Result<(), Self::Error> {
        self.finish()
    }
}

fn resolve_microphone_input(id: &str) -> Result<AudioInput> {
    let requested_name = audio::resolve_microphone_name(id)
        .ok_or_else(|| anyhow!("The selected microphone is no longer available"))?;
    let requested_lower = requested_name.to_lowercase();
    let host = cpal::default_host();
    let devices = host
        .input_devices()
        .context("Unable to enumerate native Windows microphone devices")?;

    let mut partial_match = None;
    let mut selected = None;
    for device in devices {
        let Ok(name) = device.name() else { continue; };
        let lower = name.to_lowercase();
        if lower == requested_lower {
            selected = Some(device);
            break;
        }
        if partial_match.is_none()
            && (lower.contains(&requested_lower) || requested_lower.contains(&lower))
        {
            partial_match = Some(device);
        }
    }
    let device = selected.or(partial_match).ok_or_else(|| {
        anyhow!("The selected microphone is not available through Windows audio capture: {requested_name}")
    })?;
    let supported = device
        .default_input_config()
        .with_context(|| format!("Unable to read microphone format for {requested_name}"))?;
    audio_input_from_supported(device, supported, NativeAudioSource::Microphone)
}

fn resolve_system_audio_input() -> Result<AudioInput> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| anyhow!("Windows has no default audio output device"))?;
    let supported = device
        .default_output_config()
        .context("Unable to read the Windows default output format")?;
    audio_input_from_supported(device, supported, NativeAudioSource::System)
}

fn audio_input_from_supported(
    device: Device,
    supported: cpal::SupportedStreamConfig,
    source: NativeAudioSource,
) -> Result<AudioInput> {
    let sample_format = supported.sample_format();
    let config: StreamConfig = supported.into();
    if config.channels == 0 || config.sample_rate.0 == 0 {
        return Err(anyhow!(
            "The {} endpoint reported an invalid audio format",
            source.label()
        ));
    }

    Ok(AudioInput {
        device,
        spec: AudioSpec {
            sample_rate: config.sample_rate.0,
            channels: u32::from(config.channels),
        },
        config,
        sample_format,
        source,
    })
}

fn encode_pcm_i16<T: Copy>(samples: &[T], convert: impl Fn(T) -> i16) -> Vec<u8> {
    let mut pcm = Vec::with_capacity(samples.len() * 2);
    for &sample in samples {
        pcm.extend_from_slice(&convert(sample).to_le_bytes());
    }
    pcm
}

fn submit_audio_pcm<T: Copy>(
    callback: &Arc<Mutex<GpuFrameEncoder>>,
    error_state: &Arc<Mutex<Option<String>>>,
    source: NativeAudioSource,
    samples: &[T],
    convert: impl Fn(T) -> i16,
) {
    if error_state.lock().is_some() {
        return;
    }
    let pcm = encode_pcm_i16(samples, convert);
    if let Err(error) = callback.lock().send_audio_pcm(&pcm, source) {
        *error_state.lock() = Some(error);
    }
}

struct NativeSingleAudioCapture {
    stream: Stream,
    error_state: Arc<Mutex<Option<String>>>,
}

unsafe impl Send for NativeSingleAudioCapture {}

impl NativeSingleAudioCapture {
    fn stop(self) -> Result<()> {
        let Self { stream, error_state } = self;
        let _ = stream.pause();
        drop(stream);
        if let Some(error) = error_state.lock().take() {
            return Err(anyhow!(error));
        }
        Ok(())
    }
}

fn start_audio_capture(
    input: AudioInput,
    callback: Arc<Mutex<GpuFrameEncoder>>,
) -> Result<NativeSingleAudioCapture> {
    let source = input.source;
    let source_label = source.label();
    let error_state = Arc::new(Mutex::new(None::<String>));
    let stream_error_state = error_state.clone();
    let error_callback = move |error| {
        eprintln!("Native {source_label} stream error: {error}");
        let mut state = stream_error_state.lock();
        if state.is_none() {
            *state = Some(format!("Native {source_label} stream error: {error}"));
        }
    };

    let stream = match input.sample_format {
        SampleFormat::F32 => {
            let sink = callback.clone();
            let errors = error_state.clone();
            input.device.build_input_stream(
                &input.config,
                move |data: &[f32], _| {
                    submit_audio_pcm(&sink, &errors, source, data, |v| {
                        (v.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16
                    })
                },
                error_callback,
                None,
            )
        }
        SampleFormat::I16 => {
            let sink = callback.clone();
            let errors = error_state.clone();
            input.device.build_input_stream(
                &input.config,
                move |data: &[i16], _| submit_audio_pcm(&sink, &errors, source, data, |v| v),
                error_callback,
                None,
            )
        }
        SampleFormat::U16 => {
            let sink = callback.clone();
            let errors = error_state.clone();
            input.device.build_input_stream(
                &input.config,
                move |data: &[u16], _| {
                    submit_audio_pcm(&sink, &errors, source, data, |v| (i32::from(v) - 32_768) as i16)
                },
                error_callback,
                None,
            )
        }
        SampleFormat::I32 => {
            let sink = callback.clone();
            let errors = error_state.clone();
            input.device.build_input_stream(
                &input.config,
                move |data: &[i32], _| {
                    submit_audio_pcm(&sink, &errors, source, data, |v| (v >> 16) as i16)
                },
                error_callback,
                None,
            )
        }
        SampleFormat::F64 => {
            let sink = callback.clone();
            let errors = error_state.clone();
            input.device.build_input_stream(
                &input.config,
                move |data: &[f64], _| {
                    submit_audio_pcm(&sink, &errors, source, data, |v| {
                        (v.clamp(-1.0, 1.0) * i16::MAX as f64).round() as i16
                    })
                },
                error_callback,
                None,
            )
        }
        other => return Err(anyhow!("Unsupported native {source_label} sample format: {other:?}")),
    }
    .with_context(|| format!("Unable to create native Windows {source_label} stream"))?;

    stream
        .play()
        .with_context(|| format!("Unable to start native Windows {source_label} capture"))?;
    Ok(NativeSingleAudioCapture { stream, error_state })
}

struct StereoResampler {
    input_rate: f64,
    channels: usize,
    next_position: f64,
    previous: Option<[f32; 2]>,
}

impl StereoResampler {
    fn new(config: &StreamConfig) -> Self {
        Self {
            input_rate: f64::from(config.sample_rate.0),
            channels: usize::from(config.channels),
            next_position: 0.0,
            previous: None,
        }
    }

    fn push<T: Copy>(
        &mut self,
        samples: &[T],
        convert: impl Fn(T) -> f32,
        queue: &AudioFrameQueue,
    ) {
        if self.channels == 0 {
            return;
        }

        let mut frames = Vec::<[f32; 2]>::with_capacity(samples.len() / self.channels + 1);
        if let Some(previous) = self.previous {
            frames.push(previous);
        }
        for frame in samples.chunks_exact(self.channels) {
            let left = convert(frame[0]).clamp(-1.0, 1.0);
            let right = if self.channels == 1 {
                left
            } else {
                convert(frame[1]).clamp(-1.0, 1.0)
            };
            frames.push([left, right]);
        }

        if frames.is_empty() {
            return;
        }
        if frames.len() == 1 {
            self.previous = frames.last().copied();
            return;
        }

        let step = self.input_rate / f64::from(MIX_SAMPLE_RATE);
        let mut position = self.next_position;
        let mut output = Vec::<[f32; 2]>::new();
        while position + 1.0 < frames.len() as f64 {
            let index = position.floor() as usize;
            let fraction = (position - index as f64) as f32;
            let a = frames[index];
            let b = frames[index + 1];
            output.push([
                a[0] + (b[0] - a[0]) * fraction,
                a[1] + (b[1] - a[1]) * fraction,
            ]);
            position += step;
        }

        position -= (frames.len() - 1) as f64;
        self.next_position = position.max(0.0);
        self.previous = frames.last().copied();

        if output.is_empty() {
            return;
        }
        let mut queue = queue.lock();
        queue.extend(output);
        while queue.len() > MAX_MIX_QUEUE_FRAMES {
            queue.pop_front();
        }
    }
}

fn build_resampled_stream(
    input: AudioInput,
    queue: AudioFrameQueue,
    error_state: Arc<Mutex<Option<String>>>,
) -> Result<Stream> {
    let source_label = input.source.label();
    let stream_error_state = error_state.clone();
    let error_callback = move |error| {
        eprintln!("Native {source_label} stream error: {error}");
        let mut state = stream_error_state.lock();
        if state.is_none() {
            *state = Some(format!("Native {source_label} stream error: {error}"));
        }
    };

    let stream = match input.sample_format {
        SampleFormat::F32 => {
            let mut resampler = StereoResampler::new(&input.config);
            let sink = queue.clone();
            input.device.build_input_stream(
                &input.config,
                move |data: &[f32], _| resampler.push(data, |v| v, &sink),
                error_callback,
                None,
            )
        }
        SampleFormat::I16 => {
            let mut resampler = StereoResampler::new(&input.config);
            let sink = queue.clone();
            input.device.build_input_stream(
                &input.config,
                move |data: &[i16], _| resampler.push(data, |v| v as f32 / i16::MAX as f32, &sink),
                error_callback,
                None,
            )
        }
        SampleFormat::U16 => {
            let mut resampler = StereoResampler::new(&input.config);
            let sink = queue.clone();
            input.device.build_input_stream(
                &input.config,
                move |data: &[u16], _| {
                    resampler.push(data, |v| (v as f32 / u16::MAX as f32) * 2.0 - 1.0, &sink)
                },
                error_callback,
                None,
            )
        }
        SampleFormat::I32 => {
            let mut resampler = StereoResampler::new(&input.config);
            let sink = queue.clone();
            input.device.build_input_stream(
                &input.config,
                move |data: &[i32], _| resampler.push(data, |v| v as f32 / i32::MAX as f32, &sink),
                error_callback,
                None,
            )
        }
        SampleFormat::F64 => {
            let mut resampler = StereoResampler::new(&input.config);
            let sink = queue.clone();
            input.device.build_input_stream(
                &input.config,
                move |data: &[f64], _| resampler.push(data, |v| v as f32, &sink),
                error_callback,
                None,
            )
        }
        other => return Err(anyhow!("Unsupported native {source_label} sample format: {other:?}")),
    }
    .with_context(|| format!("Unable to create native Windows {source_label} stream for mixing"))?;

    Ok(stream)
}

fn queue_len(queue: &AudioFrameQueue) -> usize {
    queue.lock().len()
}

fn try_drain_mix_chunk(queue: &AudioFrameQueue) -> Option<Vec<[f32; 2]>> {
    let mut queue = queue.lock();
    if queue.len() < MIX_CHUNK_FRAMES {
        // Crucial: do not consume a short callback and pad the remainder with
        // silence. Keeping partial data lets the next callback complete the block.
        return None;
    }

    let mut chunk = Vec::with_capacity(MIX_CHUNK_FRAMES);
    for _ in 0..MIX_CHUNK_FRAMES {
        if let Some(frame) = queue.pop_front() {
            chunk.push(frame);
        }
    }
    Some(chunk)
}

fn silence_chunk() -> Vec<[f32; 2]> {
    vec![[0.0, 0.0]; MIX_CHUNK_FRAMES]
}

fn microphone_rms(chunk: &[[f32; 2]]) -> f32 {
    let sum = chunk.iter().fold(0.0f32, |acc, frame| {
        acc + frame[0] * frame[0] + frame[1] * frame[1]
    });
    (sum / (chunk.len().max(1) as f32 * 2.0)).sqrt()
}

fn start_mixer_thread(
    callback: Arc<Mutex<GpuFrameEncoder>>,
    microphone_queue: AudioFrameQueue,
    system_queue: AudioFrameQueue,
    stop: Arc<AtomicBool>,
    error_state: Arc<Mutex<Option<String>>>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        // Let the independent WASAPI callback clocks build a small cushion first.
        // This avoids the mixer starting at t=0 and repeatedly draining one source
        // before the other source's first callback has arrived.
        let deadline = Instant::now() + Duration::from_millis(500);
        while !stop.load(Ordering::Acquire)
            && Instant::now() < deadline
            && (queue_len(&microphone_queue) < MIX_PREBUFFER_FRAMES
                || queue_len(&system_queue) < MIX_PREBUFFER_FRAMES)
        {
            thread::sleep(Duration::from_millis(5));
        }

        let period = Duration::from_millis(10);
        let mut next_tick = Instant::now();

        while !stop.load(Ordering::Acquire) {
            let microphone = try_drain_mix_chunk(&microphone_queue).unwrap_or_else(silence_chunk);
            let system = try_drain_mix_chunk(&system_queue).unwrap_or_else(silence_chunk);
            let mic_rms = microphone_rms(&microphone);
            let system_gain = if mic_rms >= MIC_DUCK_RMS {
                SYSTEM_DUCK_GAIN
            } else {
                SYSTEM_GAIN
            };

            let mut pcm = Vec::with_capacity(MIX_CHUNK_FRAMES * MIX_CHANNELS as usize * 2);
            for index in 0..MIX_CHUNK_FRAMES {
                // Microphone has priority over media audio. When speech is present,
                // system audio ducks instead of masking the speaker's voice.
                let left = (microphone[index][0] * MIC_GAIN + system[index][0] * system_gain)
                    .clamp(-1.0, 1.0);
                let right = (microphone[index][1] * MIC_GAIN + system[index][1] * system_gain)
                    .clamp(-1.0, 1.0);
                pcm.extend_from_slice(&((left * i16::MAX as f32).round() as i16).to_le_bytes());
                pcm.extend_from_slice(&((right * i16::MAX as f32).round() as i16).to_le_bytes());
            }

            if let Err(error) = callback
                .lock()
                .send_audio_pcm(&pcm, NativeAudioSource::Mixed)
            {
                let mut state = error_state.lock();
                if state.is_none() {
                    *state = Some(error);
                }
                break;
            }

            next_tick += period;
            let now = Instant::now();
            if next_tick > now {
                thread::sleep(next_tick - now);
            } else {
                // Never burst stale audio after a scheduler stall.
                next_tick = now;
            }
        }
    })
}

struct NativeMixedAudioCapture {
    streams: Vec<Stream>,
    stop: Arc<AtomicBool>,
    mixer_thread: Option<JoinHandle<()>>,
    error_state: Arc<Mutex<Option<String>>>,
}

unsafe impl Send for NativeMixedAudioCapture {}

impl NativeMixedAudioCapture {
    fn stop(mut self) -> Result<()> {
        for stream in &self.streams {
            let _ = stream.pause();
        }
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.mixer_thread.take() {
            if thread.join().is_err() {
                return Err(anyhow!("Native microphone/system mixer thread panicked"));
            }
        }
        self.streams.clear();
        if let Some(error) = self.error_state.lock().take() {
            return Err(anyhow!(error));
        }
        Ok(())
    }
}

fn start_mixed_audio_capture(
    microphone: AudioInput,
    system: AudioInput,
    callback: Arc<Mutex<GpuFrameEncoder>>,
) -> Result<NativeMixedAudioCapture> {
    eprintln!(
        "[Recorder] Native audio mixer: mic={}Hz/{}ch, system={}Hz/{}ch -> {}Hz/{}ch",
        microphone.spec.sample_rate,
        microphone.spec.channels,
        system.spec.sample_rate,
        system.spec.channels,
        MIX_SAMPLE_RATE,
        MIX_CHANNELS
    );

    let microphone_queue: AudioFrameQueue = Arc::new(Mutex::new(VecDeque::new()));
    let system_queue: AudioFrameQueue = Arc::new(Mutex::new(VecDeque::new()));
    let error_state = Arc::new(Mutex::new(None::<String>));

    let microphone_stream = build_resampled_stream(
        microphone,
        microphone_queue.clone(),
        error_state.clone(),
    )?;
    let system_stream = build_resampled_stream(
        system,
        system_queue.clone(),
        error_state.clone(),
    )?;

    microphone_stream
        .play()
        .context("Unable to start microphone stream for native audio mixing")?;
    if let Err(error) = system_stream.play() {
        let _ = microphone_stream.pause();
        return Err(error).context("Unable to start system-audio stream for native audio mixing");
    }

    let stop = Arc::new(AtomicBool::new(false));
    let mixer_thread = start_mixer_thread(
        callback,
        microphone_queue,
        system_queue,
        stop.clone(),
        error_state.clone(),
    );

    Ok(NativeMixedAudioCapture {
        streams: vec![microphone_stream, system_stream],
        stop,
        mixer_thread: Some(mixer_thread),
        error_state,
    })
}

enum NativeAudioCapture {
    Single(NativeSingleAudioCapture),
    Mixed(NativeMixedAudioCapture),
}

impl NativeAudioCapture {
    fn stop(self) -> Result<()> {
        match self {
            Self::Single(capture) => capture.stop(),
            Self::Mixed(capture) => capture.stop(),
        }
    }
}

pub struct NativeGpuVideoCapture {
    control: Option<CaptureControl<GpuFrameEncoder, String>>,
    audio: Option<NativeAudioCapture>,
    camera: Option<camera::CameraFrameCapture>,
    captures_system_audio: bool,
}

impl NativeGpuVideoCapture {
    pub const fn captures_system_audio(&self) -> bool {
        self.captures_system_audio
    }

    pub fn stop(mut self) -> Result<()> {
        // Stop auxiliary producers before WGC/encoder finalization so neither
        // webcam frames nor PCM can race with the final MP4 flush.
        let camera_result = match self.camera.take() {
            Some(camera) => camera.stop(),
            None => Ok(()),
        };
        let audio_result = match self.audio.take() {
            Some(audio) => audio.stop(),
            None => Ok(()),
        };

        let Some(control) = self.control.take() else {
            camera_result?;
            audio_result?;
            return Ok(());
        };

        let callback = control.callback();
        let capture_result = control
            .stop()
            .map_err(|error| anyhow!("Unable to stop Windows Graphics Capture: {error}"));
        let finish_result = callback.lock().finish().map_err(anyhow::Error::msg);

        camera_result?;
        audio_result?;
        capture_result?;
        finish_result?;
        Ok(())
    }
}

fn start_item<T>(
    item: T,
    config: &RecordingConfig,
    width: u32,
    height: u32,
    output_path: &Path,
) -> Result<NativeGpuVideoCapture>
where
    T: TryInto<GraphicsCaptureItemType> + Send + 'static,
{
    let microphone_input = config
        .microphone_id
        .as_deref()
        .map(resolve_microphone_input)
        .transpose()?;
    let system_input = if config.system_audio {
        Some(resolve_system_audio_input()?)
    } else {
        None
    };

    let audio_spec = match (&microphone_input, &system_input) {
        (Some(_), Some(_)) => Some(MIX_AUDIO_SPEC),
        (Some(input), None) | (None, Some(input)) => Some(input.spec),
        (None, None) => None,
    };
    let captures_system_audio = system_input.is_some();

    let camera_frame: Option<CameraFrameSlot> = config
        .camera_id
        .as_ref()
        .map(|_| Arc::new(Mutex::new(None)));

    let fps = config.fps.clamp(1, 60);
    let settings = Settings::new(
        item,
        CursorCaptureSettings::Default,
        DrawBorderSettings::Default,
        SecondaryWindowSettings::Default,
        MinimumUpdateIntervalSettings::Default,
        DirtyRegionSettings::Default,
        ColorFormat::Bgra8,
        GpuFlags {
            output_path: output_path.to_path_buf(),
            width,
            height,
            fps,
            audio: audio_spec,
            camera_frame: camera_frame.clone(),
        },
    );

    let control = GpuFrameEncoder::start_free_threaded(settings)
        .map_err(|error| anyhow!("Unable to start native WGC/D3D11 encoder: {error}"))?;

    let mut camera_capture = if let (Some(id), Some(slot)) =
        (config.camera_id.as_deref(), camera_frame)
    {
        let callback = control.callback();
        match camera::start_camera_frame_capture(id, slot) {
            Ok(camera) => Some(camera),
            Err(error) => {
                let _ = control.stop();
                let _ = callback.lock().finish();
                return Err(error.context("Unable to attach camera to native D3D11 recording"));
            }
        }
    } else {
        None
    };

    let callback = control.callback();
    let audio_result: Result<Option<NativeAudioCapture>> = match (microphone_input, system_input) {
        (Some(microphone), Some(system)) => {
            start_mixed_audio_capture(microphone, system, callback.clone())
                .map(NativeAudioCapture::Mixed)
                .map(Some)
        }
        (Some(input), None) | (None, Some(input)) => start_audio_capture(input, callback.clone())
            .map(NativeAudioCapture::Single)
            .map(Some),
        (None, None) => Ok(None),
    };

    let audio_capture = match audio_result {
        Ok(capture) => capture,
        Err(error) => {
            if let Some(camera) = camera_capture.take() {
                let _ = camera.stop();
            }
            let _ = control.stop();
            let _ = callback.lock().finish();
            return Err(error.context("Unable to attach audio to native Windows encoder"));
        }
    };

    Ok(NativeGpuVideoCapture {
        control: Some(control),
        audio: audio_capture,
        camera: camera_capture,
        captures_system_audio,
    })
}

pub fn start_native_gpu_video_capture(
    config: &RecordingConfig,
    target: &CaptureTarget,
    width: u32,
    height: u32,
    output_path: &Path,
) -> Result<NativeGpuVideoCapture> {
    match target.kind {
        CaptureKind::Display => {
            let raw = target
                .id
                .strip_prefix("monitor-")
                .ok_or_else(|| anyhow!("Invalid monitor id"))?;
            let handle = usize::from_str_radix(raw, 16).map_err(|_| anyhow!("Invalid monitor id"))?;
            let monitor = Monitor::from_raw_hmonitor(handle as *mut c_void);
            start_item(monitor, config, width, height, output_path)
        }
        CaptureKind::Window => {
            let raw = target
                .id
                .strip_prefix("window-")
                .ok_or_else(|| anyhow!("Invalid window id"))?;
            let handle = usize::from_str_radix(raw, 16).map_err(|_| anyhow!("Invalid window id"))?;
            let window = Window::from_raw_hwnd(handle as *mut c_void);
            if !window.is_valid() {
                return Err(anyhow!("Selected window is no longer capturable"));
            }
            start_item(window, config, width, height, output_path)
        }
    }
}
