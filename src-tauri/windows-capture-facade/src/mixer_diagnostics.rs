//! Per-segment microphone/system mixer health counters.
//!
//! The recorder's two current Windows mixer backends intentionally keep their
//! existing queue and scheduling code. This module provides `VecDeque`-shaped
//! queues that passively count silence substitutions and bounded-queue evictions.

use parking_lot::Mutex;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Instant;

const MIX_CHUNK_FRAMES: usize = 480;
const MAX_MIX_QUEUE_FRAMES: usize = 96_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MixerSource {
    Microphone,
    System,
}

#[doc(hidden)]
pub trait MixerBackend {
    const ID: u8;
    const LABEL: &'static str;
}

pub enum GpuMixerBackend {}
pub enum BufferMixerBackend {}

impl MixerBackend for GpuMixerBackend {
    const ID: u8 = 1;
    const LABEL: &'static str = "native-wgc-d3d11";
}

impl MixerBackend for BufferMixerBackend {
    const ID: u8 = 2;
    const LABEL: &'static str = "native-wgc-buffer";
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
    backend_id: u8,
    health: Arc<MixerHealthSession>,
}

thread_local! {
    static PENDING_MIXER_SESSION: RefCell<Option<PendingMixerSession>> = const { RefCell::new(None) };
}

fn attach_queue<B: MixerBackend>() -> (MixerSource, Arc<MixerHealthSession>) {
    PENDING_MIXER_SESSION.with(|slot| {
        let mut pending = slot.borrow_mut();
        if let Some(session) = pending.as_ref() {
            if session.backend_id == B::ID {
                let health = session.health.clone();
                pending.take();
                return (MixerSource::System, health);
            }
        }

        let health = Arc::new(MixerHealthSession {
            state: Mutex::new(MixerHealthState {
                backend: B::LABEL,
                started_at: Instant::now(),
                microphone_underruns: 0,
                system_underruns: 0,
                microphone_dropped_frames: 0,
                system_dropped_frames: 0,
                microphone_peak_queue_frames: 0,
                system_peak_queue_frames: 0,
            }),
        });
        *pending = Some(PendingMixerSession {
            backend_id: B::ID,
            health: health.clone(),
        });
        (MixerSource::Microphone, health)
    })
}

/// Drop-in replacement for the recorder's local `VecDeque<[f32; 2]>` queues.
///
/// `len()` counts an underrun only after the queue has supplied its first full
/// mixer chunk. This excludes the intentional startup prebuffer. `extend()`
/// retains the existing newest-data policy while counting evicted old frames.
pub struct InstrumentedVecDeque<T, B: MixerBackend> {
    frames: VecDeque<T>,
    source: MixerSource,
    health: Arc<MixerHealthSession>,
    mixing_started: Cell<bool>,
    suppress_next_len_check: Cell<bool>,
    backend: PhantomData<B>,
}

impl<T, B: MixerBackend> InstrumentedVecDeque<T, B> {
    pub fn new() -> Self {
        let (source, health) = attach_queue::<B>();
        Self {
            frames: VecDeque::new(),
            source,
            health,
            mixing_started: Cell::new(false),
            suppress_next_len_check: Cell::new(false),
            backend: PhantomData,
        }
    }

    pub fn len(&self) -> usize {
        let len = self.frames.len();
        if self.suppress_next_len_check.replace(false) {
            return len;
        }

        if self.mixing_started.get() && len < MIX_CHUNK_FRAMES {
            self.health.state.lock().record_underrun(self.source);
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

impl<T, B: MixerBackend> Default for InstrumentedVecDeque<T, B> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, B: MixerBackend> Extend<T> for InstrumentedVecDeque<T, B> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        self.frames.extend(iter);
        self.health
            .state
            .lock()
            .record_queue_depth(self.source, self.frames.len());

        let dropped = self.frames.len().saturating_sub(MAX_MIX_QUEUE_FRAMES);
        for _ in 0..dropped {
            self.frames.pop_front();
        }
        if dropped > 0 {
            self.health
                .state
                .lock()
                .record_dropped_frames(self.source, dropped);
        }

        // The existing backend performs an immediate `len() > MAX` check after
        // extending. Suppress that administrative check so it is not mistaken for
        // the mixer's 10 ms drain request.
        self.suppress_next_len_check.set(true);
    }
}

pub type GpuVecDeque<T> = InstrumentedVecDeque<T, GpuMixerBackend>;
pub type BufferVecDeque<T> = InstrumentedVecDeque<T, BufferMixerBackend>;
