//! Application state: the active recording engine, in-memory recordings
//! index, and persisted user settings.
//!
//! Persistence in Phase 1 is a JSON file under Tauri's `AppData` dir.
//! We deliberately avoid SQLite for now — the working set is tiny and
//! desktop-local.

use crate::recording::{types::*, RecordingEngine};
use parking_lot::RwLock;
use std::sync::Arc;

#[derive(Debug, Default)]
pub struct LibraryStore {
    pub recordings: RwLock<Vec<RecordingOutput>>,
}

#[derive(Debug, Default)]
pub struct SettingsStore {
    pub settings: RwLock<RecorderSettings>,
}

pub struct AppState {
    pub engine: Arc<RecordingEngine>,
    pub library: LibraryStore,
    pub settings: SettingsStore,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            engine: RecordingEngine::new(),
            library: LibraryStore::default(),
            settings: SettingsStore {
                settings: RwLock::new(RecorderSettings::default()),
            },
        }
    }
}

// TODO(native): load/persist library and settings JSON from Tauri
// `app_data_dir()` on startup and on every mutation.
