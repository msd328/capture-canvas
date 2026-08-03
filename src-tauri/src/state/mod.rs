//! Application state and lightweight Phase 1 persistence.
//!
//! Recordings and settings are stored as JSON under the user's roaming app-data
//! directory. Media files themselves remain in the configured Recordings folder.

use crate::{
    auth::SecureAuthStore,
    encoding,
    recording::{types::*, RecordingEngine},
    security,
};
use parking_lot::{Condvar, Mutex, RwLock};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

const LIBRARY_UPDATE_WAIT: Duration = Duration::from_secs(25);

#[derive(Debug)]
struct LibraryRevision {
    value: Mutex<u64>,
    changed: Condvar,
}

impl LibraryRevision {
    fn new() -> Self {
        Self {
            value: Mutex::new(1),
            changed: Condvar::new(),
        }
    }

    fn current(&self) -> u64 {
        *self.value.lock()
    }

    fn bump(&self) -> u64 {
        let mut value = self.value.lock();
        *value = (*value).saturating_add(1);
        let revision = *value;
        self.changed.notify_all();
        revision
    }

    fn wait_for_change(&self, after_revision: u64) {
        let mut value = self.value.lock();
        if *value <= after_revision {
            let _ = self.changed.wait_for(&mut value, LIBRARY_UPDATE_WAIT);
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySnapshot {
    pub revision: u64,
    pub recordings: Vec<RecordingOutput>,
}

#[derive(Clone, Debug)]
pub struct LibraryStore {
    pub recordings: Arc<RwLock<Vec<RecordingOutput>>>,
    path: PathBuf,
    persist_lock: Arc<Mutex<()>>,
    revision: Arc<LibraryRevision>,
}

impl LibraryStore {
    fn new(path: PathBuf) -> Self {
        let mut recordings: Vec<RecordingOutput> = load_json(&path).unwrap_or_default();
        // Persisted metadata is untrusted. Keep only non-empty UUID-named MP4 files
        // that canonicalise to a direct child of the approved Recordings directory.
        recordings.retain(|recording| {
            match security::validate_existing_recording_path(&recording.id, &recording.file_path) {
                Ok(_) => true,
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
            revision: Arc::new(LibraryRevision::new()),
        };
        let _ = store.persist();
        store.refresh_thumbnails_async();
        store
    }

    pub fn persist(&self) -> Result<(), String> {
        let _persist_guard = self.persist_lock.lock();
        write_json(&self.path, &*self.recordings.read())
    }

    pub fn persist_and_notify(&self, reason: &'static str) -> Result<u64, String> {
        self.persist()?;
        Ok(self.notify_changed(reason))
    }

    pub fn snapshot(&self) -> LibrarySnapshot {
        // Read the revision first. If a writer changes the recordings after this
        // read, its later revision bump makes the next wait return immediately.
        let revision = self.revision.current();
        let recordings = self.recordings.read().clone();
        LibrarySnapshot {
            revision,
            recordings,
        }
    }

    pub fn wait_for_update(&self, after_revision: u64) -> LibrarySnapshot {
        self.revision.wait_for_change(after_revision);
        self.snapshot()
    }

    fn notify_changed(&self, reason: &'static str) -> u64 {
        let revision = self.revision.bump();
        eprintln!("[Recorder][LibraryHealth] stage=notify reason={reason} revision={revision}");
        revision
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
        let store = self.clone();
        let worker_id = id.clone();
        let spawn_result = std::thread::Builder::new()
            .name("recorder-thumbnail".to_string())
            .spawn(move || {
                let started = std::time::Instant::now();
                let thumbnail = match encoding::generate_thumbnail_data_url(&worker_path) {
                    Ok(thumbnail) => thumbnail,
                    Err(error) => {
                        eprintln!(
                            "[Recorder][ThumbnailHealth] ok=false id={worker_id} stage={} attempt={} code={} thumbnail_ms={}",
                            error.stage,
                            error.attempt,
                            error.code.as_deref().unwrap_or("none"),
                            started.elapsed().as_millis()
                        );
                        return;
                    }
                };

                let changed = {
                    let mut guard = store.recordings.write();
                    let Some(recording) = guard
                        .iter_mut()
                        .find(|recording| recording.id == worker_id)
                    else {
                        eprintln!(
                            "[Recorder][ThumbnailHealth] ok=false id={worker_id} stage=library_entry_missing attempt=0 code=none thumbnail_ms={}",
                            started.elapsed().as_millis()
                        );
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
                    if let Err(error) = store.persist() {
                        eprintln!(
                            "[Recorder][ThumbnailHealth] ok=false id={worker_id} stage=persist attempt=0 code={error} thumbnail_ms={}",
                            started.elapsed().as_millis()
                        );
                    } else {
                        store.notify_changed("thumbnail_ready");
                        eprintln!(
                            "[Recorder][ThumbnailHealth] ok=true id={worker_id} stage=complete attempt=0 code=none thumbnail_ms={}",
                            started.elapsed().as_millis()
                        );
                    }
                } else {
                    eprintln!(
                        "[Recorder][ThumbnailHealth] ok=true id={worker_id} stage=already_present attempt=0 code=none thumbnail_ms={}",
                        started.elapsed().as_millis()
                    );
                }
            });

        if let Err(error) = spawn_result {
            eprintln!(
                "[Recorder][ThumbnailHealth] ok=false id={id} stage=spawn_worker attempt=0 code={error} thumbnail_ms=0"
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
                security::validate_existing_recording_path(&recording.id, &recording.file_path)
                    .ok()
                    .map(|path| (recording.id.clone(), path))
            })
            .collect();
        if pending.is_empty() {
            return;
        }

        let store = self.clone();
        let spawn_result = std::thread::Builder::new()
            .name("recorder-thumbnail-backfill".to_string())
            .spawn(move || {
                let mut generated = 0usize;
                let mut failed = 0usize;
                for (id, file_path) in pending {
                    let thumbnail = match encoding::generate_thumbnail_data_url(&file_path) {
                        Ok(thumbnail) => thumbnail,
                        Err(error) => {
                            failed = failed.saturating_add(1);
                            eprintln!(
                                "[Recorder][ThumbnailHealth] ok=false id={id} stage={} attempt={} code={} backfill=true",
                                error.stage,
                                error.attempt,
                                error.code.as_deref().unwrap_or("none")
                            );
                            continue;
                        }
                    };
                    let mut guard = store.recordings.write();
                    if let Some(recording) = guard.iter_mut().find(|recording| recording.id == id) {
                        if recording.thumbnail_data_url.is_none() {
                            recording.thumbnail_data_url = Some(thumbnail);
                            generated += 1;
                        }
                    }
                }

                if generated > 0 {
                    if let Err(error) = store.persist() {
                        eprintln!(
                            "[Recorder][ThumbnailHealth] ok=false stage=backfill_persist generated={generated} failed={failed} code={error}"
                        );
                    } else {
                        store.notify_changed("thumbnail_backfill");
                        eprintln!(
                            "[Recorder][ThumbnailHealth] ok=true stage=backfill_complete generated={generated} failed={failed} code=none"
                        );
                    }
                } else {
                    eprintln!(
                        "[Recorder][ThumbnailHealth] ok={} stage=backfill_complete generated=0 failed={failed} code=none",
                        failed == 0
                    );
                }
            });

        if let Err(error) = spawn_result {
            eprintln!(
                "[Recorder][ThumbnailHealth] ok=false stage=spawn_backfill_worker generated=0 failed=0 code={error}"
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
            eprintln!("[Recorder][Security] settings_reset=true error={error}");
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
    pub auth: SecureAuthStore,
}

impl AppState {
    pub fn new() -> Self {
        let data_dir = app_data_dir();
        let _ = fs::create_dir_all(&data_dir);
        Self {
            engine: RecordingEngine::new(),
            library: LibraryStore::new(data_dir.join("recordings.json")),
            settings: SettingsStore::new(data_dir.join("settings.json")),
            auth: SecureAuthStore::new(),
        }
    }
}

fn app_data_dir() -> PathBuf {
    if let Some(value) = std::env::var_os("APPDATA") {
        return PathBuf::from(value).join("Recorder");
    }
    if let Some(home) = std::env::var_os("USERPROFILE") {
        return PathBuf::from(home)
            .join("AppData")
            .join("Roaming")
            .join("Recorder");
    }
    PathBuf::from(".").join("RecorderData")
}

fn load_json<T: DeserializeOwned>(path: &Path) -> Option<T> {
    let bytes = fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn write_json<T: Serialize + ?Sized>(path: &Path, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Unable to create app-data directory: {error}"))?;
    }
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("Unable to serialize local data: {error}"))?;
    let temp = path.with_extension("tmp");
    fs::write(&temp, bytes).map_err(|error| format!("Unable to write local data: {error}"))?;
    if path.exists() {
        fs::remove_file(path).map_err(|error| format!("Unable to replace local data: {error}"))?;
    }
    fs::rename(&temp, path).map_err(|error| format!("Unable to finalize local data: {error}"))
}
