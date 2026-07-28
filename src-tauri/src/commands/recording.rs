use crate::{recording::types::*, state::AppState};
use serde::Serialize;
use tauri::State;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartResponse {
    pub id: String,
}

fn worker_error(operation: &str, error: impl std::fmt::Display) -> String {
    format!("Recorder {operation} worker failed: {error}")
}

#[tauri::command]
pub async fn start_recording(
    config: RecordingConfig,
    state: State<'_, AppState>,
) -> Result<StartResponse, String> {
    let engine = state.engine.clone();
    let result = tauri::async_runtime::spawn_blocking(move || engine.start(config))
        .await
        .map_err(|error| worker_error("start", error))?;

    result
        .map(|id| StartResponse { id })
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn pause_recording(state: State<'_, AppState>) -> Result<(), String> {
    let engine = state.engine.clone();
    tauri::async_runtime::spawn_blocking(move || engine.pause())
        .await
        .map_err(|error| worker_error("pause", error))?
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn resume_recording(state: State<'_, AppState>) -> Result<(), String> {
    let engine = state.engine.clone();
    tauri::async_runtime::spawn_blocking(move || engine.resume())
        .await
        .map_err(|error| worker_error("resume", error))?
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn stop_recording(state: State<'_, AppState>) -> Result<RecordingOutput, String> {
    let engine = state.engine.clone();
    let output = tauri::async_runtime::spawn_blocking(move || engine.stop())
        .await
        .map_err(|error| worker_error("stop", error))?
        .map_err(|error| error.to_string())?;

    state.library.recordings.write().insert(0, output.clone());
    state.library.persist()?;
    Ok(output)
}
