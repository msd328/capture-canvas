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
pub fn system_audio_supported() -> bool {
    audio::system_audio_supported()
}
