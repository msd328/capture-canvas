//! Recording engine surface.
//!
//! The engine owns the lifecycle of a single active recording. It composes
//! screen capture, microphone capture, optional system-audio capture, and
//! optional camera capture into a synchronized MP4 file.
//!
//! Current status: STUB. The methods below record only lifecycle timing so
//! the UI can be exercised end-to-end. The real pipeline must live behind
//! the same public surface — do not add new public methods that the
//! frontend depends on; instead extend `commands/` and route through here.

pub mod types;

use anyhow::{anyhow, Result};
use chrono::Utc;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

pub use types::*;

#[derive(Debug)]
struct ActiveRecording {
    id: String,
    config: RecordingConfig,
    started_at: Instant,
    paused_total_ms: u64,
    paused_at: Option<Instant>,
}

#[derive(Debug, Default)]
pub struct RecordingEngine {
    active: Mutex<Option<ActiveRecording>>,
}

impl RecordingEngine {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn start(&self, config: RecordingConfig) -> Result<String> {
        let mut guard = self.active.lock();
        if guard.is_some() {
            return Err(anyhow!("A recording is already in progress"));
        }
        let id = Uuid::new_v4().to_string();

        // TODO(native):
        //   1. Request/check screen-recording, microphone, camera permissions.
        //   2. Spin up capture pipelines in `crate::capture`, `crate::audio`,
        //      `crate::camera` based on `config`.
        //   3. Bring up the encoder in `crate::encoding` and open the output
        //      MP4 muxer at `config.output_path` (default to
        //      settings.output_directory/<id>.mp4).
        //   4. Excludes the floating-controls window from capture on
        //      supported OSes (WDA_EXCLUDEFROMCAPTURE / NSWindow.sharingType).

        *guard = Some(ActiveRecording {
            id: id.clone(),
            config,
            started_at: Instant::now(),
            paused_total_ms: 0,
            paused_at: None,
        });
        Ok(id)
    }

    pub fn pause(&self) -> Result<()> {
        let mut guard = self.active.lock();
        let rec = guard.as_mut().ok_or_else(|| anyhow!("No active recording"))?;
        if rec.paused_at.is_none() {
            rec.paused_at = Some(Instant::now());
        }
        // TODO(native): signal each capture pipeline to pause; hold encoder frames.
        Ok(())
    }

    pub fn resume(&self) -> Result<()> {
        let mut guard = self.active.lock();
        let rec = guard.as_mut().ok_or_else(|| anyhow!("No active recording"))?;
        if let Some(at) = rec.paused_at.take() {
            rec.paused_total_ms = rec.paused_total_ms.saturating_add(at.elapsed().as_millis() as u64);
        }
        // TODO(native): resume capture pipelines and re-sync clocks.
        Ok(())
    }

    pub fn stop(&self) -> Result<RecordingOutput> {
        let mut guard = self.active.lock();
        let rec = guard.take().ok_or_else(|| anyhow!("No active recording"))?;
        let now = Instant::now();
        let paused = rec.paused_total_ms
            + rec
                .paused_at
                .map(|at| at.elapsed().as_millis() as u64)
                .unwrap_or(0);
        let duration_ms = (now
            .duration_since(rec.started_at)
            .as_millis() as u64)
            .saturating_sub(paused);

        // TODO(native):
        //   1. Flush encoder, finalize MP4 headers (moov atom).
        //   2. Compute actual width/height/file_size from the produced file.
        //   3. Extract a thumbnail (first keyframe) to data URL for the library.

        let file_path = rec
            .config
            .output_path
            .clone()
            .unwrap_or_else(|| format!("~/Recordings/{}.mp4", rec.id));

        Ok(RecordingOutput {
            id: rec.id,
            title: rec
                .config
                .title
                .unwrap_or_else(|| format!("Recording {}", Utc::now().format("%Y-%m-%d %H:%M"))),
            file_path,
            created_at: Utc::now().to_rfc3339(),
            duration_ms: duration_ms.max(1000),
            width: 1920,
            height: 1080,
            file_size_bytes: 0, // TODO(native): stat the file.
            thumbnail_data_url: None,
        })
    }
}
