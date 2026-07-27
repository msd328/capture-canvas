//! Webcam enumeration and recording helpers.
//!
//! Windows camera discovery uses the Windows MediaDevice API so cameras still
//! appear in the UI even when FFmpeg's DirectShow diagnostic output differs by
//! build or locale. For native GPU screen recording, FFmpeg is used only as a
//! small 320x180 webcam frame source; the full screen never passes through it.

use crate::{encoding, recording::types::CameraInfo};

pub const OVERLAY_WIDTH: u32 = 320;
pub const OVERLAY_HEIGHT: u32 = 180;

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

#[cfg(windows)]
pub struct CameraFrameCapture {
    child: std::process::Child,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    reader_thread: Option<std::thread::JoinHandle<()>>,
    error_state: std::sync::Arc<parking_lot::Mutex<Option<String>>>,
}

#[cfg(windows)]
unsafe impl Send for CameraFrameCapture {}

#[cfg(windows)]
impl CameraFrameCapture {
    pub fn stop(mut self) -> anyhow::Result<()> {
        use std::sync::atomic::Ordering;

        self.stop.store(true, Ordering::Release);
        let _ = self.child.kill();
        if let Some(thread) = self.reader_thread.take() {
            let _ = thread.join();
        }
        let _ = self.child.wait();
        if let Some(error) = self.error_state.lock().take() {
            return Err(anyhow::anyhow!(error));
        }
        Ok(())
    }
}

#[cfg(windows)]
pub fn start_camera_frame_capture(
    id: &str,
    latest_frame: std::sync::Arc<parking_lot::Mutex<Option<Vec<u8>>>>,
) -> anyhow::Result<CameraFrameCapture> {
    use anyhow::{anyhow, Context};
    use std::io::Read;
    use std::process::Stdio;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    let name = resolve_camera_name(id)
        .ok_or_else(|| anyhow!("The selected camera is no longer available"))?;
    let frame_size = OVERLAY_WIDTH as usize * OVERLAY_HEIGHT as usize * 4;
    let filter = format!(
        "scale={OVERLAY_WIDTH}:{OVERLAY_HEIGHT}:force_original_aspect_ratio=decrease,pad={OVERLAY_WIDTH}:{OVERLAY_HEIGHT}:(ow-iw)/2:(oh-ih)/2,format=bgra"
    );

    let mut child = encoding::ffmpeg_command()
        .args([
            "-hide_banner",
            "-loglevel",
            "warning",
            "-f",
            "dshow",
            "-rtbufsize",
            "128M",
            "-i",
            &format!("video={name}"),
            "-an",
            "-vf",
            &filter,
            "-pix_fmt",
            "bgra",
            "-r",
            "30",
            "-f",
            "rawvideo",
            "pipe:1",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .context("Unable to start the camera frame source")?;

    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("Camera frame source stdout was not available"))?;
    let stop = Arc::new(AtomicBool::new(false));
    let error_state = Arc::new(parking_lot::Mutex::new(None::<String>));
    let reader_stop = stop.clone();
    let reader_errors = error_state.clone();

    let reader_thread = thread::spawn(move || {
        let mut frame = vec![0u8; frame_size];
        while !reader_stop.load(Ordering::Acquire) {
            match stdout.read_exact(&mut frame) {
                Ok(()) => {
                    *latest_frame.lock() = Some(frame.clone());
                }
                Err(error) => {
                    if !reader_stop.load(Ordering::Acquire)
                        && error.kind() != std::io::ErrorKind::UnexpectedEof
                    {
                        *reader_errors.lock() = Some(format!(
                            "Camera frame source stopped while reading frames: {error}"
                        ));
                    }
                    break;
                }
            }
        }
    });

    thread::sleep(Duration::from_millis(250));
    if let Some(status) = child
        .try_wait()
        .context("Unable to inspect the camera frame source")?
    {
        stop.store(true, Ordering::Release);
        let _ = reader_thread.join();
        return Err(anyhow!(
            "Camera frame source exited during startup (exit code {})",
            status
                .code()
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        ));
    }

    Ok(CameraFrameCapture {
        child,
        stop,
        reader_thread: Some(reader_thread),
        error_state,
    })
}
