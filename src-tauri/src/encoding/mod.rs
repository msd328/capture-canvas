//! Encoding + muxing helpers used by the Windows recorder.
//!
//! Video frames are captured natively through Windows Graphics Capture and are
//! currently encoded/muxed by an FFmpeg process. Keep all FFmpeg discovery and
//! encoder selection in this module so packaging can later swap PATH lookup for
//! an approved bundled sidecar without touching capture/audio orchestration.

use anyhow::{anyhow, Context, Result};
use base64::Engine;
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

static H264_ENCODER: OnceLock<String> = OnceLock::new();

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

fn encoder_list() -> String {
    let Ok(output) = ffmpeg_command().args(["-hide_banner", "-encoders"]).output() else {
        return String::new();
    };
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    text
}

fn h264_pixel_format(encoder: &str) -> &'static str {
    match encoder {
        // Intel Quick Sync's H.264 encoder uses NV12 surfaces. Explicitly
        // request NV12 so FFmpeg does not have to auto-correct yuv420p and emit
        // an "Incompatible pixel format" warning on every recording.
        "h264_qsv" => "nv12",
        _ => "yuv420p",
    }
}

fn probe_encoder(name: &str) -> bool {
    #[cfg(windows)]
    let null_sink = "NUL";
    #[cfg(not(windows))]
    let null_sink = "/dev/null";

    ffmpeg_command()
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=64x64:r=1",
            "-frames:v",
            "1",
            "-an",
            "-c:v",
            name,
            "-pix_fmt",
            h264_pixel_format(name),
            "-f",
            "null",
            null_sink,
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Select a hardware encoder only when it can actually encode on this machine.
/// FFmpeg can list NVENC/QSV/AMF even when the matching GPU/driver is absent, so
/// a tiny one-frame probe is more reliable than merely parsing `-encoders`.
pub fn selected_h264_encoder() -> &'static str {
    H264_ENCODER
        .get_or_init(|| {
            let encoders = encoder_list();
            for candidate in ["h264_nvenc", "h264_qsv", "h264_amf", "h264_mf"] {
                if encoders.contains(candidate) && probe_encoder(candidate) {
                    return candidate.to_string();
                }
            }
            "libx264".to_string()
        })
        .as_str()
}

pub fn apply_h264_options(cmd: &mut Command, encoder: &str) {
    cmd.args(["-c:v", encoder, "-pix_fmt", h264_pixel_format(encoder)]);
    match encoder {
        "libx264" => {
            cmd.args(["-preset", "veryfast", "-crf", "23"]);
        }
        "h264_nvenc" => {
            cmd.args(["-preset", "p4", "-b:v", "8M", "-maxrate", "12M", "-bufsize", "16M"]);
        }
        _ => {
            // Generic bitrate options are accepted by Media Foundation, QSV and
            // AMF and avoid depending on vendor-specific quality flags.
            cmd.args(["-b:v", "8M", "-maxrate", "12M", "-bufsize", "16M"]);
        }
    }
}

fn extract_thumbnail(video_path: &Path, seek: Option<&str>) -> Option<Vec<u8>> {
    let mut cmd = ffmpeg_command();
    cmd.args(["-hide_banner", "-loglevel", "error"]);
    if let Some(seek) = seek {
        cmd.args(["-ss", seek]);
    }
    let output = cmd
        .arg("-i")
        .arg(video_path)
        .args([
            "-frames:v",
            "1",
            "-vf",
            "scale=480:-2",
            "-q:v",
            "6",
            "-f",
            "image2pipe",
            "-vcodec",
            "mjpeg",
            "pipe:1",
        ])
        .output()
        .ok()?;

    (output.status.success() && !output.stdout.is_empty()).then_some(output.stdout)
}

/// Generate a compact JPEG data URL for the local Library card. A half-second
/// seek normally avoids a black encoder-start frame; very short clips retry from
/// the first frame.
pub fn thumbnail_data_url(video_path: &Path) -> Option<String> {
    let jpeg = extract_thumbnail(video_path, Some("0.5"))
        .or_else(|| extract_thumbnail(video_path, None))?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(jpeg);
    Some(format!("data:image/jpeg;base64,{encoded}"))
}
