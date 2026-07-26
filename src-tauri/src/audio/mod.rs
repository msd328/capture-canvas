//! Audio capture helpers.
//!
//! Microphone enumeration uses Windows device APIs. Microphone recording still
//! uses DirectShow in the current encoder process. Windows system audio is now
//! captured natively from the default render endpoint through CPAL/WASAPI into
//! a temporary floating-point WAV track, which the recording finalizer mixes
//! into the MP4. This removes the old Stereo Mix / What U Hear requirement.

use crate::recording::types::MicrophoneInfo;
use anyhow::{anyhow, Context, Result};
use std::path::Path;

#[cfg(windows)]
mod windows_backend {
    use super::{MicrophoneInfo, Path, Result};
    use anyhow::{anyhow, Context};
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use cpal::{SampleFormat, Stream, StreamConfig};
    use hound::{SampleFormat as WavSampleFormat, WavSpec, WavWriter};
    use parking_lot::Mutex;
    use std::fs::File;
    use std::io::BufWriter;
    use std::sync::Arc;
    use windows::Devices::Enumeration::DeviceInformation;
    use windows::Media::Devices::{AudioDeviceRole, MediaDevice};

    type SharedWriter = Arc<Mutex<Option<WavWriter<BufWriter<File>>>>>;

    pub fn enumerate_microphones() -> Vec<MicrophoneInfo> {
        let selector = match MediaDevice::GetAudioCaptureSelector() {
            Ok(selector) => selector,
            Err(_) => return Vec::new(),
        };
        let default_id = MediaDevice::GetDefaultAudioCaptureId(AudioDeviceRole::Default)
            .ok()
            .map(|id| id.to_string());
        let operation = match DeviceInformation::FindAllAsyncAqsFilter(&selector) {
            Ok(operation) => operation,
            Err(_) => return Vec::new(),
        };
        let devices = match operation.get() {
            Ok(devices) => devices,
            Err(_) => return Vec::new(),
        };
        let size = devices.Size().unwrap_or(0);
        let mut microphones = Vec::with_capacity(size as usize);
        for index in 0..size {
            let Ok(device) = devices.GetAt(index) else { continue; };
            let Ok(id) = device.Id() else { continue; };
            let Ok(name) = device.Name() else { continue; };
            let id = id.to_string();
            microphones.push(MicrophoneInfo {
                is_default: default_id.as_deref() == Some(id.as_str()),
                id,
                name: name.to_string(),
            });
        }
        if !microphones.iter().any(|mic| mic.is_default) {
            if let Some(first) = microphones.first_mut() {
                first.is_default = true;
            }
        }
        microphones
    }

    pub fn system_audio_supported() -> bool {
        let host = cpal::default_host();
        host.default_output_device()
            .and_then(|device| device.default_output_config().ok())
            .is_some()
    }

    /// A live WASAPI loopback stream and the WAV writer receiving its PCM.
    /// The stream is deliberately kept alive for the whole recording segment.
    pub struct SystemAudioCapture {
        stream: Stream,
        writer: SharedWriter,
    }

    // CPAL's Windows stream is owned and stopped exclusively by the recording
    // engine thread/state. This mirrors the ownership model used by Cap's
    // permissively licensed scap-cpal Windows capturer.
    unsafe impl Send for SystemAudioCapture {}

    impl SystemAudioCapture {
        pub fn stop(self) -> Result<()> {
            let Self { stream, writer } = self;
            let _ = stream.pause();
            drop(stream);
            if let Some(writer) = writer.lock().take() {
                writer.finalize().context("Unable to finalize system-audio WAV")?;
            }
            Ok(())
        }
    }

    fn wav_spec(config: &StreamConfig) -> WavSpec {
        WavSpec {
            channels: config.channels,
            sample_rate: config.sample_rate.0,
            bits_per_sample: 32,
            sample_format: WavSampleFormat::Float,
        }
    }

    fn write_f32(writer: &SharedWriter, samples: impl IntoIterator<Item = f32>) {
        let mut guard = writer.lock();
        let Some(writer) = guard.as_mut() else { return; };
        for sample in samples {
            if writer.write_sample(sample.clamp(-1.0, 1.0)).is_err() {
                break;
            }
        }
    }

    pub fn start_system_audio_capture(path: &Path) -> Result<SystemAudioCapture> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| anyhow!("Windows has no default audio output device"))?;
        let supported = device
            .default_output_config()
            .context("Unable to read the Windows default output format")?;
        let sample_format = supported.sample_format();
        let config: StreamConfig = supported.into();

        let writer = WavWriter::create(path, wav_spec(&config))
            .with_context(|| format!("Unable to create system-audio track {}", path.display()))?;
        let writer: SharedWriter = Arc::new(Mutex::new(Some(writer)));
        let error_callback = |error| eprintln!("WASAPI loopback stream error: {error}");

        let stream = match sample_format {
            SampleFormat::F32 => {
                let sink = writer.clone();
                device.build_input_stream(
                    &config,
                    move |data: &[f32], _| write_f32(&sink, data.iter().copied()),
                    error_callback,
                    None,
                )
            }
            SampleFormat::I16 => {
                let sink = writer.clone();
                device.build_input_stream(
                    &config,
                    move |data: &[i16], _| {
                        write_f32(&sink, data.iter().map(|&v| v as f32 / i16::MAX as f32))
                    },
                    error_callback,
                    None,
                )
            }
            SampleFormat::U16 => {
                let sink = writer.clone();
                device.build_input_stream(
                    &config,
                    move |data: &[u16], _| {
                        write_f32(&sink, data.iter().map(|&v| (v as f32 / u16::MAX as f32) * 2.0 - 1.0))
                    },
                    error_callback,
                    None,
                )
            }
            SampleFormat::I32 => {
                let sink = writer.clone();
                device.build_input_stream(
                    &config,
                    move |data: &[i32], _| {
                        write_f32(&sink, data.iter().map(|&v| v as f32 / i32::MAX as f32))
                    },
                    error_callback,
                    None,
                )
            }
            SampleFormat::F64 => {
                let sink = writer.clone();
                device.build_input_stream(
                    &config,
                    move |data: &[f64], _| write_f32(&sink, data.iter().map(|&v| v as f32)),
                    error_callback,
                    None,
                )
            }
            other => return Err(anyhow!("Unsupported Windows loopback sample format: {other:?}")),
        }
        .context("Unable to create WASAPI loopback input stream")?;

        stream.play().context("Unable to start WASAPI loopback capture")?;
        Ok(SystemAudioCapture { stream, writer })
    }
}

pub fn enumerate_microphones() -> Vec<MicrophoneInfo> {
    #[cfg(windows)]
    { return windows_backend::enumerate_microphones(); }
    #[cfg(not(windows))]
    { Vec::new() }
}

pub fn resolve_microphone_name(id: &str) -> Option<String> {
    enumerate_microphones()
        .into_iter()
        .find(|device| device.id == id)
        .map(|device| device.name)
}

pub fn system_audio_supported() -> bool {
    #[cfg(windows)]
    { return windows_backend::system_audio_supported(); }
    #[cfg(not(windows))]
    { false }
}

#[cfg(windows)]
pub use windows_backend::SystemAudioCapture;

#[cfg(windows)]
pub fn start_system_audio_capture(path: &Path) -> Result<SystemAudioCapture> {
    windows_backend::start_system_audio_capture(path)
}

#[cfg(not(windows))]
pub struct SystemAudioCapture;

#[cfg(not(windows))]
pub fn start_system_audio_capture(_path: &Path) -> Result<SystemAudioCapture> {
    Err(anyhow!("System audio capture is not implemented on this operating system"))
}
