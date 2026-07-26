//! Application state and lightweight Phase 1 persistence.
//!
//! Recordings and settings are stored as JSON under the user's roaming app-data
//! directory. Media files themselves remain in the configured Recordings folder.

use crate::recording::{types::*, RecordingEngine};
use parking_lot::RwLock;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug)]
pub struct LibraryStore {
    pub recordings: RwLock<Vec<RecordingOutput>>,
    path: PathBuf,
}

impl LibraryStore {
    fn new(path: PathBuf) -> Self {
        Self {
            recordings: RwLock::new(load_json(&path).unwrap_or_default()),
            path,
        }
    }

    pub fn persist(&self) -> Result<(), String> {
        write_json(&self.path, &*self.recordings.read())
    }
}

#[derive(Debug)]
pub struct SettingsStore {
    pub settings: RwLock<RecorderSettings>,
    path: PathBuf,
}

impl SettingsStore {
    fn new(path: PathBuf) -> Self {
        Self {
            settings: RwLock::new(load_json(&path).unwrap_or_default()),
            path,
        }
    }

    pub fn persist(&self) -> Result<(), String> {
        write_json(&self.path, &*self.settings.read())
    }
}

pub struct AppState {
    pub engine: Arc<RecordingEngine>,
    pub library: LibraryStore,
    pub settings: SettingsStore,
}

impl AppState {
    pub fn new() -> Self {
        let data_dir = app_data_dir();
        let _ = fs::create_dir_all(&data_dir);
        Self {
            engine: RecordingEngine::new(),
            library: LibraryStore::new(data_dir.join("recordings.json")),
            settings: SettingsStore::new(data_dir.join("settings.json")),
        }
    }
}

fn app_data_dir() -> PathBuf {
    if let Some(value) = std::env::var_os("APPDATA") {
        return PathBuf::from(value).join("Recorder");
    }
    if let Some(home) = std::env::var_os("USERPROFILE") {
        return PathBuf::from(home).join("AppData").join("Roaming").join("Recorder");
    }
    PathBuf::from(".").join("RecorderData")
}

fn load_json<T: DeserializeOwned>(path: &Path) -> Option<T> {
    let bytes = fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn write_json<T: Serialize + ?Sized>(path: &Path, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Unable to create app-data directory: {e}"))?;
    }
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| format!("Unable to serialize local data: {e}"))?;
    let temp = path.with_extension("tmp");
    fs::write(&temp, bytes).map_err(|e| format!("Unable to write local data: {e}"))?;
    if path.exists() {
        fs::remove_file(path).map_err(|e| format!("Unable to replace local data: {e}"))?;
    }
    fs::rename(&temp, path).map_err(|e| format!("Unable to finalize local data: {e}"))
}
