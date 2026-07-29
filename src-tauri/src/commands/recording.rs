use crate::{recording::types::*, security, state::AppState};
use serde::Serialize;
use std::time::Instant;
use tauri::State;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartResponse {
    pub id: String,
}

fn worker_error(operation: &str, error: impl std::fmt::Display) -> String {
    format!("Recorder {operation} worker failed: {error}")
}

fn log_control_failure(operation: &str, stage: &str, started: Instant) {
    eprintln!(
        "[Recorder][ControlHealth] operation={operation} ok=false stage={stage} total_ms={}",
        started.elapsed().as_millis(),
    );
}

#[tauri::command]
pub async fn start_recording(
    mut config: RecordingConfig,
    state: State<'_, AppState>,
) -> Result<StartResponse, String> {
    let command_started = Instant::now();
    let validation_started = Instant::now();

    if let Err(error) = security::validate_start_config(&config) {
        log_control_failure("start", "validation", command_started);
        return Err(error);
    }
    if let Some(title) = config.title.take() {
        match security::validate_title(&title) {
            Ok(title) => config.title = Some(title),
            Err(error) => {
                log_control_failure("start", "title_validation", command_started);
                return Err(error);
            }
        }
    }
    let validation_ms = validation_started.elapsed().as_millis();

    let engine = state.engine.clone();
    let engine_started = Instant::now();
    let worker_result = tauri::async_runtime::spawn_blocking(move || engine.start(config)).await;
    let engine_ms = engine_started.elapsed().as_millis();
    let result = match worker_result {
        Ok(result) => result,
        Err(error) => {
            let error = worker_error("start", error);
            log_control_failure("start", "worker_join", command_started);
            return Err(error);
        }
    };

    match result {
        Ok(id) => {
            eprintln!(
                "[Recorder][ControlHealth] operation=start ok=true validation_ms={validation_ms} engine_ms={engine_ms} total_ms={}",
                command_started.elapsed().as_millis(),
            );
            Ok(StartResponse { id })
        }
        Err(error) => {
            log_control_failure("start", "engine", command_started);
            Err(error.to_string())
        }
    }
}

#[tauri::command]
pub async fn pause_recording(state: State<'_, AppState>) -> Result<(), String> {
    let command_started = Instant::now();
    let engine = state.engine.clone();
    let engine_started = Instant::now();
    let worker_result = tauri::async_runtime::spawn_blocking(move || engine.pause()).await;
    let engine_ms = engine_started.elapsed().as_millis();
    let result = match worker_result {
        Ok(result) => result,
        Err(error) => {
            let error = worker_error("pause", error);
            log_control_failure("pause", "worker_join", command_started);
            return Err(error);
        }
    };

    match result {
        Ok(()) => {
            eprintln!(
                "[Recorder][ControlHealth] operation=pause ok=true engine_ms={engine_ms} total_ms={}",
                command_started.elapsed().as_millis(),
            );
            Ok(())
        }
        Err(error) => {
            log_control_failure("pause", "engine", command_started);
            Err(error.to_string())
        }
    }
}

#[tauri::command]
pub async fn resume_recording(state: State<'_, AppState>) -> Result<(), String> {
    let command_started = Instant::now();
    let engine = state.engine.clone();
    let engine_started = Instant::now();
    let worker_result = tauri::async_runtime::spawn_blocking(move || engine.resume()).await;
    let engine_ms = engine_started.elapsed().as_millis();
    let result = match worker_result {
        Ok(result) => result,
        Err(error) => {
            let error = worker_error("resume", error);
            log_control_failure("resume", "worker_join", command_started);
            return Err(error);
        }
    };

    match result {
        Ok(()) => {
            eprintln!(
                "[Recorder][ControlHealth] operation=resume ok=true engine_ms={engine_ms} total_ms={}",
                command_started.elapsed().as_millis(),
            );
            Ok(())
        }
        Err(error) => {
            log_control_failure("resume", "engine", command_started);
            Err(error.to_string())
        }
    }
}

#[tauri::command]
pub async fn stop_recording(state: State<'_, AppState>) -> Result<RecordingOutput, String> {
    let command_started = Instant::now();
    let engine = state.engine.clone();
    let engine_started = Instant::now();
    let worker_result = tauri::async_runtime::spawn_blocking(move || engine.stop()).await;
    let engine_ms = engine_started.elapsed().as_millis();
    let output = match worker_result {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => {
            log_control_failure("stop", "engine", command_started);
            return Err(error.to_string());
        }
        Err(error) => {
            let error = worker_error("stop", error);
            log_control_failure("stop", "worker_join", command_started);
            return Err(error);
        }
    };

    let validation_started = Instant::now();
    if let Err(error) = security::validate_existing_recording_path(&output.id, &output.file_path) {
        log_control_failure("stop", "path_validation", command_started);
        return Err(error);
    }
    let path_validation_ms = validation_started.elapsed().as_millis();

    let insert_started = Instant::now();
    state.library.recordings.write().insert(0, output.clone());
    let library_insert_ms = insert_started.elapsed().as_millis();

    let persist_started = Instant::now();
    if let Err(error) = state.library.persist() {
        log_control_failure("stop", "library_persist", command_started);
        return Err(error);
    }
    let library_persist_ms = persist_started.elapsed().as_millis();

    let thumbnail_started = Instant::now();
    state
        .library
        .schedule_thumbnail_async(output.id.clone(), output.file_path.clone());
    let thumbnail_schedule_ms = thumbnail_started.elapsed().as_millis();

    eprintln!(
        "[Recorder][ControlHealth] operation=stop ok=true engine_ms={engine_ms} path_validation_ms={path_validation_ms} library_insert_ms={library_insert_ms} library_persist_ms={library_persist_ms} thumbnail_schedule_ms={thumbnail_schedule_ms} total_ms={}",
        command_started.elapsed().as_millis(),
    );
    Ok(output)
}
