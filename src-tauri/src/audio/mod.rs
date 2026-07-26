//! Audio capture — microphone and (optionally) system audio.
//!
//! Phase 1 currently exposes real Windows microphone enumeration. Actual PCM
//! capture will be added to the recording pipeline later without changing the
//! frontend command contract.

use crate::recording::types::MicrophoneInfo;

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
            let Ok(device) = devices.GetAt(index) else {
                continue;
            };
            let Ok(id) = device.Id() else {
                continue;
            };
            let Ok(name) = device.Name() else {
                continue;
            };

            let id = id.to_string();
            microphones.push(MicrophoneInfo {
                is_default: default_id.as_deref() == Some(id.as_str()),
                id,
                name: name.to_string(),
            });
        }

        // If Windows did not identify a default capture endpoint, still give
        // the UI a sensible initial selection.
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
    {
        return windows_backend::enumerate_microphones();
    }

    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

/// Returns true when system audio capture is planned/supported for the current OS.
/// The actual WASAPI loopback capture pipeline is a later Phase 1 milestone.
pub fn system_audio_supported() -> bool {
    cfg!(target_os = "windows")
}
