//! Audio capture helpers.
//!
//! Microphone enumeration uses Windows device APIs. The current FFmpeg-backed
//! recorder opens those devices through DirectShow. System audio is enabled only
//! when Windows exposes a loopback-style DirectShow endpoint such as Stereo Mix.

use crate::recording::types::MicrophoneInfo;
use anyhow::{anyhow, Context, Result};
use std::process::Command;

#[cfg(windows)]
mod windows_backend {
    use super::MicrophoneInfo;
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
    resolve_system_audio_name().is_some()
}

/// Sample a short slice from the microphone and convert FFmpeg's measured
/// mean-volume dB value into a normalized 0..1 meter value.
pub fn microphone_level(id: &str) -> Result<f32> {
    let name = resolve_microphone_name(id).ok_or_else(|| anyhow!("Selected microphone is no longer available"))?;
    let input = format!("audio={name}");
    let output = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "info",
            "-f",
            "dshow",
            "-i",
            &input,
            "-t",
            "0.20",
            "-af",
            "volumedetect",
            "-f",
            "null",
            "NUL",
        ])
        .output()
        .context("Unable to sample microphone level")?;

    let text = String::from_utf8_lossy(&output.stderr);
    let db = text
        .lines()
        .find_map(|line| {
            let marker = "mean_volume:";
            let pos = line.find(marker)?;
            let rest = line[pos + marker.len()..].trim();
            if rest.starts_with("-inf") {
                return Some(-90.0f32);
            }
            let value = rest.split_whitespace().next()?.parse::<f32>().ok()?;
            Some(value)
        })
        .unwrap_or(-90.0);

    Ok(10f32.powf(db / 20.0).clamp(0.0, 1.0))
}
