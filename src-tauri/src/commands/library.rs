use crate::{recording::types::*, state::AppState};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tauri::State;

#[tauri::command]
pub fn get_recordings(state: State<'_, AppState>) -> Vec<RecordingOutput> {
    state.library.recordings.read().clone()
}

#[tauri::command]
pub fn get_recording(id: String, state: State<'_, AppState>) -> Option<RecordingOutput> {
    state
        .library
        .recordings
        .read()
        .iter()
        .find(|r| r.id == id)
        .cloned()
}

#[tauri::command]
pub fn delete_recording(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let path = state
        .library
        .recordings
        .read()
        .iter()
        .find(|r| r.id == id)
        .map(|r| PathBuf::from(&r.file_path));

    if let Some(path) = path {
        if path.exists() {
            fs::remove_file(&path)
                .map_err(|e| format!("Unable to delete {}: {e}", path.display()))?;
        }
    }
    state.library.recordings.write().retain(|r| r.id != id);
    Ok(())
}

#[tauri::command]
pub fn rename_recording(
    id: String,
    title: String,
    state: State<'_, AppState>,
) -> Result<RecordingOutput, String> {
    let mut guard = state.library.recordings.write();
    let rec = guard
        .iter_mut()
        .find(|r| r.id == id)
        .ok_or_else(|| "Recording not found".to_string())?;
    rec.title = title;
    Ok(rec.clone())
}

#[tauri::command]
pub fn open_recording_location(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let path = state
        .library
        .recordings
        .read()
        .iter()
        .find(|r| r.id == id)
        .map(|r| PathBuf::from(&r.file_path))
        .ok_or_else(|| "Recording not found".to_string())?;

    #[cfg(windows)]
    {
        Command::new("explorer.exe")
            .arg(format!("/select,{}", path.display()))
            .spawn()
            .map_err(|e| format!("Unable to open File Explorer: {e}"))?;
        return Ok(());
    }

    #[cfg(not(windows))]
    {
        let _ = path;
        Err("Open file location is not implemented for this OS yet".to_string())
    }
}
