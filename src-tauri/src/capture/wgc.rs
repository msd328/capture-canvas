use crate::{audio, camera, recording::types::{CaptureKind, CaptureTarget, RecordingConfig}};
use anyhow::{anyhow, Context, Result};
use parking_lot::Mutex;
use std::ffi::c_void;
use std::io::Write;
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Arc;
use std::thread;
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

#[derive(Clone)]
struct PipeFlags {
    writer: Arc<Mutex<Option<ChildStdin>>>,
    width: u32,
    height: u32,
    fps: u32,
}

struct FramePipe {
    writer: Arc<Mutex<Option<ChildStdin>>>,
    width: u32,
    height: u32,
    fps: u32,
    started_at: Instant,
    frames_written: u64,
    last_frame: Vec<u8>,
    scratch: Vec<u8>,
}

impl FramePipe {
    fn write_frame_copies(&mut self, frame: &[u8], target_count: u64) -> Result<(), String> {
        if target_count <= self.frames_written {
            return Ok(());
        }
        let mut guard = self.writer.lock();
        let Some(stdin) = guard.as_mut() else {
            return Ok(());
        };
        while self.frames_written < target_count {
            stdin.write_all(frame).map_err(|e| format!("Unable to pipe WGC frame to FFmpeg: {e}"))?;
            self.frames_written += 1;
        }
        Ok(())
    }

    fn target_frame_count(&self) -> u64 {
        let elapsed = self.started_at.elapsed().as_secs_f64();
        ((elapsed * self.fps as f64).floor() as u64).saturating_add(1)
    }

    fn pad_to_now(&mut self) -> Result<(), String> {
        if self.last_frame.is_empty() {
            return Ok(());
        }
        let target = self.target_frame_count();
        let frame = self.last_frame.clone();
        self.write_frame_copies(&frame, target)
    }
}

impl GraphicsCaptureApiHandler for FramePipe {
    type Flags = PipeFlags;
    type Error = String;

    fn new(ctx: CaptureContext<Self::Flags>) -> Result<Self, Self::Error> {
        Ok(Self {
            writer: ctx.flags.writer,
            width: ctx.flags.width,
            height: ctx.flags.height,
            fps: ctx.flags.fps,
            started_at: Instant::now(),
            frames_written: 0,
            last_frame: Vec::new(),
            scratch: Vec::new(),
        })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        capture_control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        if frame.width() < self.width || frame.height() < self.height {
            return Err(format!(
                "Captured window became smaller than the recording size ({}x{} -> {}x{})",
                self.width,
                self.height,
                frame.width(),
                frame.height()
            ));
        }

        let buffer = if frame.width() == self.width && frame.height() == self.height {
            frame.buffer().map_err(|e| format!("Unable to map WGC frame: {e}"))?
        } else {
            frame
                .buffer_crop(0, 0, self.width, self.height)
                .map_err(|e| format!("Unable to crop WGC frame: {e}"))?
        };

        self.scratch.clear();
        let bytes = buffer.as_nopadding_buffer(&mut self.scratch);
        let expected = self.width as usize * self.height as usize * 4;
        if bytes.len() != expected {
            return Err(format!("Unexpected WGC frame size: got {}, expected {expected}", bytes.len()));
        }

        self.last_frame.clear();
        self.last_frame.extend_from_slice(bytes);
        let target = self.target_frame_count();
        let current = self.last_frame.clone();
        if let Err(error) = self.write_frame_copies(&current, target) {
            capture_control.stop();
            return Err(error);
        }
        Ok(())
    }

    fn on_closed(&mut self) -> Result<(), Self::Error> {
        self.pad_to_now()
    }
}

pub struct NativeVideoCapture {
    control: Option<CaptureControl<FramePipe, String>>,
    writer: Arc<Mutex<Option<ChildStdin>>>,
    child: Child,
}

impl NativeVideoCapture {
    pub fn stop(mut self) -> Result<()> {
        if let Some(control) = self.control.take() {
            // Preserve real elapsed duration even when WGC emitted no new frame
            // while the captured content was static.
            let callback = control.callback();
            callback
                .lock()
                .pad_to_now()
                .map_err(|e| anyhow!(e))?;
            control.stop().map_err(|e| anyhow!("Unable to stop Windows Graphics Capture: {e}"))?;
        }

        // Closing stdin signals EOF to FFmpeg's rawvideo input so it can flush
        // the H.264/AAC streams and write the MP4 trailer normally.
        self.writer.lock().take();
        let status = self.child.wait().context("Unable to finalize WGC recording segment")?;
        if !status.success() {
            return Err(anyhow!("FFmpeg could not finalize the WGC recording segment"));
        }
        Ok(())
    }
}

fn choose_h264_encoder() -> &'static str {
    let Ok(output) = Command::new("ffmpeg").args(["-hide_banner", "-encoders"]).output() else {
        return "libx264";
    };
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    if text.contains("h264_mf") { "h264_mf" } else { "libx264" }
}

fn spawn_ffmpeg(config: &RecordingConfig, width: u32, height: u32, output_path: &Path) -> Result<(Child, Arc<Mutex<Option<ChildStdin>>>)> {
    let fps = config.fps.clamp(1, 60).to_string();
    let size = format!("{width}x{height}");
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-hide_banner", "-loglevel", "warning", "-y"]);
    cmd.args([
        "-thread_queue_size", "1024",
        "-f", "rawvideo",
        "-pixel_format", "bgra",
        "-video_size", &size,
        "-framerate", &fps,
        "-i", "pipe:0",
    ]);

    let mut next_input = 1usize;
    let mic_input = if let Some(id) = config.microphone_id.as_deref() {
        let name = audio::resolve_microphone_name(id)
            .ok_or_else(|| anyhow!("The selected microphone is no longer available"))?;
        cmd.args(["-thread_queue_size", "1024", "-f", "dshow", "-i", &format!("audio={name}")]);
        let index = next_input;
        next_input += 1;
        Some(index)
    } else {
        None
    };

    let camera_input = if let Some(id) = config.camera_id.as_deref() {
        let name = camera::resolve_camera_name(id)
            .ok_or_else(|| anyhow!("The selected camera is no longer available"))?;
        cmd.args(["-thread_queue_size", "1024", "-f", "dshow", "-i", &format!("video={name}")]);
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
        cmd.args(["-vf", "scale=trunc(iw/2)*2:trunc(ih/2)*2", "-map", "0:v:0"]);
    }

    if let Some(index) = mic_input {
        cmd.args(["-map", &format!("{index}:a:0"), "-c:a", "aac", "-b:a", "160k"]);
    } else {
        cmd.arg("-an");
    }

    let encoder = choose_h264_encoder();
    cmd.args(["-c:v", encoder, "-pix_fmt", "yuv420p"]);
    if encoder == "libx264" {
        cmd.args(["-preset", "veryfast", "-crf", "23"]);
    } else {
        cmd.args(["-b:v", "6000k"]);
    }
    cmd.args(["-r", &fps, "-movflags", "+faststart"]);
    cmd.arg(output_path);
    cmd.stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::inherit());

    let mut child = cmd.spawn().context("Unable to start FFmpeg for Windows Graphics Capture")?;
    let stdin = child.stdin.take().ok_or_else(|| anyhow!("FFmpeg rawvideo stdin was not available"))?;
    let writer = Arc::new(Mutex::new(Some(stdin)));

    thread::sleep(Duration::from_millis(250));
    if let Some(status) = child.try_wait().context("Unable to inspect FFmpeg process")? {
        return Err(anyhow!(
            "FFmpeg stopped while starting WGC recording (exit code {})",
            status.code().map(|v| v.to_string()).unwrap_or_else(|| "unknown".into())
        ));
    }
    Ok((child, writer))
}

fn start_item<T>(item: T, config: &RecordingConfig, width: u32, height: u32, output_path: &Path) -> Result<NativeVideoCapture>
where
    T: TryInto<GraphicsCaptureItemType> + Send + 'static,
{
    let (child, writer) = spawn_ffmpeg(config, width, height, output_path)?;
    let fps = config.fps.clamp(1, 60);
    let settings = Settings::new(
        item,
        CursorCaptureSettings::WithCursor,
        DrawBorderSettings::WithoutBorder,
        SecondaryWindowSettings::Default,
        MinimumUpdateIntervalSettings::Custom(Duration::from_secs_f64(1.0 / fps as f64)),
        DirtyRegionSettings::Default,
        ColorFormat::Bgra8,
        PipeFlags { writer: writer.clone(), width, height, fps },
    );

    let control = match FramePipe::start_free_threaded(settings) {
        Ok(control) => control,
        Err(error) => {
            writer.lock().take();
            let mut child = child;
            let _ = child.kill();
            let _ = child.wait();
            return Err(anyhow!("Unable to start Windows Graphics Capture: {error}"));
        }
    };

    Ok(NativeVideoCapture { control: Some(control), writer, child })
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
            let raw = target.id.strip_prefix("monitor-").ok_or_else(|| anyhow!("Invalid monitor id"))?;
            let handle = usize::from_str_radix(raw, 16).map_err(|_| anyhow!("Invalid monitor id"))?;
            let monitor = Monitor::from_raw_hmonitor(handle as *mut c_void);
            start_item(monitor, config, width, height, output_path)
        }
        CaptureKind::Window => {
            let raw = target.id.strip_prefix("window-").ok_or_else(|| anyhow!("Invalid window id"))?;
            let handle = usize::from_str_radix(raw, 16).map_err(|_| anyhow!("Invalid window id"))?;
            let window = Window::from_raw_hwnd(handle as *mut c_void);
            if !window.is_valid() {
                return Err(anyhow!("Selected window is no longer capturable"));
            }
            start_item(window, config, width, height, output_path)
        }
    }
}
