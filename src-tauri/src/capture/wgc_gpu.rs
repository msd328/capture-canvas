use crate::{
    audio,
    recording::types::{CaptureKind, CaptureTarget, RecordingConfig},
};
use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, Stream, StreamConfig};
use parking_lot::Mutex;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use windows_capture::capture::{CaptureControl, Context as CaptureContext, GraphicsCaptureApiHandler};
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

#[derive(Clone, Copy)]
struct MicrophoneSpec {
    sample_rate: u32,
    channels: u32,
}

struct MicrophoneInput {
    device: Device,
    config: StreamConfig,
    sample_format: SampleFormat,
    spec: MicrophoneSpec,
}

#[derive(Clone)]
struct GpuFlags {
    output_path: PathBuf,
    width: u32,
    height: u32,
    fps: u32,
    microphone: Option<MicrophoneSpec>,
}

struct GpuFrameEncoder {
    encoder: Option<VideoEncoder>,
    frame_interval_hns: i64,
    next_frame_hns: Option<i64>,
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

    fn send_microphone_pcm(&mut self, pcm: &[u8]) -> Result<(), String> {
        self.encoder
            .as_mut()
            .ok_or_else(|| "Native Windows encoder is already finalized".to_string())?
            .send_audio_buffer(pcm, 0)
            .map_err(|error| format!("Unable to submit microphone PCM to native Windows encoder: {error}"))
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

        let audio_settings = match ctx.flags.microphone {
            Some(mic) => AudioSettingsBuilder::new()
                .sample_rate(mic.sample_rate)
                .channel_count(mic.channels)
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

        // WGC can deliver at the desktop refresh rate. Decimate before the encoder
        // so a 120/144 Hz display does not create an unbounded queue for a 30/60 fps recording.
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

fn resolve_microphone_input(id: &str) -> Result<MicrophoneInput> {
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
    let device = selected
        .or(partial_match)
        .ok_or_else(|| anyhow!("The selected microphone is not available through Windows audio capture: {requested_name}"))?;
    let supported = device
        .default_input_config()
        .with_context(|| format!("Unable to read microphone format for {requested_name}"))?;
    let sample_format = supported.sample_format();
    let config: StreamConfig = supported.into();
    if config.channels == 0 || config.sample_rate.0 == 0 {
        return Err(anyhow!("The selected microphone reported an invalid audio format"));
    }

    Ok(MicrophoneInput {
        device,
        spec: MicrophoneSpec {
            sample_rate: config.sample_rate.0,
            channels: u32::from(config.channels),
        },
        config,
        sample_format,
    })
}

fn encode_pcm_i16<T: Copy>(samples: &[T], convert: impl Fn(T) -> i16) -> Vec<u8> {
    let mut pcm = Vec::with_capacity(samples.len() * 2);
    for &sample in samples {
        pcm.extend_from_slice(&convert(sample).to_le_bytes());
    }
    pcm
}

fn submit_microphone_pcm<T: Copy>(
    callback: &Arc<Mutex<GpuFrameEncoder>>,
    error_state: &Arc<Mutex<Option<String>>>,
    samples: &[T],
    convert: impl Fn(T) -> i16,
) {
    if error_state.lock().is_some() {
        return;
    }
    let pcm = encode_pcm_i16(samples, convert);
    if let Err(error) = callback.lock().send_microphone_pcm(&pcm) {
        *error_state.lock() = Some(error);
    }
}

struct NativeMicrophoneCapture {
    stream: Stream,
    error_state: Arc<Mutex<Option<String>>>,
}

// CPAL's WASAPI stream is owned and stopped exclusively by the recording
// session. It never escapes the recorder state except behind this wrapper.
unsafe impl Send for NativeMicrophoneCapture {}

impl NativeMicrophoneCapture {
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

fn start_microphone_capture(
    input: MicrophoneInput,
    callback: Arc<Mutex<GpuFrameEncoder>>,
) -> Result<NativeMicrophoneCapture> {
    let error_state = Arc::new(Mutex::new(None::<String>));
    let stream_error_state = error_state.clone();
    let error_callback = move |error| {
        eprintln!("Native microphone stream error: {error}");
        let mut state = stream_error_state.lock();
        if state.is_none() {
            *state = Some(format!("Native microphone stream error: {error}"));
        }
    };

    let stream = match input.sample_format {
        SampleFormat::F32 => {
            let sink = callback.clone();
            let errors = error_state.clone();
            input.device.build_input_stream(
                &input.config,
                move |data: &[f32], _| {
                    submit_microphone_pcm(&sink, &errors, data, |v| {
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
                move |data: &[i16], _| submit_microphone_pcm(&sink, &errors, data, |v| v),
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
                    submit_microphone_pcm(&sink, &errors, data, |v| (i32::from(v) - 32_768) as i16)
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
                move |data: &[i32], _| submit_microphone_pcm(&sink, &errors, data, |v| (v >> 16) as i16),
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
                    submit_microphone_pcm(&sink, &errors, data, |v| {
                        (v.clamp(-1.0, 1.0) * i16::MAX as f64).round() as i16
                    })
                },
                error_callback,
                None,
            )
        }
        other => return Err(anyhow!("Unsupported native microphone sample format: {other:?}")),
    }
    .context("Unable to create native Windows microphone stream")?;

    stream
        .play()
        .context("Unable to start native Windows microphone capture")?;
    Ok(NativeMicrophoneCapture { stream, error_state })
}

pub struct NativeGpuVideoCapture {
    control: Option<CaptureControl<GpuFrameEncoder, String>>,
    microphone: Option<NativeMicrophoneCapture>,
}

impl NativeGpuVideoCapture {
    pub fn stop(mut self) -> Result<()> {
        // Stop microphone callbacks first so no audio can race with encoder finalization.
        let microphone_result = match self.microphone.take() {
            Some(microphone) => microphone.stop(),
            None => Ok(()),
        };

        let Some(control) = self.control.take() else {
            microphone_result?;
            return Ok(());
        };

        // Keep a handle to the encoder state, stop WGC so no more video frames can arrive,
        // then flush/finalize the MediaTranscoder-backed MP4 encoder.
        let callback = control.callback();
        let capture_result = control
            .stop()
            .map_err(|error| anyhow!("Unable to stop Windows Graphics Capture: {error}"));
        let finish_result = callback.lock().finish().map_err(anyhow::Error::msg);

        microphone_result?;
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
    let microphone_spec = microphone_input.as_ref().map(|input| input.spec);
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
            microphone: microphone_spec,
        },
    );

    let control = GpuFrameEncoder::start_free_threaded(settings)
        .map_err(|error| anyhow!("Unable to start native WGC/D3D11 encoder: {error}"))?;

    let microphone = if let Some(input) = microphone_input {
        let callback = control.callback();
        match start_microphone_capture(input, callback.clone()) {
            Ok(microphone) => Some(microphone),
            Err(error) => {
                let _ = control.stop();
                let _ = callback.lock().finish();
                return Err(error.context("Unable to attach microphone to native Windows encoder"));
            }
        }
    } else {
        None
    };

    Ok(NativeGpuVideoCapture {
        control: Some(control),
        microphone,
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
