use crate::{audio, camera, capture, recording::types::*};
use tauri::State;

#[tauri::command]
pub fn list_displays() -> Vec<DisplayInfo> {
    capture::enumerate_displays()
}

#[tauri::command]
pub fn list_windows() -> Vec<WindowInfo> {
    capture::enumerate_windows()
}

#[tauri::command]
pub fn list_microphones() -> Vec<MicrophoneInfo> {
    audio::enumerate_microphones()
}

#[tauri::command]
pub fn list_cameras() -> Vec<CameraInfo> {
    camera::enumerate_cameras()
}

#[tauri::command]
pub fn system_audio_supported() -> bool {
    audio::system_audio_supported()
}

#[tauri::command]
pub async fn capture_source_preview(
    target: CaptureTarget,
    _state: State<'_, crate::state::AppState>,
) -> Result<CapturePreview, String> {
    tauri::async_runtime::spawn_blocking(move || capture::capture_source_preview(&target))
        .await
        .map_err(|error| format!("Source preview worker failed: {error}"))?
        .map_err(|error| error.to_string())
}
