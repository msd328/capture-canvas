//! Process-local diagnostics shared by the recorder camera source and encoder facade.
//!
//! The desktop recorder currently permits one active recording at a time, so one
//! camera segment can be tracked safely in a single process-local state object.

use parking_lot::Mutex;
use std::sync::OnceLock;
use std::time::Instant;

#[derive(Default)]
struct CameraHealth {
    active: bool,
    backend: &'static str,
    started_at: Option<Instant>,
    source_stopped: bool,
    frames_received: u64,
    acquire_misses: u64,
    source_errors: u64,
    overlay_frames: u64,
    first_received_at: Option<Instant>,
    last_received_at: Option<Instant>,
    largest_receive_gap_ms: u128,
}

fn camera_health() -> &'static Mutex<CameraHealth> {
    static CAMERA_HEALTH: OnceLock<Mutex<CameraHealth>> = OnceLock::new();
    CAMERA_HEALTH.get_or_init(|| Mutex::new(CameraHealth::default()))
}

/// Start a fresh per-segment camera health window after the source is confirmed active.
pub fn start_camera_segment(backend: &'static str) {
    let mut health = camera_health().lock();
    *health = CameraHealth {
        active: true,
        backend,
        started_at: Some(Instant::now()),
        ..CameraHealth::default()
    };
}

/// Record one camera frame successfully copied into the recorder's shared BGRA slot.
pub fn record_camera_received() {
    let now = Instant::now();
    let mut health = camera_health().lock();
    if !health.active {
        return;
    }

    health.frames_received = health.frames_received.saturating_add(1);
    if health.first_received_at.is_none() {
        health.first_received_at = Some(now);
    }
    if let Some(previous) = health.last_received_at {
        health.largest_receive_gap_ms = health
            .largest_receive_gap_ms
            .max(now.duration_since(previous).as_millis());
    }
    health.last_received_at = Some(now);
}

/// Record a MediaFrameReader callback that did not yield a usable latest frame.
pub fn record_camera_acquire_miss() {
    let mut health = camera_health().lock();
    if health.active {
        health.acquire_misses = health.acquire_misses.saturating_add(1);
    }
}

/// Record a camera callback/read failure. The owning capture still propagates the error.
pub fn record_camera_source_error() {
    let mut health = camera_health().lock();
    if health.active {
        health.source_errors = health.source_errors.saturating_add(1);
    }
}

/// Record a successfully encoded video frame while a usable camera frame is available.
///
/// Both current recorder backends reuse the latest camera frame for each encoded screen
/// frame, so this is the number of encoded frames expected to contain the overlay.
pub fn record_camera_overlay_submission() {
    let mut health = camera_health().lock();
    if health.active && health.frames_received > 0 {
        health.overlay_frames = health.overlay_frames.saturating_add(1);
    }
}

pub fn mark_camera_source_stopped() {
    let mut health = camera_health().lock();
    if health.active {
        health.source_stopped = true;
    }
}

/// Emit and reset the current segment's camera diagnostics when its encoder finalizes.
pub fn finish_camera_segment(finalize_ok: bool) {
    let mut health = camera_health().lock();
    if !health.active {
        return;
    }

    let wall_ms = health
        .started_at
        .map(|started| started.elapsed().as_millis())
        .unwrap_or(0);
    let receive_span_seconds = match (health.first_received_at, health.last_received_at) {
        (Some(first), Some(last)) => last.duration_since(first).as_secs_f64(),
        _ => 0.0,
    };
    let receive_fps = if health.frames_received > 1 && receive_span_seconds > 0.0 {
        (health.frames_received - 1) as f64 / receive_span_seconds
    } else {
        0.0
    };

    eprintln!(
        "[Recorder][CameraHealth] backend={} finalize_ok={} source_stopped={} wall_ms={} frames_received={} overlay_frames={} acquire_misses={} source_errors={} receive_fps={:.2} max_receive_gap_ms={}",
        health.backend,
        finalize_ok,
        health.source_stopped,
        wall_ms,
        health.frames_received,
        health.overlay_frames,
        health.acquire_misses,
        health.source_errors,
        receive_fps,
        health.largest_receive_gap_ms,
    );

    if health.frames_received == 0 {
        eprintln!(
            "[Recorder][CameraHealth] warning=no_camera_frames_received backend={}",
            health.backend
        );
    }
    if health.source_errors > 0 {
        eprintln!(
            "[Recorder][CameraHealth] warning=camera_source_errors backend={} errors={}",
            health.backend, health.source_errors
        );
    }
    if health.largest_receive_gap_ms > 1_000 {
        eprintln!(
            "[Recorder][CameraHealth] warning=large_camera_gap backend={} gap_ms={}",
            health.backend, health.largest_receive_gap_ms
        );
    }

    *health = CameraHealth::default();
}
