//! Recording engine surface.
//!
//! Windows display/window frames come from Windows.Graphics.Capture/D3D11. The
//! preferred path encodes D3D11 surfaces through the native Windows H.264 encoder,
//! mixes microphone/system audio natively, and can composite the webcam on-GPU.
//! Windows MediaComposition is the primary multi-segment pause/resume finalizer;
//! FFmpeg remains only as a compatibility fallback and best-effort thumbnail tool.

pub mod types;

mod ffmpeg_finalizer;

#[cfg(windows)]
mod windows_media;

use crate::{audio, capture, encoding};
use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use parking_lot::Mutex;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
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
    system_audio_paths: Vec<PathBuf>,
    current_video: Option<capture::NativeVideoCapture>,
    current_system_audio: Option<audio::SystemAudioCapture>,
    external_system_audio: bool,
    source_width: u32,
    source_height: u32,
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

    pub fn start(&self, mut config: RecordingConfig) -> Result<String> {
        let mut guard = self.active.lock();
        if guard.is_some() {
            return Err(anyhow!("A recording is already in progress"));
        }
        if config.system_audio && !audio::system_audio_supported() {
            return Err(anyhow!(
                "Windows has no usable default audio output endpoint for system-audio capture"
            ));
        }

        let id = Uuid::new_v4().to_string();
        let source = capture::resolve_target(&config.target)?;
        let source_width = source.width;
        let source_height = source.height;
        let (width, height) = normalize_crop_region(&mut config, source_width, source_height)?;

        let final_path = output_path_for(&id, config.output_path.as_deref())?;
        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("Unable to create recording folder {}", parent.display())
            })?;
        }

        let first_segment = segment_path(&final_path, &id, 0);
        let video = capture::start_native_video_capture(
            &config,
            &config.target,
            width,
            height,
            &first_segment,
        )?;
        let external_system_audio = config.system_audio && !video.captures_system_audio();

        // The preferred GPU backend owns system audio itself. An external WAV is
        // needed only when native capture/encoding failed and the compatibility
        // video backend took over.
        let (current_system_audio, system_audio_paths) = if external_system_audio {
            if let Err(error) = encoding::ensure_ffmpeg_available() {
                let _ = video.stop();
                let _ = fs::remove_file(&first_segment);
                return Err(error.context(
                    "This recording configuration requires FFmpeg for compatibility system-audio mixing",
                ));
            }

            let path = system_audio_path(&final_path, &id, 0);
            match audio::start_system_audio_capture(&path) {
                Ok(capture) => (Some(capture), vec![path]),
                Err(error) => {
                    let _ = video.stop();
                    let _ = fs::remove_file(&first_segment);
                    return Err(error.context("Unable to start native Windows system audio"));
                }
            }
        } else {
            (None, Vec::new())
        };

        *guard = Some(ActiveRecording {
            id: id.clone(),
            config,
            started_at: Instant::now(),
            paused_total_ms: 0,
            paused_at: None,
            final_path,
            segment_paths: vec![first_segment],
            system_audio_paths,
            current_video: Some(video),
            current_system_audio,
            external_system_audio,
            source_width,
            source_height,
            width,
            height,
        });
        Ok(id)
    }

    pub fn pause(&self) -> Result<()> {
        let mut guard = self.active.lock();
        let rec = guard
            .as_mut()
            .ok_or_else(|| anyhow!("No active recording"))?;
        if rec.paused_at.is_some() {
            return Ok(());
        }

        // Each active interval remains a self-contained MP4. Stop/finalization now
        // prefers Windows MediaComposition, so Pause itself does not require FFmpeg.
        if let Some(system) = rec.current_system_audio.take() {
            system.stop()?;
        }
        if let Some(video) = rec.current_video.take() {
            video.stop()?;
        }
        rec.paused_at = Some(Instant::now());
        Ok(())
    }

    pub fn resume(&self) -> Result<()> {
        let mut guard = self.active.lock();
        let rec = guard
            .as_mut()
            .ok_or_else(|| anyhow!("No active recording"))?;
        let Some(paused_at) = rec.paused_at else {
            return Ok(());
        };

        // Resolve again so disconnected monitors/closed windows produce a useful
        // resume error rather than silently continuing against a stale handle.
        let source = capture::resolve_target(&rec.config.target)?;
        if source.width != rec.source_width || source.height != rec.source_height {
            return Err(anyhow!(
                "The capture source size changed while paused ({}x{} -> {}x{}). Restore the original size and resume again.",
                rec.source_width,
                rec.source_height,
                source.width,
                source.height
            ));
        }

        let next_index = rec.segment_paths.len();
        let path = segment_path(&rec.final_path, &rec.id, next_index);
        let video = capture::start_native_video_capture(
            &rec.config,
            &rec.config.target,
            rec.width,
            rec.height,
            &path,
        )?;
        let segment_external_system_audio =
            rec.config.system_audio && !video.captures_system_audio();

        // Keep every segment on the same audio ownership model. This prevents a
        // mid-recording native-encoder fallback from producing incompatible tracks.
        if segment_external_system_audio != rec.external_system_audio {
            let _ = video.stop();
            let _ = fs::remove_file(&path);
            return Err(anyhow!(
                "The Windows recording backend changed while paused. Start a new recording so audio routing stays consistent."
            ));
        }

        let system_capture = if rec.external_system_audio {
            let system_path = system_audio_path(&rec.final_path, &rec.id, next_index);
            match audio::start_system_audio_capture(&system_path) {
                Ok(capture) => {
                    rec.system_audio_paths.push(system_path);
                    Some(capture)
                }
                Err(error) => {
                    let _ = video.stop();
                    let _ = fs::remove_file(&path);
                    return Err(error.context("Unable to resume native Windows system audio"));
                }
            }
        } else {
            None
        };

        rec.segment_paths.push(path);
        rec.current_video = Some(video);
        rec.current_system_audio = system_capture;
        rec.paused_total_ms = rec
            .paused_total_ms
            .saturating_add(paused_at.elapsed().as_millis() as u64);
        rec.paused_at = None;
        Ok(())
    }

    pub fn stop(&self) -> Result<RecordingOutput> {
        let mut guard = self.active.lock();
        let mut rec = guard.take().ok_or_else(|| anyhow!("No active recording"))?;

        if let Some(system) = rec.current_system_audio.take() {
            system.stop()?;
        }
        if let Some(video) = rec.current_video.take() {
            video.stop()?;
        }

        let paused = rec.paused_total_ms
            + rec
                .paused_at
                .map(|at| at.elapsed().as_millis() as u64)
                .unwrap_or(0);
        let duration_ms = (Instant::now().duration_since(rec.started_at).as_millis() as u64)
            .saturating_sub(paused)
            .max(1);

        let final_segments = if rec.external_system_audio {
            mix_native_system_audio_segments(
                &rec.segment_paths,
                &rec.system_audio_paths,
                rec.config.microphone_id.is_some(),
            )?
        } else {
            rec.segment_paths.clone()
        };

        finalize_segments(&final_segments, &rec.final_path)?;
        let metadata = fs::metadata(&rec.final_path).with_context(|| {
            format!(
                "Recording file was not created: {}",
                rec.final_path.display()
            )
        })?;
        if metadata.len() == 0 {
            return Err(anyhow!("Recording completed but the MP4 file is empty"));
        }

        // Thumbnail generation is best-effort. Native recordings therefore remain
        // usable even when FFmpeg is intentionally absent from the machine.
        let thumbnail_data_url = encoding::thumbnail_data_url(&rec.final_path);

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
            thumbnail_data_url,
        })
    }
}

fn normalize_crop_region(
    config: &mut RecordingConfig,
    source_width: u32,
    source_height: u32,
) -> Result<(u32, u32)> {
    let full_width = source_width & !1;
    let full_height = source_height & !1;
    if full_width < 2 || full_height < 2 {
        return Err(anyhow!("The selected capture source has an invalid size"));
    }

    let Some(mut crop) = config.crop_region else {
        return Ok((full_width, full_height));
    };

    crop.width &= !1;
    crop.height &= !1;
    if crop.width < 64 || crop.height < 64 {
        return Err(anyhow!(
            "The selected recording area must be at least 64 × 64 pixels"
        ));
    }

    let right = crop
        .x
        .checked_add(crop.width)
        .ok_or_else(|| anyhow!("The selected recording area overflowed horizontally"))?;
    let bottom = crop
        .y
        .checked_add(crop.height)
        .ok_or_else(|| anyhow!("The selected recording area overflowed vertically"))?;
    if right > source_width || bottom > source_height {
        return Err(anyhow!(
            "The selected recording area ({}x{} at {},{}) is outside the source ({}x{})",
            crop.width,
            crop.height,
            crop.x,
            crop.y,
            source_width,
            source_height
        ));
    }

    if crop.x == 0 && crop.y == 0 && crop.width == full_width && crop.height == full_height {
        config.crop_region = None;
        return Ok((full_width, full_height));
    }

    config.crop_region = Some(crop);
    Ok((crop.width, crop.height))
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

fn system_audio_path(final_path: &Path, id: &str, index: usize) -> PathBuf {
    let parent = final_path.parent().unwrap_or_else(|| Path::new("."));
    parent.join(format!("{id}.system{index:03}.wav"))
}

fn mixed_segment_path(segment: &Path) -> PathBuf {
    let parent = segment.parent().unwrap_or_else(|| Path::new("."));
    let stem = segment
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("segment");
    parent.join(format!("{stem}.mixed.mp4"))
}

fn mix_native_system_audio_segments(
    video_segments: &[PathBuf],
    system_tracks: &[PathBuf],
    has_microphone: bool,
) -> Result<Vec<PathBuf>> {
    if video_segments.len() != system_tracks.len() {
        return Err(anyhow!(
            "System-audio segment count does not match video segment count"
        ));
    }

    let mut outputs = Vec::with_capacity(video_segments.len());
    for (video, system) in video_segments.iter().zip(system_tracks) {
        // A WAV header with no PCM is valid when the output endpoint produced no
        // packets. Keep the video/mic segment unchanged instead of failing stop.
        if fs::metadata(system).map(|m| m.len()).unwrap_or(0) <= 64 {
            let _ = fs::remove_file(system);
            outputs.push(video.clone());
            continue;
        }

        let output = mixed_segment_path(video);
        let mut cmd = encoding::ffmpeg_command();
        cmd.args(["-hide_banner", "-loglevel", "warning", "-y"])
            .arg("-i")
            .arg(video)
            .arg("-i")
            .arg(system);

        if has_microphone {
            cmd.args([
                "-filter_complex",
                "[0:a]aresample=async=1:first_pts=0[mic];[1:a]aresample=async=1:first_pts=0[sys];[mic][sys]amix=inputs=2:duration=first:dropout_transition=2[a]",
                "-map",
                "0:v:0",
                "-map",
                "[a]",
            ]);
        } else {
            cmd.args(["-map", "0:v:0", "-map", "1:a:0"]);
        }

        let status = cmd
            .args([
                "-c:v",
                "copy",
                "-c:a",
                "aac",
                "-b:a",
                "192k",
                "-shortest",
                "-movflags",
                "+faststart",
            ])
            .arg(&output)
            .status()
            .context("Unable to mix native Windows system audio into recording")?;
        if !status.success() {
            return Err(anyhow!(
                "FFmpeg could not mux the native system-audio track"
            ));
        }

        let _ = fs::remove_file(video);
        let _ = fs::remove_file(system);
        outputs.push(output);
    }
    Ok(outputs)
}

fn finalize_segments(segments: &[PathBuf], final_path: &Path) -> Result<()> {
    let total_started = Instant::now();
    if segments.is_empty() {
        eprintln!(
            "[Recorder][FinalizationHealth] stage=complete ok=false method=none segment_count=0 native_ms=0 fallback_ms=0 total_ms={} code=no_segments",
            total_started.elapsed().as_millis()
        );
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
        eprintln!(
            "[Recorder][FinalizationHealth] stage=complete ok=true method=single_segment segment_count=1 native_ms=0 fallback_ms=0 total_ms={}",
            total_started.elapsed().as_millis()
        );
        return Ok(());
    }

    #[cfg(windows)]
    let native_ms = {
        let native_started = Instant::now();
        match windows_media::concatenate_segments(segments, final_path) {
            Ok(()) => {
                let native_ms = native_started.elapsed().as_millis();
                eprintln!(
                    "[Recorder] Native Windows MediaComposition pause/resume finalizer active"
                );
                eprintln!(
                    "[Recorder][FinalizationHealth] stage=complete ok=true method=windows-mediacomposition segment_count={} native_ms={native_ms} fallback_ms=0 total_ms={}",
                    segments.len(),
                    total_started.elapsed().as_millis()
                );
                return Ok(());
            }
            Err(_) => {
                let native_ms = native_started.elapsed().as_millis();
                eprintln!(
                    "[Recorder] Native Windows pause/resume finalization unavailable; falling back to bounded FFmpeg concat"
                );
                eprintln!(
                    "[Recorder][FinalizationHealth] stage=native_complete ok=false method=windows-mediacomposition segment_count={} native_ms={native_ms} total_ms={}",
                    segments.len(),
                    total_started.elapsed().as_millis()
                );
                let _ = fs::remove_file(final_path);
                native_ms
            }
        }
    };
    #[cfg(not(windows))]
    let native_ms = 0u128;

    let fallback_result = ffmpeg_finalizer::concatenate_segments(segments, final_path);
    match fallback_result {
        Ok(fallback_ms) => {
            eprintln!(
                "[Recorder][FinalizationHealth] stage=complete ok=true method=ffmpeg-fallback segment_count={} native_ms={native_ms} fallback_ms={fallback_ms} total_ms={}",
                segments.len(),
                total_started.elapsed().as_millis()
            );
            Ok(())
        }
        Err(error) => {
            let fallback_ms = total_started
                .elapsed()
                .as_millis()
                .saturating_sub(native_ms);
            eprintln!(
                "[Recorder][FinalizationHealth] stage=complete ok=false method=ffmpeg-fallback segment_count={} native_ms={native_ms} fallback_ms={fallback_ms} total_ms={}",
                segments.len(),
                total_started.elapsed().as_millis()
            );
            Err(error)
        }
    }
}
