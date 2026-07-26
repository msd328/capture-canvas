//! Recording engine surface.
//!
//! The first real Windows pipeline deliberately keeps the existing frontend
//! contract: a selected display/window is captured with FFmpeg's gdigrab input,
//! optional microphone/camera devices are opened through DirectShow, and the
//! result is encoded to H.264/AAC MP4. Pause/resume is implemented as segments
//! that are concatenated losslessly when the recording stops.

pub mod types;

use crate::{audio, camera, capture};
use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use parking_lot::Mutex;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;

pub use types::*;

struct ActiveRecording {
    id: String,
    config: RecordingConfig,
    started_at: Instant,
    paused_total_ms: u64,
    paused_at: Option<Instant>,
    final_path: PathBuf,
    segment_paths: Vec<PathBuf>,
    current_child: Option<Child>,
    width: u32,
    height: u32,
}

#[derive(Default)]
pub struct RecordingEngine {
    active: Mutex<Option<ActiveRecording>>,
}

impl RecordingEngine {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn start(&self, config: RecordingConfig) -> Result<String> {
        let mut guard = self.active.lock();
        if guard.is_some() {
            return Err(anyhow!("A recording is already in progress"));
        }
        if config.system_audio {
            return Err(anyhow!(
                "System audio capture is not enabled yet. Turn System Audio off for this build."
            ));
        }
        ensure_ffmpeg_available()?;

        let id = Uuid::new_v4().to_string();
        let source = capture::resolve_target(&config.target)?;
        let width = source.width & !1;
        let height = source.height & !1;
        if width == 0 || height == 0 {
            return Err(anyhow!("The selected capture source has an invalid size"));
        }

        let final_path = output_path_for(&id, config.output_path.as_deref())?;
        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Unable to create recording folder {}", parent.display()))?;
        }
        let first_segment = segment_path(&final_path, &id, 0);
        let child = spawn_segment(&config, &source, &first_segment)?;

        *guard = Some(ActiveRecording {
            id: id.clone(),
            config,
            started_at: Instant::now(),
            paused_total_ms: 0,
            paused_at: None,
            final_path,
            segment_paths: vec![first_segment],
            current_child: Some(child),
            width,
            height,
        });
        Ok(id)
    }

    pub fn pause(&self) -> Result<()> {
        let mut guard = self.active.lock();
        let rec = guard.as_mut().ok_or_else(|| anyhow!("No active recording"))?;
        if rec.paused_at.is_some() {
            return Ok(());
        }
        if let Some(mut child) = rec.current_child.take() {
            stop_ffmpeg(&mut child)?;
        }
        rec.paused_at = Some(Instant::now());
        Ok(())
    }

    pub fn resume(&self) -> Result<()> {
        let mut guard = self.active.lock();
        let rec = guard.as_mut().ok_or_else(|| anyhow!("No active recording"))?;
        let Some(paused_at) = rec.paused_at.take() else {
            return Ok(());
        };
        rec.paused_total_ms = rec
            .paused_total_ms
            .saturating_add(paused_at.elapsed().as_millis() as u64);

        let source = capture::resolve_target(&rec.config.target)?;
        let next_index = rec.segment_paths.len();
        let path = segment_path(&rec.final_path, &rec.id, next_index);
        let child = spawn_segment(&rec.config, &source, &path)?;
        rec.segment_paths.push(path);
        rec.current_child = Some(child);
        Ok(())
    }

    pub fn stop(&self) -> Result<RecordingOutput> {
        let mut guard = self.active.lock();
        let mut rec = guard.take().ok_or_else(|| anyhow!("No active recording"))?;

        if let Some(mut child) = rec.current_child.take() {
            stop_ffmpeg(&mut child)?;
        }

        let paused = rec.paused_total_ms
            + rec
                .paused_at
                .map(|at| at.elapsed().as_millis() as u64)
                .unwrap_or(0);
        let duration_ms = (Instant::now()
            .duration_since(rec.started_at)
            .as_millis() as u64)
            .saturating_sub(paused)
            .max(1);

        finalize_segments(&rec.segment_paths, &rec.final_path)?;
        let metadata = fs::metadata(&rec.final_path)
            .with_context(|| format!("Recording file was not created: {}", rec.final_path.display()))?;
        if metadata.len() == 0 {
            return Err(anyhow!("Recording completed but the MP4 file is empty"));
        }

        Ok(RecordingOutput {
            id: rec.id,
            title: rec
                .config
                .title
                .unwrap_or_else(|| format!("Recording {}", Utc::now().format("%Y-%m-%d %H:%M"))),
            file_path: rec.final_path.to_string_lossy().to_string(),
            created_at: Utc::now().to_rfc3339(),
            duration_ms,
            width: rec.width,
            height: rec.height,
            file_size_bytes: metadata.len(),
            thumbnail_data_url: None,
        })
    }
}

fn ensure_ffmpeg_available() -> Result<()> {
    let status = Command::new("ffmpeg")
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|_| anyhow!(
            "FFmpeg is required for the current Windows recording milestone but was not found on PATH. Install FFmpeg, restart PowerShell, and try again."
        ))?;
    if !status.success() {
        return Err(anyhow!("FFmpeg is installed but could not be started"));
    }
    Ok(())
}

fn output_path_for(id: &str, requested: Option<&str>) -> Result<PathBuf> {
    if let Some(path) = requested.filter(|p| !p.trim().is_empty()) {
        return Ok(expand_tilde(path));
    }
    let home = std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("Unable to determine your Windows user profile directory"))?;
    Ok(home.join("Recordings").join(format!("{id}.mp4")))
}

fn expand_tilde(path: &str) -> PathBuf {
    if path == "~" {
        if let Some(home) = std::env::var_os("USERPROFILE") {
            return PathBuf::from(home);
        }
    }
    if let Some(rest) = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        if let Some(home) = std::env::var_os("USERPROFILE") {
            return PathBuf::from(home).join(rest.replace('/', "\\"));
        }
    }
    PathBuf::from(path)
}

fn segment_path(final_path: &Path, id: &str, index: usize) -> PathBuf {
    let parent = final_path.parent().unwrap_or_else(|| Path::new("."));
    parent.join(format!("{id}.part{index:03}.mp4"))
}

fn choose_h264_encoder() -> &'static str {
    let Ok(output) = Command::new("ffmpeg")
        .args(["-hide_banner", "-encoders"])
        .output()
    else {
        return "libx264";
    };
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    if text.contains("h264_mf") {
        "h264_mf"
    } else {
        "libx264"
    }
}

fn spawn_segment(
    config: &RecordingConfig,
    source: &capture::CaptureSource,
    output_path: &Path,
) -> Result<Child> {
    let fps = config.fps.clamp(1, 60).to_string();
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-hide_banner", "-loglevel", "warning", "-y"]);

    cmd.args(["-thread_queue_size", "1024", "-f", "gdigrab", "-framerate", &fps, "-draw_mouse", "1"]);
    if let (Some(x), Some(y)) = (source.offset_x, source.offset_y) {
        cmd.args(["-offset_x", &x.to_string(), "-offset_y", &y.to_string()]);
        cmd.args(["-video_size", &format!("{}x{}", source.width, source.height)]);
    }
    cmd.args(["-i", &source.ffmpeg_input]);

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
        let index = next_input;
        Some(index)
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
        cmd.args(["-map", &format!("{index}:a:0")]);
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
    if mic_input.is_some() {
        cmd.args(["-c:a", "aac", "-b:a", "160k"]);
    }
    cmd.args(["-r", &fps, "-movflags", "+faststart"]);
    cmd.arg(output_path);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());

    let mut child = cmd.spawn().context("Unable to start FFmpeg recording process")?;
    thread::sleep(Duration::from_millis(450));
    if let Some(status) = child.try_wait().context("Unable to inspect FFmpeg process")? {
        return Err(anyhow!(
            "FFmpeg stopped while starting the recording (exit code {}). Check the terminal output above for the device/capture error.",
            status.code().map(|v| v.to_string()).unwrap_or_else(|| "unknown".into())
        ));
    }
    Ok(child)
}

fn stop_ffmpeg(child: &mut Child) -> Result<()> {
    if let Some(stdin) = child.stdin.as_mut() {
        let _ = stdin.write_all(b"q\n");
        let _ = stdin.flush();
    }
    let status = child.wait().context("Unable to finalize FFmpeg recording")?;
    if !status.success() {
        return Err(anyhow!("FFmpeg could not finalize the current recording segment"));
    }
    Ok(())
}

fn finalize_segments(segments: &[PathBuf], final_path: &Path) -> Result<()> {
    if segments.is_empty() {
        return Err(anyhow!("No recording segments were produced"));
    }
    if final_path.exists() {
        let _ = fs::remove_file(final_path);
    }
    if segments.len() == 1 {
        fs::rename(&segments[0], final_path)
            .or_else(|_| {
                fs::copy(&segments[0], final_path)?;
                fs::remove_file(&segments[0])
            })
            .with_context(|| format!("Unable to move recording to {}", final_path.display()))?;
        return Ok(());
    }

    let list_path = final_path.with_extension("concat.txt");
    let mut body = String::new();
    for segment in segments {
        let normalized = segment.to_string_lossy().replace('\\', "/").replace('\'', "'\\''");
        body.push_str(&format!("file '{normalized}'\n"));
    }
    fs::write(&list_path, body).context("Unable to create FFmpeg concat list")?;

    let status = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "warning", "-y", "-f", "concat", "-safe", "0", "-i"])
        .arg(&list_path)
        .args(["-c", "copy", "-movflags", "+faststart"])
        .arg(final_path)
        .status()
        .context("Unable to concatenate recording segments")?;
    let _ = fs::remove_file(&list_path);
    if !status.success() {
        return Err(anyhow!("FFmpeg could not concatenate paused recording segments"));
    }
    for segment in segments {
        let _ = fs::remove_file(segment);
    }
    Ok(())
}
