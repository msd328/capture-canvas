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
use uuid::Uuid;

const MAX_TITLE_CHARS: usize = 200;
const MAX_DEVICE_ID_CHARS: usize = 1_024;

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
    Ok(recordings_root_display_path()?.to_string_lossy().into_owned())
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
    validate_device_id("Default microphone", settings.default_microphone_id.as_deref())?;
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
    if let Some(rest) = path
        .strip_prefix("~/")
        .or_else(|| path.strip_prefix("~\\"))
    {
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
