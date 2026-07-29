//! Application state and lightweight Phase 1 persistence.
//!
//! Recordings and settings are stored as JSON under the user's roaming app-data
//! directory. Media files themselves remain in the configured Recordings folder.

use crate::{
    encoding,
    recording::{types::*, RecordingEngine},
    security,
};
use parking_lot::{Mutex, RwLock};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug)]
pub struct LibraryStore {
    pub recordings: Arc<RwLock<Vec<RecordingOutput>>>,
    path: PathBuf,
    persist_lock: Arc<Mutex<()>>,
}

impl LibraryStore {
    fn new(path: PathBuf) -> Self {
        let mut recordings: Vec<RecordingOutput> = load_json(&path).unwrap_or_default();
        // Persisted metadata is untrusted. Keep only non-empty UUID-named MP4 files
        // that canonicalise to a direct child of the approved Recordings directory.
        recordings.retain(|recording| {
            match security::validate_existing_recording_path(
                &recording.id,
                &recording.file_path,
            ) {
                Ok(canonical) => {
                    recording.file_path = canonical.to_string_lossy().into_owned();
                    true
                }
                Err(error) => {
                    eprintln!(
                        "[Recorder][Security] library_entry_rejected=true id={} error={error}",
                        recording.id
                    );
                    false
                }
            }
        });
        let store = Self {
            recordings: Arc::new(RwLock::new(recordings)),
            path,
            persist_lock: Arc::new(Mutex::new(())),
        };
        let _ = store.persist();
        store.refresh_thumbnails_async();
        store
    }

    pub fn persist(&self) -> Result<(), String> {
        let _persist_guard = self.persist_lock.lock();
        write_json(&self.path, &*self.recordings.read())
    }

    /// Generate one newly completed recording's thumbnail without extending the
    /// Stop transaction. The shared library entry and recordings.json are updated
    /// after Windows returns the thumbnail.
    pub fn schedule_thumbnail_async(&self, id: String, file_path: String) {
        let worker_path = match security::validate_existing_recording_path(&id, &file_path) {
            Ok(path) => path,
            Err(error) => {
                eprintln!(
                    "[Recorder][Security] thumbnail_path_rejected=true id={id} error={error}"
                );
                return;
            }
        };
        let recordings = Arc::clone(&self.recordings);
        let metadata_path = self.path.clone();
        let persist_lock = Arc::clone(&self.persist_lock);
        let worker_id = id.clone();
        let spawn_result = std::thread::Builder::new()
            .name("recorder-thumbnail".to_string())
            .spawn(move || {
                let started = std::time::Instant::now();
                let Some(thumbnail) = encoding::generate_thumbnail_data_url(&worker_path) else {
                    eprintln!(
                        "[Recorder][Health] thumbnail_ok=false id={worker_id}"
                    );
                    return;
                };

                let changed = {
                    let mut guard = recordings.write();
                    let Some(recording) = guard
                        .iter_mut()
                        .find(|recording| recording.id == worker_id)
                    else {
                        return;
                    };
                    if recording.thumbnail_data_url.is_some() {
                        false
                    } else {
                        recording.thumbnail_data_url = Some(thumbnail);
                        true
                    }
                };

                if changed {
                    let _persist_guard = persist_lock.lock();
                    if let Err(error) = write_json(&metadata_path, &*recordings.read()) {
                        eprintln!(
                            "[Recorder][Health] thumbnail_persist_ok=false id={worker_id} error={error}"
                        );
                    } else {
                        eprintln!(
                            "[Recorder][Health] thumbnail_ok=true id={worker_id} thumbnail_ms={}",
                            started.elapsed().as_millis()
                        );
                    }
                }
            });

        if let Err(error) = spawn_result {
            eprintln!(
                "[Recorder][Health] thumbnail_ok=false id={id} error=unable_to_spawn_thumbnail_worker:{error}"
            );
        }
    }

    /// Backfill thumbnails for recordings created before native thumbnail support.
    /// One worker processes the pending list sequentially to avoid flooding the
    /// Windows thumbnail cache with parallel requests during application startup.
    pub fn refresh_thumbnails_async(&self) {
        let pending: Vec<(String, PathBuf)> = self
            .recordings
            .read()
            .iter()
            .filter(|recording| recording.thumbnail_data_url.is_none())
            .filter_map(|recording| {
                security::validate_existing_recording_path(
                    &recording.id,
                    &recording.file_path,
                )
                .ok()
                .map(|path| (recording.id.clone(), path))
            })
            .collect();
        if pending.is_empty() {
            return;
        }

        let recordings = Arc::clone(&self.recordings);
        let metadata_path = self.path.clone();
        let persist_lock = Arc::clone(&self.persist_lock);
        let spawn_result = std::thread::Builder::new()
            .name("recorder-thumbnail-backfill".to_string())
            .spawn(move || {
                let mut generated = 0usize;
                for (id, file_path) in pending {
                    let Some(thumbnail) = encoding::generate_thumbnail_data_url(&file_path) else {
                        continue;
                    };
                    let mut guard = recordings.write();
                    if let Some(recording) = guard.iter_mut().find(|recording| recording.id == id) {
                        if recording.thumbnail_data_url.is_none() {
                            recording.thumbnail_data_url = Some(thumbnail);
                            generated += 1;
                        }
                    }
                }

                if generated > 0 {
                    let _persist_guard = persist_lock.lock();
                    if let Err(error) = write_json(&metadata_path, &*recordings.read()) {
                        eprintln!(
                            "[Recorder][Health] thumbnail_backfill_ok=false generated={generated} error={error}"
                        );
                    } else {
                        eprintln!(
                            "[Recorder][Health] thumbnail_backfill_ok=true generated={generated}"
                        );
                    }
                }
            });

        if let Err(error) = spawn_result {
            eprintln!(
                "[Recorder][Health] thumbnail_backfill_ok=false error=unable_to_spawn_thumbnail_backfill:{error}"
            );
        }
    }
}

#[derive(Debug)]
pub struct SettingsStore {
    pub settings: RwLock<RecorderSettings>,
    path: PathBuf,
}

impl SettingsStore {
    fn new(path: PathBuf) -> Self {
        let loaded: RecorderSettings = load_json(&path).unwrap_or_default();
        let settings = security::validate_settings(loaded).unwrap_or_else(|error| {
            eprintln!(
                "[Recorder][Security] settings_reset=true error={error}"
            );
            let mut defaults = RecorderSettings::default();
            if let Ok(root) = security::recordings_root_string() {
                defaults.output_directory = root;
            }
            defaults
        });
        Self {
            settings: RwLock::new(settings),
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
