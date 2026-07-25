use crate::{recording::types::*, state::AppState};
use serde::Serialize;
use tauri::State;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartResponse {
    pub id: String,
}

#[tauri::command]
pub fn start_recording(
    config: RecordingConfig,
    state: State<'_, AppState>,
) -> Result<StartResponse, String> {
    state
        .engine
        .start(config)
        .map(|id| StartResponse { id })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn pause_recording(state: State<'_, AppState>) -> Result<(), String> {
    state.engine.pause().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn resume_recording(state: State<'_, AppState>) -> Result<(), String> {
    state.engine.resume().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn stop_recording(state: State<'_, AppState>) -> Result<RecordingOutput, String> {
    let output = state.engine.stop().map_err(|e| e.to_string())?;
    state.library.recordings.write().insert(0, output.clone());
    // TODO(native): persist the updated library to disk.
    Ok(output)
}
