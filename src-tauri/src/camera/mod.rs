//! Webcam capture. Provides frames to the encoder and (optionally) a
//! low-latency preview stream to the frontend for the setup screen.
//!
//! Phase 1 currently exposes real Windows camera enumeration. Frame capture
//! and the live preview stream are separate milestones.

use crate::recording::types::CameraInfo;

#[cfg(windows)]
mod windows_backend {
    use super::CameraInfo;
    use windows::Devices::Enumeration::DeviceInformation;
    use windows::Media::Devices::MediaDevice;

    pub fn enumerate_cameras() -> Vec<CameraInfo> {
        let selector = match MediaDevice::GetVideoCaptureSelector() {
            Ok(selector) => selector,
            Err(_) => return Vec::new(),
        };

        let operation = match DeviceInformation::FindAllAsyncAqsFilter(&selector) {
            Ok(operation) => operation,
            Err(_) => return Vec::new(),
        };
        let devices = match operation.get() {
            Ok(devices) => devices,
            Err(_) => return Vec::new(),
        };

        let size = devices.Size().unwrap_or(0);
        let mut cameras = Vec::with_capacity(size as usize);

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

            cameras.push(CameraInfo {
                id: id.to_string(),
                name: name.to_string(),
                // Windows' video selector does not expose a single global
                // default camera. Use the first available device as the initial
                // selection; the user can choose another from the dropdown.
                is_default: cameras.is_empty(),
            });
        }

        cameras
    }
}

pub fn enumerate_cameras() -> Vec<CameraInfo> {
    #[cfg(windows)]
    {
        return windows_backend::enumerate_cameras();
    }

    #[cfg(not(windows))]
    {
        Vec::new()
    }
}
