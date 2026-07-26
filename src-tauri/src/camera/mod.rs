//! Webcam enumeration and recording helpers.
//!
//! On Windows we prefer the exact DirectShow device names reported by FFmpeg,
//! because those are the names the recording process can actually open. This
//! avoids the previous mismatch between Windows MediaDevice ids and DirectShow.

use crate::recording::types::CameraInfo;
use anyhow::{anyhow, Context, Result};
use base64::Engine;
use std::process::Command;

fn parse_dshow_device_names(section_name: &str) -> Vec<String> {
    let Ok(output) = Command::new("ffmpeg")
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

pub fn enumerate_cameras() -> Vec<CameraInfo> {
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

pub fn resolve_camera_name(id: &str) -> Option<String> {
    if let Some(name) = id.strip_prefix("dshow-camera:") {
        return Some(name.to_string());
    }
    enumerate_cameras()
        .into_iter()
        .find(|device| device.id == id)
        .map(|device| device.name)
}

/// Capture one real JPEG frame from the selected DirectShow camera.
/// The frontend polls this only while the setup screen is idle; recording stops
/// polling before FFmpeg opens the camera for the actual recording session.
pub fn preview_frame_data_url(id: &str) -> Result<String> {
    let name = resolve_camera_name(id).ok_or_else(|| anyhow!("Selected camera is no longer available"))?;
    let input = format!("video={name}");
    let output = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "dshow",
            "-i",
            &input,
            "-frames:v",
            "1",
            "-vf",
            "scale=320:-2",
            "-f",
            "image2pipe",
            "-vcodec",
            "mjpeg",
            "pipe:1",
        ])
        .output()
        .context("Unable to start camera preview")?;

    if !output.status.success() || output.stdout.is_empty() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(anyhow!(if detail.is_empty() {
            "The selected camera could not provide a preview frame".to_string()
        } else {
            format!("Camera preview failed: {detail}")
        }));
    }

    let encoded = base64::engine::general_purpose::STANDARD.encode(output.stdout);
    Ok(format!("data:image/jpeg;base64,{encoded}"))
}
