//! Per-segment microphone/system mixer health counters.
//!
//! The recorder's two current Windows mixer backends intentionally keep their
//! existing queue and scheduling code. This module provides a `VecDeque`-shaped
//! queue that passively counts silence substitutions and bounded-queue evictions.

use parking_lot::Mutex;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MixerSource {
    Microphone,
    System,
}

struct MixerHealthState {
    backend: &'static str,
    started_at: Instant,
    microphone_underruns: u64,
    system_underruns: u64,
    microphone_dropped_frames: u64,
    system_dropped_frames: u64,
    microphone_peak_queue_frames: usize,
    system_peak_queue_frames: usize,
}

impl MixerHealthState {
    fn record_underrun(&mut self, source: MixerSource) {
        match source {
            MixerSource::Microphone => {
                self.microphone_underruns = self.microphone_underruns.saturating_add(1);
            }
            MixerSource::System => {
                self.system_underruns = self.system_underruns.saturating_add(1);
            }
        }
    }

    fn record_queue_depth(&mut self, source: MixerSource, frames: usize) {
        match source {
            MixerSource::Microphone => {
                self.microphone_peak_queue_frames = self.microphone_peak_queue_frames.max(frames);
            }
            MixerSource::System => {
                self.system_peak_queue_frames = self.system_peak_queue_frames.max(frames);
            }
        }
    }

    fn record_dropped_frames(&mut self, source: MixerSource, frames: usize) {
        let frames = u64::try_from(frames).unwrap_or(u64::MAX);
        match source {
            MixerSource::Microphone => {
                self.microphone_dropped_frames =
                    self.microphone_dropped_frames.saturating_add(frames);
            }
            MixerSource::System => {
                self.system_dropped_frames =
                    self.system_dropped_frames.saturating_add(frames);
            }
        }
    }
}

struct MixerHealthSession {
    state: Mutex<MixerHealthState>,
}

impl Drop for MixerHealthSession {
    fn drop(&mut self) {
        let state = self.state.lock();
        eprintln!(
            "[Recorder][AudioMixerHealth] backend={} wall_ms={} microphone_underruns={} system_underruns={} microphone_dropped_frames={} system_dropped_frames={} microphone_peak_queue_frames={} system_peak_queue_frames={}",
            state.backend,
            state.started_at.elapsed().as_millis(),
            state.microphone_underruns,
            state.system_underruns,
            state.microphone_dropped_frames,
            state.system_dropped_frames,
            state.microphone_peak_queue_frames,
            state.system_peak_queue_frames,
        );

        if state.microphone_underruns > 0 || state.system_underruns > 0 {
            eprintln!(
                "[Recorder][AudioMixerHealth] warning=mixer_underruns microphone={} system={}",
                state.microphone_underruns, state.system_underruns,
            );
        }
        if state.microphone_dropped_frames > 0 || state.system_dropped_frames > 0 {
            eprintln!(
                "[Recorder][AudioMixerHealth] warning=queue_overflow_drops microphone_frames={} system_frames={}",
                state.microphone_dropped_frames, state.system_dropped_frames,
            );
        }
    }
}

struct PendingMixerSession {
    health: Arc<MixerHealthSession>,
    next_source: u8,
    chunk_frames: usize,
    max_frames: usize,
}

thread_local! {
    static PENDING_MIXER_SESSION: RefCell<Option<PendingMixerSession>> = const { RefCell::new(None) };
}

/// Prepare the next two queue constructions as microphone and system queues for
/// one mixer segment. The current recorder creates those queues consecutively on
/// the same thread, before any fallible stream setup.
pub fn start_audio_mixer_segment(
    backend: &'static str,
    chunk_frames: usize,
    max_frames: usize,
) {
    let health = Arc::new(MixerHealthSession {
        state: Mutex::new(MixerHealthState {
            backend,
            started_at: Instant::now(),
            microphone_underruns: 0,
            system_underruns: 0,
            microphone_dropped_frames: 0,
            system_dropped_frames: 0,
            microphone_peak_queue_frames: 0,
            system_peak_queue_frames: 0,
        }),
    });

    PENDING_MIXER_SESSION.with(|slot| {
        *slot.borrow_mut() = Some(PendingMixerSession {
            health,
            next_source: 0,
            chunk_frames: chunk_frames.max(1),
            max_frames: max_frames.max(chunk_frames.max(1)),
        });
    });
}

/// Drop-in replacement for the recorder's local `VecDeque<[f32; 2]>` queues.
///
/// `len()` counts an underrun only after the queue has supplied its first full
/// mixer chunk. This excludes the intentional startup prebuffer. `extend()`
/// retains the existing newest-data policy while counting evicted old frames.
pub struct InstrumentedVecDeque<T> {
    frames: VecDeque<T>,
    source: Option<MixerSource>,
    chunk_frames: usize,
    max_frames: usize,
    health: Option<Arc<MixerHealthSession>>,
    mixing_started: Cell<bool>,
    suppress_next_len_check: Cell<bool>,
}

impl<T> InstrumentedVecDeque<T> {
    pub fn new() -> Self {
        let configured = PENDING_MIXER_SESSION.with(|slot| {
            let mut pending = slot.borrow_mut();
            let Some(session) = pending.as_mut() else {
                return None;
            };

            let source = match session.next_source {
                0 => MixerSource::Microphone,
                1 => MixerSource::System,
                _ => return None,
            };
            session.next_source = session.next_source.saturating_add(1);
            let result = (
                source,
                session.chunk_frames,
                session.max_frames,
                session.health.clone(),
            );
            if session.next_source >= 2 {
                pending.take();
            }
            Some(result)
        });

        let (source, chunk_frames, max_frames, health) = match configured {
            Some((source, chunk_frames, max_frames, health)) => {
                (Some(source), chunk_frames, max_frames, Some(health))
            }
            None => (None, usize::MAX, usize::MAX, None),
        };

        Self {
            frames: VecDeque::new(),
            source,
            chunk_frames,
            max_frames,
            health,
            mixing_started: Cell::new(false),
            suppress_next_len_check: Cell::new(false),
        }
    }

    pub fn len(&self) -> usize {
        let len = self.frames.len();
        if self.suppress_next_len_check.replace(false) {
            return len;
        }

        if self.mixing_started.get() && len < self.chunk_frames {
            if let (Some(source), Some(health)) = (self.source, self.health.as_ref()) {
                health.state.lock().record_underrun(source);
            }
        }
        len
    }

    pub fn pop_front(&mut self) -> Option<T> {
        let value = self.frames.pop_front();
        if value.is_some() {
            self.mixing_started.set(true);
        }
        value
    }
}

impl<T> Default for InstrumentedVecDeque<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Extend<T> for InstrumentedVecDeque<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        self.frames.extend(iter);

        if let (Some(source), Some(health)) = (self.source, self.health.as_ref()) {
            health
                .state
                .lock()
                .record_queue_depth(source, self.frames.len());
        }

        let dropped = self.frames.len().saturating_sub(self.max_frames);
        for _ in 0..dropped {
            self.frames.pop_front();
        }
        if dropped > 0 {
            if let (Some(source), Some(health)) = (self.source, self.health.as_ref()) {
                health
                    .state
                    .lock()
                    .record_dropped_frames(source, dropped);
            }
        }

        // The existing backend performs an immediate `len() > MAX` check after
        // extending. Suppress that administrative check so it is not mistaken for
        // the mixer's 10 ms drain request.
        self.suppress_next_len_check.set(true);
    }
}
