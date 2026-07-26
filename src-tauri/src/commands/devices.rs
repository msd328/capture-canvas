use crate::{audio, camera, capture, recording::types::*};

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
pub fn get_camera_preview_frame(camera_id: String) -> Result<String, String> {
    camera::preview_frame_data_url(&camera_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn system_audio_supported() -> bool {
    audio::system_audio_supported()
}
