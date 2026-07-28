//! Encoding + muxing helpers used by the Windows recorder.
//!
//! Live screen capture uses the native Windows encoder. FFmpeg remains available
//! only for compatibility camera capture and multi-segment fallback finalization.
//! Expensive media decoration work must not block recorder controls.

use anyhow::{anyhow, Context, Result};
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[cfg(windows)]
use std::sync::atomic::{AtomicU8, Ordering};
#[cfg(windows)]
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[cfg(windows)]
const WARMUP_NOT_STARTED: u8 = 0;
#[cfg(windows)]
const WARMUP_RUNNING: u8 = 1;
#[cfg(windows)]
const WARMUP_FINISHED: u8 = 2;
#[cfg(windows)]
const WARMUP_FAILED: u8 = 3;

#[cfg(windows)]
static NATIVE_ENCODER_WARMUP_STATE: AtomicU8 = AtomicU8::new(WARMUP_NOT_STARTED);

/// Start the one-time Media Foundation warm-up without delaying application setup.
///
/// Windows often pays most of the H.264/AAC MediaTranscoder initialization cost on
/// the first encoder created by the process. Exercising a tiny temporary recording
/// while the frontend is loading moves that cost away from the Start button.
pub fn warm_native_capture_pipeline_async() {
    #[cfg(windows)]
    {
        if NATIVE_ENCODER_WARMUP_STATE
            .compare_exchange(
                WARMUP_NOT_STARTED,
                WARMUP_RUNNING,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_err()
        {
            return;
        }

        let spawn_result = std::thread::Builder::new()
            .name("recorder-media-warmup".to_string())
            .spawn(|| {
                let started = Instant::now();
                match warm_native_capture_pipeline() {
                    Ok(()) => {
                        NATIVE_ENCODER_WARMUP_STATE
                            .store(WARMUP_FINISHED, Ordering::Release);
                        eprintln!(
                            "[Recorder][Health] native_media_warmup_ok=true warmup_ms={}",
                            started.elapsed().as_millis()
                        );
                    }
                    Err(error) => {
                        NATIVE_ENCODER_WARMUP_STATE.store(WARMUP_FAILED, Ordering::Release);
                        eprintln!(
                            "[Recorder][Health] native_media_warmup_ok=false warmup_ms={} error={error}",
                            started.elapsed().as_millis()
                        );
                    }
                }
            });

        if let Err(error) = spawn_result {
            NATIVE_ENCODER_WARMUP_STATE.store(WARMUP_FAILED, Ordering::Release);
            eprintln!(
                "[Recorder][Health] native_media_warmup_ok=false error=unable_to_spawn_warmup_thread:{error}"
            );
        }
    }
}

#[cfg(windows)]
fn warm_native_capture_pipeline() -> Result<()> {
    use windows_capture::encoder::{
        AudioSettingsBuilder, ContainerSettingsBuilder, VideoEncoder, VideoSettingsBuilder,
        VideoSettingsSubType,
    };

    const WIDTH: u32 = 320;
    const HEIGHT: u32 = 180;
    const FPS: u32 = 30;
    const SAMPLE_RATE: u32 = 48_000;
    const CHANNELS: u32 = 2;

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let output_path = env::temp_dir().join(format!(
        "recorder-native-warmup-{}-{unique}.mp4",
        std::process::id()
    ));

    let result = (|| -> Result<()> {
        let mut encoder = VideoEncoder::new(
            VideoSettingsBuilder::new(WIDTH, HEIGHT)
                .sub_type(VideoSettingsSubType::H264)
                .bitrate(1_000_000)
                .frame_rate(FPS),
            AudioSettingsBuilder::new()
                .sample_rate(SAMPLE_RATE)
                .channel_count(CHANNELS)
                .bit_per_sample(16),
            ContainerSettingsBuilder::new(),
            &output_path,
        )
        .map_err(|error| anyhow!("Unable to initialize native media warm-up encoder: {error}"))?;

        // The raw-buffer encoder expects bottom-up BGRA. A solid black frame is the
        // same in either orientation, and an opaque alpha channel avoids driver-specific
        // treatment of fully transparent pixels.
        let mut frame = vec![0u8; WIDTH as usize * HEIGHT as usize * 4];
        for pixel in frame.chunks_exact_mut(4) {
            pixel[3] = 255;
        }

        let frame_interval_hns = 10_000_000i64 / i64::from(FPS);
        for index in 0..3i64 {
            encoder
                .send_frame_buffer(&frame, index * frame_interval_hns)
                .map_err(|error| anyhow!("Unable to submit native warm-up frame: {error}"))?;
        }

        // Thirty milliseconds of stereo i16 silence initializes the AAC side of the
        // same MediaTranscoder pipeline used by microphone/system-audio recordings.
        let audio_frames = SAMPLE_RATE as usize * 30 / 1_000;
        let silence = vec![0u8; audio_frames * CHANNELS as usize * 2];
        encoder
            .send_audio_buffer(&silence, 0)
            .map_err(|error| anyhow!("Unable to submit native warm-up audio: {error}"))?;

        encoder
            .finish()
            .map_err(|error| anyhow!("Unable to finalize native media warm-up: {error}"))?;
        Ok(())
    })();

    // Warm-up is best-effort and must never leave media files in the user's temp folder.
    let _ = std::fs::remove_file(&output_path);
    result
}

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

/// Stop must never wait for thumbnail extraction. The recording engine calls this
/// placeholder while constructing its immediate response; the library worker below
/// performs the real Windows thumbnail extraction after the MP4 has been returned.
pub fn thumbnail_data_url(_video_path: &Path) -> Option<String> {
    None
}

/// Extract a cached Windows video thumbnail and return it as a browser-ready data URL.
/// This function is intentionally blocking and must only be called from a background
/// library worker, never from Start/Pause/Resume/Stop command handling.
pub fn generate_thumbnail_data_url(video_path: &Path) -> Option<String> {
    #[cfg(windows)]
    {
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine as _;
        use windows::core::HSTRING;
        use windows::Storage::FileProperties::{ThumbnailMode, ThumbnailOptions};
        use windows::Storage::Streams::{Buffer, DataReader, InputStreamOptions};
        use windows::Storage::StorageFile;

        const REQUESTED_EDGE: u32 = 480;
        const MAX_THUMBNAIL_BYTES: u64 = 16 * 1024 * 1024;

        let absolute = std::fs::canonicalize(video_path).ok()?;
        let path = HSTRING::from(absolute.to_string_lossy().as_ref());
        let file = StorageFile::GetFileFromPathAsync(&path).ok()?.get().ok()?;
        let thumbnail = file
            .GetThumbnailAsync(
                ThumbnailMode::VideosView,
                REQUESTED_EDGE,
                ThumbnailOptions::UseCurrentScale,
            )
            .ok()?
            .get()
            .ok()?;

        let size = thumbnail.Size().ok()?;
        if size == 0 || size > MAX_THUMBNAIL_BYTES || size > u64::from(u32::MAX) {
            let _ = thumbnail.Close();
            return None;
        }

        let buffer = Buffer::Create(size as u32).ok()?;
        let filled = thumbnail
            .ReadAsync(&buffer, size as u32, InputStreamOptions::None)
            .ok()?
            .get()
            .ok()?;
        let length = filled.Length().ok()? as usize;
        if length == 0 {
            let _ = thumbnail.Close();
            return None;
        }

        let reader = DataReader::FromBuffer(&filled).ok()?;
        let mut bytes = vec![0u8; length];
        reader.ReadBytes(&mut bytes).ok()?;

        let content_type = thumbnail
            .ContentType()
            .ok()
            .map(|value| value.to_string())
            .filter(|value| value.starts_with("image/"))
            .unwrap_or_else(|| "image/jpeg".to_string());

        let _ = reader.Close();
        let _ = thumbnail.Close();
        return Some(format!(
            "data:{content_type};base64,{}",
            STANDARD.encode(bytes)
        ));
    }

    #[cfg(not(windows))]
    {
        let _ = video_path;
        None
    }
}
