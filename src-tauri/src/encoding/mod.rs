//! Encoding + muxing helpers used by the Windows recorder.
//!
//! Live screen capture uses the native Windows encoder. FFmpeg remains available
//! only for compatibility camera capture and multi-segment fallback finalization.
//! Expensive media decoration work must not block recorder controls.

use anyhow::{anyhow, Context, Result};
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Resolve the FFmpeg executable in a packaging-friendly order:
/// 1. explicit RECORDER_FFMPEG_PATH override;
/// 2. ffmpeg(.exe) beside the Recorder executable (future Tauri sidecar);
/// 3. PATH lookup for local development.
pub fn ffmpeg_program() -> PathBuf {
    if let Some(path) = env::var_os("RECORDER_FFMPEG_PATH") {
        let path = PathBuf::from(path);
        if path.exists() {
            return path;
        }
    }

    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            #[cfg(windows)]
            let candidate = dir.join("ffmpeg.exe");
            #[cfg(not(windows))]
            let candidate = dir.join("ffmpeg");
            if candidate.exists() {
                return candidate;
            }
        }
    }

    PathBuf::from("ffmpeg")
}

pub fn ffmpeg_command() -> Command {
    Command::new(ffmpeg_program())
}

pub fn ensure_ffmpeg_available() -> Result<()> {
    let status = ffmpeg_command()
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| format!("FFmpeg was not found at {}", ffmpeg_program().display()))?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("FFmpeg is installed but could not be started"))
    }
}

/// Thumbnail extraction is deliberately not part of the Stop transaction.
/// Returning the completed MP4 to the UI takes priority; a later library worker
/// can generate thumbnails without holding the recorder on `Saving…`.
pub fn thumbnail_data_url(_video_path: &Path) -> Option<String> {
    None
}
