//! Security boundaries shared by Tauri commands and local persistence.
//!
//! The current desktop product stores completed recordings as direct children of
//! the user's `Recordings` directory using the generated recording UUID as the
//! filename. Keeping that contract narrow makes it possible to reject tampered
//! metadata before Rust opens, thumbnails, or deletes an unrelated local file.

use crate::recording::types::{RecorderSettings, RecordingConfig};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};
use uuid::Uuid;

const MAX_TITLE_CHARS: usize = 200;
const MAX_DEVICE_ID_CHARS: usize = 1_024;
const STALE_ARTIFACT_MIN_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const MAX_CLEANUP_ENTRIES: usize = 4_096;

#[derive(Default)]
struct CleanupSummary {
    scanned: usize,
    matched: usize,
    removed: usize,
    recent: usize,
    rejected: usize,
    remove_failed: usize,
    scan_limited: bool,
}

struct CleanupFailure {
    stage: &'static str,
    code: String,
}

impl CleanupFailure {
    fn new(stage: &'static str, code: impl Into<String>) -> Self {
        Self {
            stage,
            code: code.into(),
        }
    }
}

fn user_home() -> Result<PathBuf, String> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .ok_or_else(|| "Unable to determine the current user's home directory".to_string())
}

fn recordings_root_display_path() -> Result<PathBuf, String> {
    Ok(user_home()?.join("Recordings"))
}

/// Return the canonical app-approved directory for security comparisons.
pub fn recordings_root() -> Result<PathBuf, String> {
    let root = recordings_root_display_path()?;
    fs::create_dir_all(&root)
        .map_err(|error| format!("Unable to create the approved recording directory: {error}"))?;
    fs::canonicalize(&root)
        .map_err(|error| format!("Unable to resolve the approved recording directory: {error}"))
}

/// Return the normal user-facing path rather than Windows' extended canonical path.
pub fn recordings_root_string() -> Result<String, String> {
    Ok(recordings_root_display_path()?
        .to_string_lossy()
        .into_owned())
}

/// Remove abandoned recorder-owned temporary media after a conservative age gate.
///
/// Cleanup runs outside the UI thread, scans only direct children of the approved
/// Recordings root, rejects links and non-regular files, and never matches the final
/// `<UUID>.mp4` recording contract. The 24-hour threshold avoids deleting files from
/// another Recorder process that may still be capturing or finalising.
pub fn cleanup_stale_recording_artifacts_async() {
    let spawn_result = std::thread::Builder::new()
        .name("recorder-orphan-cleanup".to_string())
        .spawn(|| {
            let started = Instant::now();
            match cleanup_stale_recording_artifacts() {
                Ok(summary) => eprintln!(
                    "[Recorder][CleanupHealth] stage=complete ok={} scanned={} matched={} removed={} recent={} rejected={} remove_failed={} scan_limited={} min_age_hours={} elapsed_ms={}",
                    summary.remove_failed == 0,
                    summary.scanned,
                    summary.matched,
                    summary.removed,
                    summary.recent,
                    summary.rejected,
                    summary.remove_failed,
                    summary.scan_limited,
                    STALE_ARTIFACT_MIN_AGE.as_secs() / 3_600,
                    started.elapsed().as_millis()
                ),
                Err(error) => eprintln!(
                    "[Recorder][CleanupHealth] stage={} ok=false code={} elapsed_ms={}",
                    error.stage,
                    error.code,
                    started.elapsed().as_millis()
                ),
            }
        });

    if let Err(error) = spawn_result {
        eprintln!(
            "[Recorder][CleanupHealth] stage=spawn ok=false code={:?} elapsed_ms=0",
            error.kind()
        );
    }
}

fn cleanup_stale_recording_artifacts() -> Result<CleanupSummary, CleanupFailure> {
    let root = recordings_root().map_err(|_| CleanupFailure::new("resolve_root", "unavailable"))?;
    let entries = fs::read_dir(&root)
        .map_err(|error| CleanupFailure::new("read_root", format!("{:?}", error.kind())))?;
    let now = SystemTime::now();
    let mut summary = CleanupSummary::default();

    for entry_result in entries {
        if summary.scanned >= MAX_CLEANUP_ENTRIES {
            summary.scan_limited = true;
            break;
        }
        summary.scanned = summary.scanned.saturating_add(1);

        let entry = match entry_result {
            Ok(entry) => entry,
            Err(_) => {
                summary.rejected = summary.rejected.saturating_add(1);
                continue;
            }
        };
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        if !is_recorder_temporary_artifact(file_name) {
            continue;
        }
        summary.matched = summary.matched.saturating_add(1);

        let metadata = match fs::symlink_metadata(entry.path()) {
            Ok(metadata) => metadata,
            Err(_) => {
                summary.rejected = summary.rejected.saturating_add(1);
                continue;
            }
        };
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            summary.rejected = summary.rejected.saturating_add(1);
            continue;
        }

        let modified = match metadata.modified() {
            Ok(modified) => modified,
            Err(_) => {
                summary.rejected = summary.rejected.saturating_add(1);
                continue;
            }
        };
        let age = match now.duration_since(modified) {
            Ok(age) => age,
            Err(_) => {
                summary.recent = summary.recent.saturating_add(1);
                continue;
            }
        };
        if age < STALE_ARTIFACT_MIN_AGE {
            summary.recent = summary.recent.saturating_add(1);
            continue;
        }

        match fs::remove_file(entry.path()) {
            Ok(()) => summary.removed = summary.removed.saturating_add(1),
            Err(_) => summary.remove_failed = summary.remove_failed.saturating_add(1),
        }
    }

    Ok(summary)
}

fn is_recorder_temporary_artifact(file_name: &str) -> bool {
    if let Some(candidate) = file_name.strip_prefix('.') {
        let Some(suffix) = canonical_uuid_suffix(candidate) else {
            return false;
        };
        let identity = [".native-finalizing-", ".ffmpeg-finalizing-"]
            .into_iter()
            .find_map(|prefix| {
                suffix
                    .strip_prefix(prefix)
                    .and_then(|value| value.strip_suffix(".mp4"))
            });
        let Some(identity) = identity else {
            return false;
        };
        let Some((process_id, nonce)) = identity.split_once('-') else {
            return false;
        };
        return is_ascii_digits(process_id) && is_ascii_digits(nonce);
    }

    let Some(suffix) = canonical_uuid_suffix(file_name) else {
        return false;
    };
    if let Some(index) = suffix
        .strip_prefix(".part")
        .and_then(|value| value.strip_suffix(".mixed.mp4"))
    {
        return is_three_digit_index(index);
    }
    if let Some(index) = suffix
        .strip_prefix(".part")
        .and_then(|value| value.strip_suffix(".mp4"))
    {
        return is_three_digit_index(index);
    }
    if let Some(index) = suffix
        .strip_prefix(".system")
        .and_then(|value| value.strip_suffix(".wav"))
    {
        return is_three_digit_index(index);
    }
    false
}

fn canonical_uuid_suffix(value: &str) -> Option<&str> {
    const UUID_BYTES: usize = 36;
    let bytes = value.as_bytes();
    if bytes.len() < UUID_BYTES {
        return None;
    }
    let id = std::str::from_utf8(&bytes[..UUID_BYTES]).ok()?;
    let parsed = Uuid::parse_str(id).ok()?;
    if parsed.to_string() != id {
        return None;
    }
    value.get(UUID_BYTES..)
}

fn is_three_digit_index(value: &str) -> bool {
    value.len() == 3 && is_ascii_digits(value)
}

fn is_ascii_digits(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}

pub fn validate_recording_id(id: &str) -> Result<(), String> {
    let parsed = Uuid::parse_str(id).map_err(|_| "Invalid recording ID".to_string())?;
    if parsed.to_string() != id {
        return Err("Recording ID must use the canonical lowercase UUID format".to_string());
    }
    Ok(())
}

pub fn validate_title(title: &str) -> Result<String, String> {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return Err("Recording title cannot be empty".to_string());
    }
    if trimmed.chars().count() > MAX_TITLE_CHARS {
        return Err(format!(
            "Recording title cannot exceed {MAX_TITLE_CHARS} characters"
        ));
    }
    if trimmed.chars().any(char::is_control) {
        return Err("Recording title cannot contain control characters".to_string());
    }
    Ok(trimmed.to_string())
}

fn validate_device_id(label: &str, value: Option<&str>) -> Result<(), String> {
    let Some(value) = value else {
        return Ok(());
    };
    if value.trim().is_empty() {
        return Err(format!("{label} device ID cannot be empty"));
    }
    if value.chars().count() > MAX_DEVICE_ID_CHARS {
        return Err(format!(
            "{label} device ID cannot exceed {MAX_DEVICE_ID_CHARS} characters"
        ));
    }
    if value.chars().any(char::is_control) {
        return Err(format!("{label} device ID contains invalid characters"));
    }
    Ok(())
}

/// Validate untrusted values supplied by the WebView before starting native work.
pub fn validate_start_config(config: &RecordingConfig) -> Result<(), String> {
    if config
        .output_path
        .as_deref()
        .is_some_and(|path| !path.trim().is_empty())
    {
        return Err(
            "Custom output paths are disabled. Recordings are saved in the approved Recordings directory."
                .to_string(),
        );
    }
    if !matches!(config.fps, 30 | 60) {
        return Err("Recording FPS must be either 30 or 60".to_string());
    }
    validate_device_id("Microphone", config.microphone_id.as_deref())?;
    validate_device_id("Camera", config.camera_id.as_deref())?;
    if let Some(title) = config.title.as_deref() {
        validate_title(title)?;
    }
    Ok(())
}

/// Normalise persisted settings to the single approved recording directory.
pub fn validate_settings(mut settings: RecorderSettings) -> Result<RecorderSettings, String> {
    if !matches!(settings.fps, 30 | 60) {
        return Err("Default recording FPS must be either 30 or 60".to_string());
    }
    validate_device_id(
        "Default microphone",
        settings.default_microphone_id.as_deref(),
    )?;
    validate_device_id("Default camera", settings.default_camera_id.as_deref())?;

    let approved = recordings_root()?;
    let requested = expand_tilde(&settings.output_directory)?;
    let requested_is_approved = if requested.exists() {
        fs::canonicalize(&requested)
            .map(|path| path == approved)
            .unwrap_or(false)
    } else {
        requested == recordings_root_display_path()?
    };
    if !requested_is_approved {
        return Err(
            "The output directory must be the app-approved Recordings directory".to_string(),
        );
    }
    settings.output_directory = recordings_root_string()?;
    Ok(settings)
}

fn expand_tilde(value: &str) -> Result<PathBuf, String> {
    let path = value.trim();
    if path == "~" {
        return user_home();
    }
    if let Some(rest) = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        return Ok(user_home()?.join(rest));
    }
    Ok(PathBuf::from(path))
}

/// Validate and canonicalise an existing completed recording from persisted metadata.
pub fn validate_existing_recording_path(id: &str, raw_path: &str) -> Result<PathBuf, String> {
    validate_recording_id(id)?;
    let expected_name = format!("{id}.mp4");
    let candidate = Path::new(raw_path);
    if candidate.file_name() != Some(OsStr::new(&expected_name)) {
        return Err("Recording filename does not match its recording ID".to_string());
    }

    let canonical = fs::canonicalize(candidate)
        .map_err(|error| format!("Unable to resolve recording file: {error}"))?;
    let approved = recordings_root()?;
    if canonical.parent() != Some(approved.as_path()) {
        return Err("Recording file is outside the approved Recordings directory".to_string());
    }
    if canonical.file_name() != Some(OsStr::new(&expected_name)) {
        return Err("Resolved recording filename does not match its recording ID".to_string());
    }

    let metadata = fs::metadata(&canonical)
        .map_err(|error| format!("Unable to inspect recording file: {error}"))?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err("Recording path is not a non-empty regular file".to_string());
    }
    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use super::is_recorder_temporary_artifact;

    const ID: &str = "123e4567-e89b-12d3-a456-426614174000";

    #[test]
    fn recognises_only_recorder_owned_temporary_names() {
        assert!(is_recorder_temporary_artifact(&format!("{ID}.part000.mp4")));
        assert!(is_recorder_temporary_artifact(&format!(
            "{ID}.part123.mixed.mp4"
        )));
        assert!(is_recorder_temporary_artifact(&format!(
            "{ID}.system009.wav"
        )));
        assert!(is_recorder_temporary_artifact(&format!(
            ".{ID}.native-finalizing-1234-987654321.mp4"
        )));
        assert!(is_recorder_temporary_artifact(&format!(
            ".{ID}.ffmpeg-finalizing-1234-987654321.mp4"
        )));

        assert!(!is_recorder_temporary_artifact(&format!("{ID}.mp4")));
        assert!(!is_recorder_temporary_artifact(&format!("{ID}.part00.mp4")));
        assert!(!is_recorder_temporary_artifact(&format!(
            "{ID}.part000.mov"
        )));
        assert!(!is_recorder_temporary_artifact(&format!(
            ".{ID}.native-finalizing-process-nonce.mp4"
        )));
        assert!(!is_recorder_temporary_artifact(&format!(
            ".{ID}.ffmpeg-finalizing-process-nonce.mp4"
        )));
        assert!(!is_recorder_temporary_artifact(
            "123E4567-E89B-12D3-A456-426614174000.part000.mp4"
        ));
        assert!(!is_recorder_temporary_artifact("unrelated.tmp"));
    }
}
