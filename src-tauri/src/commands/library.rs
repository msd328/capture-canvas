use crate::{recording::types::*, state::AppState};
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
    // TODO(native): also delete the MP4 file from disk.
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
pub fn open_recording_location(_id: String) -> Result<(), String> {
    // TODO(native): reveal the file in Finder/Explorer via tauri-plugin-opener.
    Ok(())
}
