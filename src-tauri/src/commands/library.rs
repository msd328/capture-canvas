use crate::{recording::types::*, security, state::AppState};
use std::fs;
use std::process::Command;
use tauri::State;

#[tauri::command]
pub fn get_recordings(state: State<'_, AppState>) -> Vec<RecordingOutput> {
    state.library.recordings.read().clone()
}

#[tauri::command]
pub fn get_recording(id: String, state: State<'_, AppState>) -> Option<RecordingOutput> {
    if security::validate_recording_id(&id).is_err() {
        return None;
    }
    state
        .library
        .recordings
        .read()
        .iter()
        .find(|recording| recording.id == id)
        .cloned()
}

#[tauri::command]
pub fn delete_recording(id: String, state: State<'_, AppState>) -> Result<(), String> {
    security::validate_recording_id(&id)?;
    let raw_path = state
        .library
        .recordings
        .read()
        .iter()
        .find(|recording| recording.id == id)
        .map(|recording| recording.file_path.clone())
        .ok_or_else(|| "Recording not found".to_string())?;
    let path = security::validate_existing_recording_path(&id, &raw_path)?;

    fs::remove_file(&path)
        .map_err(|error| format!("Unable to delete approved recording file: {error}"))?;
    state
        .library
        .recordings
        .write()
        .retain(|recording| recording.id != id);
    state.library.persist()?;
    Ok(())
}

#[tauri::command]
pub fn rename_recording(
    id: String,
    title: String,
    state: State<'_, AppState>,
) -> Result<RecordingOutput, String> {
    security::validate_recording_id(&id)?;
    let title = security::validate_title(&title)?;
    let updated = {
        let mut guard = state.library.recordings.write();
        let recording = guard
            .iter_mut()
            .find(|recording| recording.id == id)
            .ok_or_else(|| "Recording not found".to_string())?;
        security::validate_existing_recording_path(&recording.id, &recording.file_path)?;
        recording.title = title;
        recording.clone()
    };
    state.library.persist()?;
    Ok(updated)
}

#[tauri::command]
pub fn open_recording_location(id: String, state: State<'_, AppState>) -> Result<(), String> {
    security::validate_recording_id(&id)?;
    let raw_path = state
        .library
        .recordings
        .read()
        .iter()
        .find(|recording| recording.id == id)
        .map(|recording| recording.file_path.clone())
        .ok_or_else(|| "Recording not found".to_string())?;
    let path = security::validate_existing_recording_path(&id, &raw_path)?;

    #[cfg(windows)]
    {
        Command::new("explorer.exe")
            .arg(format!("/select,{}", path.display()))
            .spawn()
            .map_err(|error| format!("Unable to open File Explorer: {error}"))?;
        return Ok(());
    }

    #[cfg(not(windows))]
    {
        let _ = path;
        Err("Open file location is not implemented for this OS yet".to_string())
    }
}
