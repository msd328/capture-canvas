//! Webcam enumeration and recording helpers.
//!
//! Windows camera enumeration uses MediaDevice. For the first working native
//! recorder, the selected camera is opened by FFmpeg/DirectShow and composited
//! over the captured screen. A dedicated low-latency preview stream is separate.

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
            let Ok(device) = devices.GetAt(index) else { continue; };
            let Ok(id) = device.Id() else { continue; };
            let Ok(name) = device.Name() else { continue; };
            cameras.push(CameraInfo {
                id: id.to_string(),
                name: name.to_string(),
                is_default: cameras.is_empty(),
            });
        }
        cameras
    }
}

pub fn enumerate_cameras() -> Vec<CameraInfo> {
    #[cfg(windows)]
    { return windows_backend::enumerate_cameras(); }
    #[cfg(not(windows))]
    { Vec::new() }
}

pub fn resolve_camera_name(id: &str) -> Option<String> {
    enumerate_cameras()
        .into_iter()
        .find(|device| device.id == id)
        .map(|device| device.name)
}
