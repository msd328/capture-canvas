use crate::{
    audio, camera, encoding,
    recording::types::{CaptureKind, CaptureTarget, CropRegion, RecordingConfig},
};
use anyhow::{anyhow, Context, Result};
use parking_lot::Mutex;
use std::ffi::c_void;
use std::io::Write;
use std::path::Path;
use std::process::{Child, ChildStdin, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use windows_capture::capture::{CaptureControl, Context as CaptureContext, GraphicsCaptureApiHandler};
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    GraphicsCaptureItemType, MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};
use windows_capture::window::Window;

/// Latest-frame exchange between the WGC callback and the FFmpeg writer.
///
/// The callback publishes frames as fast as Windows produces them. The writer
/// consumes only the newest frame at the requested recording FPS. Swapping the
/// Vecs keeps the hot path allocation-free after the first few frames and,
/// importantly, prevents a slow FFmpeg pipe from blocking Windows capture.
#[derive(Default)]
struct FrameSlot {
    frame: Vec<u8>,
    generation: u64,
}

#[derive(Clone)]
struct PipeFlags {
    slot: Arc<Mutex<FrameSlot>>,
    width: u32,
    height: u32,
    crop_region: Option<CropRegion>,
}

struct FramePipe {
    slot: Arc<Mutex<FrameSlot>>,
    width: u32,
    height: u32,
    crop_region: Option<CropRegion>,
    scratch: Vec<u8>,
    next_frame: Vec<u8>,
}

impl GraphicsCaptureApiHandler for FramePipe {
    type Flags = PipeFlags;
    type Error = String;

    fn new(ctx: CaptureContext<Self::Flags>) -> Result<Self, Self::Error> {
        Ok(Self {
            slot: ctx.flags.slot,
            width: ctx.flags.width,
            height: ctx.flags.height,
            crop_region: ctx.flags.crop_region,
            scratch: Vec::new(),
            next_frame: Vec::new(),
        })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        _capture_control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        let frame_width = frame.width();
        let frame_height = frame.height();
        let buffer = frame
            .buffer()
            .map_err(|e| format!("Unable to map WGC frame: {e}"))?;

        self.scratch.clear();
        let bytes = buffer.as_nopadding_buffer(&mut self.scratch);
        let source_expected = frame_width as usize * frame_height as usize * 4;
        if bytes.len() != source_expected {
            return Err(format!(
                "Unexpected WGC frame size: got {}, expected {source_expected}",
                bytes.len()
            ));
        }

        self.next_frame.clear();
        if let Some(crop) = self.crop_region {
            let right = crop
                .x
                .checked_add(crop.width)
                .ok_or_else(|| "Selected crop rectangle overflowed horizontally".to_string())?;
            let bottom = crop
                .y
                .checked_add(crop.height)
                .ok_or_else(|| "Selected crop rectangle overflowed vertically".to_string())?;
            if right > frame_width || bottom > frame_height {
                return Err(format!(
                    "The selected crop area is outside the current source frame (crop {}x{} at {},{}; source {}x{})",
                    crop.width, crop.height, crop.x, crop.y, frame_width, frame_height
                ));
            }
            if crop.width != self.width || crop.height != self.height {
                return Err(format!(
                    "Crop output size changed unexpectedly ({}x{} -> {}x{})",
                    crop.width, crop.height, self.width, self.height
                ));
            }

            let output_len = self.width as usize * self.height as usize * 4;
            self.next_frame.resize(output_len, 0);
            let row_bytes = self.width as usize * 4;
            for row in 0..self.height as usize {
                let src_start =
                    (((crop.y as usize + row) * frame_width as usize) + crop.x as usize) * 4;
                let dst_start = row * row_bytes;
                self.next_frame[dst_start..dst_start + row_bytes]
                    .copy_from_slice(&bytes[src_start..src_start + row_bytes]);
            }
        } else if frame_width == self.width && frame_height == self.height {
            self.next_frame.extend_from_slice(bytes);
        } else {
            // FFmpeg rawvideo dimensions are fixed for a segment. WGC can resize
            // a window while recording, so normalize changed frames into the
            // fixed encoder canvas. Larger frames are centre-cropped and smaller
            // frames are centre-letterboxed with black.
            let output_len = self.width as usize * self.height as usize * 4;
            self.next_frame.resize(output_len, 0);

            let copy_width = frame_width.min(self.width) as usize;
            let copy_height = frame_height.min(self.height) as usize;
            let src_x = ((frame_width as usize).saturating_sub(copy_width)) / 2;
            let src_y = ((frame_height as usize).saturating_sub(copy_height)) / 2;
            let dst_x = ((self.width as usize).saturating_sub(copy_width)) / 2;
            let dst_y = ((self.height as usize).saturating_sub(copy_height)) / 2;
            let row_bytes = copy_width * 4;

            for row in 0..copy_height {
                let src_start = ((src_y + row) * frame_width as usize + src_x) * 4;
                let dst_start = ((dst_y + row) * self.width as usize + dst_x) * 4;
                self.next_frame[dst_start..dst_start + row_bytes]
                    .copy_from_slice(&bytes[src_start..src_start + row_bytes]);
            }
        }

        // Publish the newest complete frame with a cheap Vec swap. Do not write
        // to FFmpeg here: a blocking pipe write would stall WGC and cause motion
        // capture to degrade into large bursts of repeated frames.
        let mut slot = self.slot.lock();
        std::mem::swap(&mut slot.frame, &mut self.next_frame);
        slot.generation = slot.generation.wrapping_add(1);
        Ok(())
    }

    fn on_closed(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

fn spawn_frame_writer(
    mut stdin: ChildStdin,
    slot: Arc<Mutex<FrameSlot>>,
    stop: Arc<AtomicBool>,
    fps: u32,
) -> JoinHandle<Result<(), String>> {
    thread::spawn(move || {
        let fps = fps.clamp(1, 60);
        let frame_interval = Duration::from_secs_f64(1.0 / fps as f64);
        let mut next_tick = Instant::now();
        let mut current_frame = Vec::<u8>::new();
        let mut seen_generation = 0u64;

        loop {
            let now = Instant::now();
            if now < next_tick {
                thread::sleep(next_tick - now);
            }
            if stop.load(Ordering::Acquire) {
                break;
            }

            // Swap in the latest WGC frame only when a newer one exists. If the
            // captured content is static, keep writing the last frame at a steady
            // CFR cadence so real elapsed duration is preserved.
            {
                let mut latest = slot.lock();
                if latest.generation != seen_generation {
                    std::mem::swap(&mut current_frame, &mut latest.frame);
                    seen_generation = latest.generation;
                }
            }

            if !current_frame.is_empty() {
                stdin
                    .write_all(&current_frame)
                    .map_err(|e| format!("Unable to pipe paced WGC frame to FFmpeg: {e}"))?;
            }

            next_tick += frame_interval;
            let after_write = Instant::now();
            if next_tick <= after_write {
                // Never try to catch up by dumping a burst of old frames. A late
                // encoder tick should simply restart cadence from the current
                // time; the next tick will use the freshest WGC frame available.
                next_tick = after_write + frame_interval;
            }
        }

        // Dropping stdin sends EOF to FFmpeg so it can flush the encoder and MP4
        // trailer normally.
        drop(stdin);
        Ok(())
    })
}

pub struct NativeVideoCapture {
    control: Option<CaptureControl<FramePipe, String>>,
    writer_stop: Arc<AtomicBool>,
    writer_thread: Option<JoinHandle<Result<(), String>>>,
    child: Child,
}

impl NativeVideoCapture {
    pub fn stop(mut self) -> Result<()> {
        let capture_result = if let Some(control) = self.control.take() {
            control
                .stop()
                .map_err(|e| anyhow!("Unable to stop Windows Graphics Capture: {e}"))
        } else {
            Ok(())
        };

        self.writer_stop.store(true, Ordering::Release);
        let writer_result = match self.writer_thread.take() {
            Some(handle) => match handle.join() {
                Ok(result) => result.map_err(|e| anyhow!(e)),
                Err(_) => Err(anyhow!("WGC frame writer thread terminated unexpectedly")),
            },
            None => Ok(()),
        };

        let status = self
            .child
            .wait()
            .context("Unable to finalize WGC recording segment")?;

        capture_result?;
        writer_result?;
        if !status.success() {
            return Err(anyhow!("FFmpeg could not finalize the WGC recording segment"));
        }
        Ok(())
    }
}

fn spawn_ffmpeg(
    config: &RecordingConfig,
    width: u32,
    height: u32,
    output_path: &Path,
) -> Result<(Child, ChildStdin)> {
    let fps = config.fps.clamp(1, 60).to_string();
    let size = format!("{width}x{height}");
    let mut cmd = encoding::ffmpeg_command();
    cmd.args(["-hide_banner", "-loglevel", "warning", "-y"]);
    cmd.args([
        "-thread_queue_size",
        "1024",
        "-f",
        "rawvideo",
        "-pixel_format",
        "bgra",
        "-video_size",
        &size,
        "-framerate",
        &fps,
        "-i",
        "pipe:0",
    ]);

    let mut next_input = 1usize;
    let mic_input = if let Some(id) = config.microphone_id.as_deref() {
        let name = audio::resolve_microphone_name(id)
            .ok_or_else(|| anyhow!("The selected microphone is no longer available"))?;
        cmd.args([
            "-thread_queue_size",
            "1024",
            "-f",
            "dshow",
            "-i",
            &format!("audio={name}"),
        ]);
        let index = next_input;
        next_input += 1;
        Some(index)
    } else {
        None
    };

    let camera_input = if let Some(id) = config.camera_id.as_deref() {
        let name = camera::resolve_camera_name(id)
            .ok_or_else(|| anyhow!("The selected camera is no longer available"))?;
        cmd.args([
            "-thread_queue_size",
            "1024",
            "-f",
            "dshow",
            "-i",
            &format!("video={name}"),
        ]);
        Some(next_input)
    } else {
        None
    };

    if let Some(index) = camera_input {
        let filter = format!(
            "[0:v]scale=trunc(iw/2)*2:trunc(ih/2)*2[base];[{index}:v]scale=320:-2[cam];[base][cam]overlay=W-w-24:H-h-24[v]"
        );
        cmd.args(["-filter_complex", &filter, "-map", "[v]"]);
    } else {
        cmd.args([
            "-vf",
            "scale=trunc(iw/2)*2:trunc(ih/2)*2",
            "-map",
            "0:v:0",
        ]);
    }

    if let Some(index) = mic_input {
        cmd.args([
            "-map",
            &format!("{index}:a:0"),
            "-c:a",
            "aac",
            "-b:a",
            "160k",
        ]);
    } else {
        cmd.arg("-an");
    }

    let encoder = encoding::selected_h264_encoder();
    encoding::apply_h264_options(&mut cmd, encoder);
    cmd.args(["-r", &fps, "-movflags", "+faststart"]);
    cmd.arg(output_path);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());

    let mut child = cmd
        .spawn()
        .context("Unable to start FFmpeg for Windows Graphics Capture")?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("FFmpeg rawvideo stdin was not available"))?;

    thread::sleep(Duration::from_millis(250));
    if let Some(status) = child
        .try_wait()
        .context("Unable to inspect FFmpeg process")?
    {
        return Err(anyhow!(
            "FFmpeg stopped while starting WGC recording (exit code {})",
            status
                .code()
                .map(|v| v.to_string())
                .unwrap_or_else(|| "unknown".into())
        ));
    }
    Ok((child, stdin))
}

fn start_item<T>(
    item: T,
    config: &RecordingConfig,
    width: u32,
    height: u32,
    output_path: &Path,
) -> Result<NativeVideoCapture>
where
    T: TryInto<GraphicsCaptureItemType> + Send + 'static,
{
    let (child, stdin) = spawn_ffmpeg(config, width, height, output_path)?;
    let fps = config.fps.clamp(1, 60);
    let slot = Arc::new(Mutex::new(FrameSlot::default()));
    let writer_stop = Arc::new(AtomicBool::new(false));
    let writer_thread = spawn_frame_writer(stdin, slot.clone(), writer_stop.clone(), fps);

    // Use platform defaults for optional WGC session properties. Border, cursor
    // and minimum-update-interval toggles are version-gated Windows APIs. The
    // dedicated writer thread above owns CFR pacing independently of WGC timing.
    let settings = Settings::new(
        item,
        CursorCaptureSettings::Default,
        DrawBorderSettings::Default,
        SecondaryWindowSettings::Default,
        MinimumUpdateIntervalSettings::Default,
        DirtyRegionSettings::Default,
        ColorFormat::Bgra8,
        PipeFlags {
            slot,
            width,
            height,
            crop_region: config.crop_region,
        },
    );

    let control = match FramePipe::start_free_threaded(settings) {
        Ok(control) => control,
        Err(error) => {
            writer_stop.store(true, Ordering::Release);
            let _ = writer_thread.join();
            let mut child = child;
            let _ = child.kill();
            let _ = child.wait();
            return Err(anyhow!("Unable to start Windows Graphics Capture: {error}"));
        }
    };

    Ok(NativeVideoCapture {
        control: Some(control),
        writer_stop,
        writer_thread: Some(writer_thread),
        child,
    })
}

pub fn start_native_video_capture(
    config: &RecordingConfig,
    target: &CaptureTarget,
    width: u32,
    height: u32,
    output_path: &Path,
) -> Result<NativeVideoCapture> {
    match target.kind {
        CaptureKind::Display => {
            let raw = target
                .id
                .strip_prefix("monitor-")
                .ok_or_else(|| anyhow!("Invalid monitor id"))?;
            let handle =
                usize::from_str_radix(raw, 16).map_err(|_| anyhow!("Invalid monitor id"))?;
            let monitor = Monitor::from_raw_hmonitor(handle as *mut c_void);
            start_item(monitor, config, width, height, output_path)
        }
        CaptureKind::Window => {
            let raw = target
                .id
                .strip_prefix("window-")
                .ok_or_else(|| anyhow!("Invalid window id"))?;
            let handle =
                usize::from_str_radix(raw, 16).map_err(|_| anyhow!("Invalid window id"))?;
            let window = Window::from_raw_hwnd(handle as *mut c_void);
            if !window.is_valid() {
                return Err(anyhow!("Selected window is no longer capturable"));
            }
            start_item(window, config, width, height, output_path)
        }
    }
}
