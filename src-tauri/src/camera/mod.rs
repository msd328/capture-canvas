//! Webcam enumeration and recording helpers.
//!
//! Windows camera discovery and recording use Windows Runtime media APIs. The
//! recorder requests a small 320x180 BGRA frame stream and uploads those frames
//! into the persistent D3D11 overlay texture owned by the screen encoder. A small
//! FFmpeg/DirectShow source remains only as a compatibility fallback for cameras
//! rejected by MediaCapture.

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

    let Ok(selector) = MediaDevice::GetVideoCaptureSelector() else {
        return Vec::new();
    };
    let Ok(operation) = DeviceInformation::FindAllAsyncAqsFilter(&selector) else {
        return Vec::new();
    };
    let Ok(devices) = operation.get() else {
        return Vec::new();
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
fn resolve_windows_camera_id(id: &str) -> Option<String> {
    if let Some(device_id) = id.strip_prefix("windows-camera:") {
        return Some(device_id.to_string());
    }

    let requested_name = id.strip_prefix("dshow-camera:")?;
    let wanted = normalize(requested_name);
    windows_cameras()
        .into_iter()
        .find(|camera| {
            let candidate = normalize(&camera.name);
            candidate == wanted || candidate.contains(&wanted) || wanted.contains(&candidate)
        })
        .and_then(|camera| {
            camera
                .id
                .strip_prefix("windows-camera:")
                .map(ToOwned::to_owned)
        })
}

#[cfg(windows)]
enum CameraFrameBackend {
    Native {
        capture: windows::Media::Capture::MediaCapture,
        reader: windows::Media::Capture::Frames::MediaFrameReader,
        frame_token: i64,
    },
    Ffmpeg {
        child: std::process::Child,
        stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
        reader_thread: Option<std::thread::JoinHandle<()>>,
    },
}

#[cfg(windows)]
pub struct CameraFrameCapture {
    backend: CameraFrameBackend,
    error_state: std::sync::Arc<parking_lot::Mutex<Option<String>>>,
}

#[cfg(windows)]
unsafe impl Send for CameraFrameCapture {}

#[cfg(windows)]
impl CameraFrameCapture {
    pub fn stop(mut self) -> anyhow::Result<()> {
        use std::sync::atomic::Ordering;

        match &mut self.backend {
            CameraFrameBackend::Native {
                capture,
                reader,
                frame_token,
            } => {
                let _ = reader.RemoveFrameArrived(*frame_token);
                let stop_result = reader
                    .StopAsync()
                    .and_then(|operation| operation.get())
                    .map_err(|error| anyhow::anyhow!(
                        "Unable to stop native Windows camera frame reader: {error}"
                    ));
                let _ = reader.Close();
                let _ = capture.Close();
                stop_result?;
            }
            CameraFrameBackend::Ffmpeg {
                child,
                stop,
                reader_thread,
            } => {
                stop.store(true, Ordering::Release);
                let _ = child.kill();
                if let Some(thread) = reader_thread.take() {
                    let _ = thread.join();
                }
                let _ = child.wait();
            }
        }

        if let Some(error) = self.error_state.lock().take() {
            return Err(anyhow::anyhow!(error));
        }
        Ok(())
    }
}

#[cfg(windows)]
fn copy_native_camera_frame(
    reader: &windows::Media::Capture::Frames::MediaFrameReader,
) -> anyhow::Result<Option<Vec<u8>>> {
    use anyhow::{anyhow, Context};
    use windows::Storage::Streams::{Buffer, DataReader};

    let frame_reference = match reader.TryAcquireLatestFrame() {
        Ok(frame) => frame,
        Err(_) => return Ok(None),
    };

    let result = (|| -> anyhow::Result<Option<Vec<u8>>> {
        let video_frame = match frame_reference.VideoMediaFrame() {
            Ok(frame) => frame,
            Err(_) => return Ok(None),
        };
        let bitmap = match video_frame.SoftwareBitmap() {
            Ok(bitmap) => bitmap,
            Err(_) => return Ok(None),
        };

        let expected = OVERLAY_WIDTH as usize * OVERLAY_HEIGHT as usize * 4;
        let buffer = Buffer::Create(expected as u32)
            .context("Unable to allocate Windows camera frame buffer")?;
        bitmap
            .CopyToBuffer(&buffer)
            .context("Unable to copy Windows camera frame to CPU buffer")?;

        let length = buffer
            .Length()
            .context("Unable to read Windows camera frame length")? as usize;
        if length != expected {
            let _ = bitmap.Close();
            return Err(anyhow!(
                "Windows camera returned an unexpected BGRA frame size: got {length}, expected {expected}"
            ));
        }

        let data_reader = DataReader::FromBuffer(&buffer)
            .context("Unable to create Windows camera frame reader")?;
        let mut bytes = vec![0u8; expected];
        data_reader
            .ReadBytes(&mut bytes)
            .context("Unable to read Windows camera BGRA pixels")?;
        let _ = data_reader.Close();
        let _ = bitmap.Close();
        Ok(Some(bytes))
    })();

    let _ = frame_reference.Close();
    result
}

#[cfg(windows)]
fn start_native_camera_frame_capture(
    id: &str,
    latest_frame: std::sync::Arc<parking_lot::Mutex<Option<Vec<u8>>>>,
) -> anyhow::Result<CameraFrameCapture> {
    use anyhow::{anyhow, Context};
    use std::sync::Arc;
    use windows::core::HSTRING;
    use windows::Foundation::TypedEventHandler;
    use windows::Graphics::Imaging::BitmapSize;
    use windows::Media::Capture::Frames::{
        MediaFrameArrivedEventArgs, MediaFrameReader, MediaFrameReaderAcquisitionMode,
        MediaFrameReaderStartStatus, MediaFrameSource, MediaFrameSourceKind,
    };
    use windows::Media::Capture::{
        MediaCapture, MediaCaptureInitializationSettings, MediaCaptureMemoryPreference,
        MediaCaptureSharingMode, StreamingCaptureMode,
    };
    use windows::Media::MediaProperties::MediaEncodingSubtypes;

    let device_id = resolve_windows_camera_id(id)
        .ok_or_else(|| anyhow!("The selected camera has no Windows MediaCapture device id"))?;

    let settings = MediaCaptureInitializationSettings::new()
        .context("Unable to create Windows camera initialization settings")?;
    settings
        .SetVideoDeviceId(&HSTRING::from(device_id))
        .context("Unable to select the Windows camera")?;
    settings
        .SetStreamingCaptureMode(StreamingCaptureMode::Video)
        .context("Unable to configure Windows camera video mode")?;
    settings
        .SetMemoryPreference(MediaCaptureMemoryPreference::Cpu)
        .context("Unable to configure Windows camera CPU frame access")?;
    settings
        .SetSharingMode(MediaCaptureSharingMode::SharedReadOnly)
        .context("Unable to configure shared Windows camera access")?;

    let capture = MediaCapture::new().context("Unable to create Windows MediaCapture")?;
    capture
        .InitializeWithSettingsAsync(&settings)
        .context("Unable to initialize Windows camera")?
        .get()
        .context("Windows camera initialization failed")?;

    let sources = capture
        .FrameSources()
        .context("Unable to enumerate Windows camera frame sources")?;
    let iterator = sources
        .First()
        .context("Unable to iterate Windows camera frame sources")?;
    let mut selected_source: Option<MediaFrameSource> = None;
    while iterator
        .HasCurrent()
        .context("Unable to inspect Windows camera frame source iterator")?
    {
        let pair = iterator
            .Current()
            .context("Unable to read Windows camera frame source")?;
        let source = pair
            .Value()
            .context("Unable to read Windows camera frame source value")?;
        if source
            .Info()
            .and_then(|info| info.SourceKind())
            .map(|kind| kind == MediaFrameSourceKind::Color)
            .unwrap_or(false)
        {
            selected_source = Some(source);
            break;
        }
        if !iterator
            .MoveNext()
            .context("Unable to advance Windows camera frame source iterator")?
        {
            break;
        }
    }

    let source = selected_source.ok_or_else(|| anyhow!(
        "The selected Windows camera did not expose a colour frame source"
    ))?;
    let subtype = MediaEncodingSubtypes::Bgra8()
        .context("Windows did not expose the BGRA8 media subtype")?;
    let output_size = BitmapSize {
        Width: OVERLAY_WIDTH,
        Height: OVERLAY_HEIGHT,
    };
    let reader = capture
        .CreateFrameReaderWithSubtypeAndSizeAsync(&source, &subtype, output_size)
        .context("Unable to create Windows camera frame reader")?
        .get()
        .context("Windows could not create the camera frame reader")?;
    reader
        .SetAcquisitionMode(MediaFrameReaderAcquisitionMode::Realtime)
        .context("Unable to set realtime Windows camera acquisition")?;

    let error_state = Arc::new(parking_lot::Mutex::new(None::<String>));
    let callback_errors = error_state.clone();
    let handler = TypedEventHandler::<MediaFrameReader, MediaFrameArrivedEventArgs>::new(
        move |sender, _| {
            if callback_errors.lock().is_some() {
                return Ok(());
            }
            let Some(reader) = sender.as_ref() else {
                return Ok(());
            };
            match copy_native_camera_frame(reader) {
                Ok(Some(frame)) => {
                    *latest_frame.lock() = Some(frame);
                }
                Ok(None) => {}
                Err(error) => {
                    let mut state = callback_errors.lock();
                    if state.is_none() {
                        *state = Some(error.to_string());
                    }
                }
            }
            Ok(())
        },
    );
    let frame_token = reader
        .FrameArrived(&handler)
        .context("Unable to subscribe to Windows camera frames")?;

    let start_status = reader
        .StartAsync()
        .context("Unable to start Windows camera frame reader")?
        .get()
        .context("Windows camera frame reader failed to start")?;
    if start_status != MediaFrameReaderStartStatus::Success {
        let _ = reader.RemoveFrameArrived(frame_token);
        let _ = reader.Close();
        let _ = capture.Close();
        return Err(anyhow!(
            "Windows camera frame reader did not start successfully: {start_status:?}"
        ));
    }

    eprintln!(
        "[Recorder] Native Windows MediaCapture webcam source active ({}x{} BGRA8)",
        OVERLAY_WIDTH, OVERLAY_HEIGHT
    );

    Ok(CameraFrameCapture {
        backend: CameraFrameBackend::Native {
            capture,
            reader,
            frame_token,
        },
        error_state,
    })
}

#[cfg(windows)]
fn start_ffmpeg_camera_frame_capture(
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
        .context("Unable to start the compatibility camera frame source")?;

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
        .context("Unable to inspect the compatibility camera frame source")?
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

    eprintln!("[Recorder] FFmpeg webcam compatibility source active");
    Ok(CameraFrameCapture {
        backend: CameraFrameBackend::Ffmpeg {
            child,
            stop,
            reader_thread: Some(reader_thread),
        },
        error_state,
    })
}

#[cfg(windows)]
pub fn start_camera_frame_capture(
    id: &str,
    latest_frame: std::sync::Arc<parking_lot::Mutex<Option<Vec<u8>>>>,
) -> anyhow::Result<CameraFrameCapture> {
    match start_native_camera_frame_capture(id, latest_frame.clone()) {
        Ok(capture) => Ok(capture),
        Err(native_error) => {
            eprintln!(
                "[Recorder] Native Windows webcam source unavailable ({native_error}); falling back to FFmpeg/DirectShow"
            );
            start_ffmpeg_camera_frame_capture(id, latest_frame).map_err(|fallback_error| {
                anyhow::anyhow!(
                    "Native Windows camera failed: {native_error}. Compatibility camera source also failed: {fallback_error}"
                )
            })
        }
    }
}
