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
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
#[cfg(windows)]
use windows::core::HSTRING;

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

/// Convert a canonical Windows path into the normal DOS/UNC form accepted by
/// WinRT Storage APIs. `std::fs::canonicalize` commonly returns an extended
/// `\\?\` path, which `StorageFile::GetFileFromPathAsync` can reject.
#[cfg(windows)]
pub(crate) fn windows_storage_path(path: &Path) -> Result<HSTRING> {
    let absolute = std::fs::canonicalize(path).context("Unable to resolve Windows media path")?;
    let extended = absolute.to_string_lossy();
    let normalized = if let Some(rest) = extended.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = extended.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        extended.into_owned()
    };
    Ok(HSTRING::from(normalized))
}

#[derive(Debug, Clone)]
pub struct ThumbnailGenerationError {
    pub stage: &'static str,
    pub attempt: usize,
    pub code: Option<String>,
}

impl ThumbnailGenerationError {
    fn new(stage: &'static str, attempt: usize, code: Option<String>) -> Self {
        Self {
            stage,
            attempt,
            code,
        }
    }

    #[cfg(windows)]
    fn windows(stage: &'static str, attempt: usize, error: &windows::core::Error) -> Self {
        Self::new(stage, attempt, Some(format!("{:?}", error.code())))
    }
}

impl std::fmt::Display for ThumbnailGenerationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.code.as_deref() {
            Some(code) => write!(
                formatter,
                "thumbnail generation failed at stage={} attempt={} code={code}",
                self.stage, self.attempt
            ),
            None => write!(
                formatter,
                "thumbnail generation failed at stage={} attempt={}",
                self.stage, self.attempt
            ),
        }
    }
}

impl std::error::Error for ThumbnailGenerationError {}

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

#[cfg(windows)]
fn generate_thumbnail_attempt(
    path: &HSTRING,
    attempt: usize,
) -> std::result::Result<String, ThumbnailGenerationError> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine as _;
    use windows::Storage::FileProperties::{ThumbnailMode, ThumbnailOptions};
    use windows::Storage::StorageFile;
    use windows::Storage::Streams::{Buffer, DataReader, InputStreamOptions};

    const REQUESTED_EDGE: u32 = 480;
    const MAX_THUMBNAIL_BYTES: u64 = 16 * 1024 * 1024;

    let file_operation = StorageFile::GetFileFromPathAsync(path)
        .map_err(|error| ThumbnailGenerationError::windows("open_file_start", attempt, &error))?;
    let file = file_operation
        .get()
        .map_err(|error| ThumbnailGenerationError::windows("open_file_wait", attempt, &error))?;

    let thumbnail_operation = file
        .GetThumbnailAsync(
            ThumbnailMode::VideosView,
            REQUESTED_EDGE,
            ThumbnailOptions::UseCurrentScale,
        )
        .map_err(|error| {
            ThumbnailGenerationError::windows("request_thumbnail_start", attempt, &error)
        })?;
    let thumbnail = thumbnail_operation.get().map_err(|error| {
        ThumbnailGenerationError::windows("request_thumbnail_wait", attempt, &error)
    })?;

    let size = thumbnail
        .Size()
        .map_err(|error| ThumbnailGenerationError::windows("thumbnail_size", attempt, &error))?;
    if size == 0 || size > MAX_THUMBNAIL_BYTES || size > u64::from(u32::MAX) {
        let _ = thumbnail.Close();
        return Err(ThumbnailGenerationError::new(
            "thumbnail_size_invalid",
            attempt,
            Some(size.to_string()),
        ));
    }

    let buffer = Buffer::Create(size as u32)
        .map_err(|error| ThumbnailGenerationError::windows("create_buffer", attempt, &error))?;
    let read_operation = thumbnail
        .ReadAsync(&buffer, size as u32, InputStreamOptions::None)
        .map_err(|error| {
            ThumbnailGenerationError::windows("read_thumbnail_start", attempt, &error)
        })?;
    let filled = read_operation.get().map_err(|error| {
        ThumbnailGenerationError::windows("read_thumbnail_wait", attempt, &error)
    })?;
    let length = filled
        .Length()
        .map_err(|error| ThumbnailGenerationError::windows("read_length", attempt, &error))?
        as usize;
    if length == 0 {
        let _ = thumbnail.Close();
        return Err(ThumbnailGenerationError::new(
            "read_length_empty",
            attempt,
            None,
        ));
    }

    let reader = DataReader::FromBuffer(&filled)
        .map_err(|error| ThumbnailGenerationError::windows("create_reader", attempt, &error))?;
    let mut bytes = vec![0u8; length];
    reader
        .ReadBytes(&mut bytes)
        .map_err(|error| ThumbnailGenerationError::windows("read_bytes", attempt, &error))?;

    let content_type = thumbnail
        .ContentType()
        .ok()
        .map(|value| value.to_string())
        .filter(|value| value.starts_with("image/"))
        .unwrap_or_else(|| "image/jpeg".to_string());

    let _ = reader.Close();
    let _ = thumbnail.Close();
    Ok(format!(
        "data:{content_type};base64,{}",
        STANDARD.encode(bytes)
    ))
}

/// Extract a cached Windows video thumbnail and return it as a browser-ready data URL.
/// This function is intentionally blocking and must only be called from a background
/// library worker, never from Start/Pause/Resume/Stop command handling.
pub fn generate_thumbnail_data_url(
    video_path: &Path,
) -> std::result::Result<String, ThumbnailGenerationError> {
    #[cfg(windows)]
    {
        const RETRY_DELAYS_MS: [u64; 3] = [0, 150, 400];

        let path = windows_storage_path(video_path)
            .map_err(|_| ThumbnailGenerationError::new("normalize_path", 0, None))?;
        let mut last_error = ThumbnailGenerationError::new("unknown", 0, None);
        for (attempt_index, delay_ms) in RETRY_DELAYS_MS.into_iter().enumerate() {
            let attempt = attempt_index + 1;
            if delay_ms > 0 {
                std::thread::sleep(Duration::from_millis(delay_ms));
            }
            match generate_thumbnail_attempt(&path, attempt) {
                Ok(thumbnail) => return Ok(thumbnail),
                Err(error) => last_error = error,
            }
        }
        Err(last_error)
    }

    #[cfg(not(windows))]
    {
        let _ = video_path;
        Err(ThumbnailGenerationError::new(
            "unsupported_platform",
            0,
            None,
        ))
    }
}
