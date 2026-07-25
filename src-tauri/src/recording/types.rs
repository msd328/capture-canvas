// Shared serde types mirroring `src/types/recorder.ts` on the frontend.
// Keep field names in camelCase via `rename_all` so JSON matches the TS side.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayInfo {
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub is_primary: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail_data_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowInfo {
    pub id: String,
    pub name: String,
    pub app_name: String,
    pub width: u32,
    pub height: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail_data_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MicrophoneInfo {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraInfo {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CaptureKind {
    #[serde(rename = "display")]
    Display,
    #[serde(rename = "window")]
    Window,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureTarget {
    pub kind: CaptureKind,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingConfig {
    pub target: CaptureTarget,
    pub microphone_id: Option<String>,
    pub camera_id: Option<String>,
    pub system_audio: bool,
    pub fps: u32,
    #[serde(default)]
    pub output_path: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingOutput {
    pub id: String,
    pub title: String,
    pub file_path: String,
    pub created_at: String,
    pub duration_ms: u64,
    pub width: u32,
    pub height: u32,
    pub file_size_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail_data_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecorderSettings {
    pub default_microphone_id: Option<String>,
    pub default_camera_id: Option<String>,
    pub fps: u32,
    pub output_directory: String,
    pub launch_at_startup: bool,
    pub show_camera_bubble: bool,
}

impl Default for RecorderSettings {
    fn default() -> Self {
        Self {
            default_microphone_id: None,
            default_camera_id: None,
            fps: 30,
            output_directory: "~/Recordings".into(),
            launch_at_startup: false,
            show_camera_bubble: true,
        }
    }
}
