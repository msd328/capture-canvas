//! Per-segment microphone/system mixer health counters.

use parking_lot::Mutex;
use std::collections::VecDeque;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::time::Instant;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MixerSource {
    Microphone,
    System,
}

impl MixerSource {
    const fn label(self) -> &'static str {
        match self {
            Self::Microphone => "microphone",
            Self::System => "system",
        }
    }
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

    fn record_drop(&mut self, source: MixerSource) {
        match source {
            MixerSource::Microphone => {
                self.microphone_dropped_frames = self.microphone_dropped_frames.saturating_add(1);
            }
            MixerSource::System => {
                self.system_dropped_frames = self.system_dropped_frames.saturating_add(1);
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

pub struct AudioFrameQueue {
    frames: VecDeque<[f32; 2]>,
    source: MixerSource,
    max_frames: usize,
    health: Arc<MixerHealthSession>,
}

impl AudioFrameQueue {
    fn new(
        source: MixerSource,
        max_frames: usize,
        health: Arc<MixerHealthSession>,
    ) -> Self {
        Self {
            frames: VecDeque::new(),
            source,
            max_frames,
            health,
        }
    }

    pub fn record_underrun(&self) {
        self.health.state.lock().record_underrun(self.source);
    }

    pub fn trim_overflow(&mut self) {
        while self.frames.len() > self.max_frames {
            self.frames.pop_front();
            self.health.state.lock().record_drop(self.source);
        }
    }
}

impl Deref for AudioFrameQueue {
    type Target = VecDeque<[f32; 2]>;

    fn deref(&self) -> &Self::Target {
        &self.frames
    }
}

impl DerefMut for AudioFrameQueue {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.frames
    }
}

pub type SharedAudioFrameQueue = Arc<Mutex<AudioFrameQueue>>;

pub fn new_audio_mixer_queues(
    backend: &'static str,
    max_frames: usize,
) -> (SharedAudioFrameQueue, SharedAudioFrameQueue) {
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

    let microphone = Arc::new(Mutex::new(AudioFrameQueue::new(
        MixerSource::Microphone,
        max_frames,
        health.clone(),
    )));
    let system = Arc::new(Mutex::new(AudioFrameQueue::new(
        MixerSource::System,
        max_frames,
        health,
    )));
    (microphone, system)
}

pub fn record_queue_depth(queue: &SharedAudioFrameQueue) {
    let queue = queue.lock();
    queue
        .health
        .state
        .lock()
        .record_queue_depth(queue.source, queue.frames.len());
}
