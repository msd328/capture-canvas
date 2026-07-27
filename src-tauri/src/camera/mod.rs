//! Webcam enumeration and recording helpers.
//!
//! Windows camera discovery uses the Windows MediaDevice API so cameras still
//! appear in the UI even when FFmpeg's DirectShow diagnostic output differs by
//! build or locale. When recording, we map the Windows friendly name to the
//! DirectShow device list exposed by the installed FFmpeg build.

use crate::{encoding, recording::types::CameraInfo};

fn parse_dshow_device_names(section_name: &str) -> Vec<String> {
    let Ok(output) = encoding::ffmpeg_command()
        .args(["-hide_banner", "-list_devices", "true", "-f", "dshow", "-i", "dummy"])
        .output()
    else {
        return Vec::new();
    };

    let text = String::from_utf8_lossy(&output.stderr);
    let mut in_section = false;
    let mut names = Vec::new();

    for line in text.lines() {
        if line.contains("DirectShow video devices") {
            in_section = section_name == "video";
            continue;
        }
        if line.contains("DirectShow audio devices") {
            in_section = section_name == "audio";
            continue;
        }
        if !in_section || line.contains("Alternative name") {
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

#[cfg(windows)]
fn windows_cameras() -> Vec<CameraInfo> {
    use windows::Devices::Enumeration::DeviceInformation;
    use windows::Media::Devices::MediaDevice;

    let Ok(selector) = MediaDevice::GetVideoCaptureSelector() else { return Vec::new(); };
    let Ok(operation) = DeviceInformation::FindAllAsyncAqsFilter(&selector) else { return Vec::new(); };
    let Ok(devices) = operation.get() else { return Vec::new(); };
    let size = devices.Size().unwrap_or(0);
    let mut cameras = Vec::with_capacity(size as usize);
    for index in 0..size {
        let Ok(device) = devices.GetAt(index) else { continue; };
        let Ok(id) = device.Id() else { continue; };
        let Ok(name) = device.Name() else { continue; };
        cameras.push(CameraInfo {
            id: format!("windows-camera:{}", id),
            name: name.to_string(),
            is_default: index == 0,
        });
    }
    cameras
}

pub fn enumerate_cameras() -> Vec<CameraInfo> {
    #[cfg(windows)]
    {
        let native = windows_cameras();
        if !native.is_empty() {
            return native;
        }
    }

    parse_dshow_device_names("video")
        .into_iter()
        .enumerate()
        .map(|(index, name)| CameraInfo {
            id: format!("dshow-camera:{name}"),
            name,
            is_default: index == 0,
        })
        .collect()
}

fn normalize(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn resolve_camera_name(id: &str) -> Option<String> {
    if let Some(name) = id.strip_prefix("dshow-camera:") {
        return Some(name.to_string());
    }

    let selected = enumerate_cameras().into_iter().find(|device| device.id == id)?;
    let wanted = normalize(&selected.name);
    let dshow = parse_dshow_device_names("video");
    dshow
        .into_iter()
        .find(|name| {
            let candidate = normalize(name);
            candidate == wanted || candidate.contains(&wanted) || wanted.contains(&candidate)
        })
        .or(Some(selected.name))
}
