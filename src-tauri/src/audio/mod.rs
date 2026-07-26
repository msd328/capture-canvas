//! Audio capture helpers.
//!
//! Microphone enumeration uses Windows device APIs. The FFmpeg recording path
//! currently opens microphones through DirectShow. Setup-screen metering uses
//! one persistent WebView2/Web Audio stream. For system audio we detect the
//! default Windows render endpoint through CPAL/WASAPI and retain the legacy
//! DirectShow loopback resolver only as a compatibility recording fallback.

use crate::recording::types::MicrophoneInfo;
use std::process::Command;

#[cfg(windows)]
mod windows_backend {
    use super::MicrophoneInfo;
    use cpal::traits::{DeviceTrait, HostTrait};
    use windows::Devices::Enumeration::DeviceInformation;
    use windows::Media::Devices::{AudioDeviceRole, MediaDevice};

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

    /// CPAL's default host is WASAPI on Windows. A usable default output device
    /// means Windows exposes a render endpoint that a native loopback capturer
    /// can open; unlike Stereo Mix this does not require a recording device.
    pub fn system_audio_supported() -> bool {
        let host = cpal::default_host();
        host.default_output_device()
            .and_then(|device| device.default_output_config().ok())
            .is_some()
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

fn dshow_audio_names() -> Vec<String> {
    let Ok(output) = Command::new("ffmpeg")
        .args(["-hide_banner", "-list_devices", "true", "-f", "dshow", "-i", "dummy"])
        .output()
    else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&output.stderr);
    let mut in_audio = false;
    let mut names = Vec::new();
    for line in text.lines() {
        if line.contains("DirectShow video devices") {
            in_audio = false;
            continue;
        }
        if line.contains("DirectShow audio devices") {
            in_audio = true;
            continue;
        }
        if !in_audio || line.contains("Alternative name") {
            continue;
        }
        if let Some(start) = line.find('"') {
            if let Some(end_rel) = line[start + 1..].find('"') {
                let name = line[start + 1..start + 1 + end_rel].trim();
                if !name.is_empty() && !names.iter().any(|n| n == name) {
                    names.push(name.to_string());
                }
            }
        }
    }
    names
}

/// Legacy FFmpeg/DirectShow loopback source. This remains usable while the
/// recording muxer is migrated to native WASAPI PCM, but it is no longer used
/// to decide whether the UI exposes the System Audio control.
pub fn resolve_system_audio_name() -> Option<String> {
    const LOOPBACK_HINTS: &[&str] = &[
        "stereo mix",
        "what u hear",
        "what you hear",
        "wave out mix",
        "loopback",
        "speaker mix",
    ];
    dshow_audio_names().into_iter().find(|name| {
        let lower = name.to_lowercase();
        LOOPBACK_HINTS.iter().any(|hint| lower.contains(hint))
    })
}

pub fn system_audio_supported() -> bool {
    #[cfg(windows)]
    { return windows_backend::system_audio_supported(); }
    #[cfg(not(windows))]
    { false }
}
