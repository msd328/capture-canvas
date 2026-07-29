//! Final recorder-facing facade that preserves the last visible frame at Stop.
//!
//! Windows Graphics Capture can avoid producing new frames while a source remains
//! unchanged. The underlying encoder timestamps samples from WGC, so stopping during
//! a long unchanged tail can otherwise leave the MP4 shorter than wall-clock time.
//! This layer retains one throttled BGRA snapshot and submits it once at the current
//! wall-clock position immediately before encoder finalisation.

pub use windows_capture_base::{
    capture, d3d11, diagnostics, frame, graphics_capture_api, mixer_diagnostics, monitor, settings,
    window,
};

pub mod encoder {
    use std::path::Path;
    use std::time::{Duration, Instant};

    use crate::frame::Frame;

    pub use windows_capture_base::encoder::{
        AudioSettingsBuilder, ContainerSettingsBuilder, ImageEncoder, ImageEncoderPixelFormat,
        ImageFormat, VideoEncoderError, VideoSettingsBuilder, VideoSettingsSubType,
    };

    const SNAPSHOT_REFRESH_INTERVAL: Duration = Duration::from_secs(1);
    const STATIC_TAIL_THRESHOLD_HNS: i64 = 1_000_000; // 100 ms in 100 ns units.

    #[derive(Default)]
    struct StaticContinuityHealth {
        first_video_wall: Option<Instant>,
        first_video_timestamp_hns: Option<i64>,
        last_video_timestamp_hns: Option<i64>,
        last_snapshot: Option<Vec<u8>>,
        last_snapshot_at: Option<Instant>,
        snapshot_refreshes: u64,
        tail_gap_ms_before_hold: u128,
        hold_needed: bool,
        hold_submitted: bool,
        hold_failed: bool,
        hold_unavailable: bool,
        suppress_log: bool,
    }

    impl StaticContinuityHealth {
        fn new<P: AsRef<Path>>(path: P) -> Self {
            let suppress_log = path
                .as_ref()
                .file_name()
                .and_then(|value| value.to_str())
                .map(|value| value.starts_with("recorder-native-warmup-"))
                .unwrap_or(false);
            Self {
                suppress_log,
                ..Self::default()
            }
        }

        fn should_refresh_snapshot(&self) -> bool {
            self.last_snapshot_at
                .map(|at| at.elapsed() >= SNAPSHOT_REFRESH_INTERVAL)
                .unwrap_or(true)
        }

        fn snapshot_frame(&self, frame: &mut Frame) -> Option<Vec<u8>> {
            let width = usize::try_from(frame.width()).ok()?;
            let height = usize::try_from(frame.height()).ok()?;
            let row_bytes = width.checked_mul(4)?;
            let expected_len = row_bytes.checked_mul(height)?;
            if expected_len == 0 {
                return None;
            }

            let frame_buffer = frame.buffer().ok()?;
            let mut compact = Vec::new();
            let pixels = frame_buffer.as_nopadding_buffer(&mut compact);
            if pixels.len() != expected_len {
                return None;
            }

            // `send_frame_buffer` expects packed BGRA rows in bottom-to-top order.
            let mut flipped = Vec::with_capacity(expected_len);
            for row in pixels.chunks_exact(row_bytes).rev() {
                flipped.extend_from_slice(row);
            }
            Some(flipped)
        }

        fn record_video_success(&mut self, timestamp_hns: Option<i64>, snapshot: Option<Vec<u8>>) {
            let now = Instant::now();
            if self.first_video_wall.is_none() {
                self.first_video_wall = Some(now);
            }
            if let Some(timestamp_hns) = timestamp_hns {
                if self.first_video_timestamp_hns.is_none() {
                    self.first_video_timestamp_hns = Some(timestamp_hns);
                }
                self.last_video_timestamp_hns = Some(timestamp_hns);
            }
            if let Some(snapshot) = snapshot {
                self.last_snapshot = Some(snapshot);
                self.last_snapshot_at = Some(now);
                self.snapshot_refreshes = self.snapshot_refreshes.saturating_add(1);
            }
        }

        fn append_final_hold(
            &mut self,
            encoder: &mut windows_capture_base::encoder::VideoEncoder,
            capture_ended_at: Instant,
        ) {
            let (Some(first_wall), Some(first_timestamp), Some(last_timestamp)) = (
                self.first_video_wall,
                self.first_video_timestamp_hns,
                self.last_video_timestamp_hns,
            ) else {
                return;
            };

            let elapsed_hns = i64::try_from(
                capture_ended_at
                    .duration_since(first_wall)
                    .as_nanos()
                    .saturating_div(100),
            )
            .unwrap_or(i64::MAX);
            let target_timestamp = first_timestamp.saturating_add(elapsed_hns);
            let tail_gap_hns = target_timestamp.saturating_sub(last_timestamp).max(0);
            self.tail_gap_ms_before_hold =
                u128::try_from(tail_gap_hns).unwrap_or(u128::MAX) / 10_000;
            self.hold_needed = tail_gap_hns > STATIC_TAIL_THRESHOLD_HNS;
            if !self.hold_needed {
                return;
            }

            let Some(snapshot) = self.last_snapshot.as_deref() else {
                self.hold_unavailable = true;
                return;
            };

            match encoder.send_frame_buffer(snapshot, target_timestamp) {
                Ok(()) => {
                    self.hold_submitted = true;
                    self.last_video_timestamp_hns = Some(target_timestamp);
                }
                Err(_) => {
                    self.hold_failed = true;
                }
            }
        }

        fn log(&self, finalize_ok: bool, capture_ended_at: Instant) {
            if self.suppress_log {
                return;
            }
            let active_wall_ms = self
                .first_video_wall
                .map(|first| capture_ended_at.duration_since(first).as_millis())
                .unwrap_or(0);
            let video_span_ms = match (
                self.first_video_timestamp_hns,
                self.last_video_timestamp_hns,
            ) {
                (Some(first), Some(last)) => {
                    u128::try_from(last.saturating_sub(first).max(0)).unwrap_or(u128::MAX) / 10_000
                }
                _ => 0,
            };

            eprintln!(
                "[Recorder][ContinuityHealth] finalize_ok={} active_wall_ms={} video_span_ms={} tail_gap_ms_before_hold={} snapshot_refreshes={} snapshot_available={} hold_needed={} hold_submitted={} hold_failed={} hold_unavailable={}",
                finalize_ok,
                active_wall_ms,
                video_span_ms,
                self.tail_gap_ms_before_hold,
                self.snapshot_refreshes,
                self.last_snapshot.is_some(),
                self.hold_needed,
                self.hold_submitted,
                self.hold_failed,
                self.hold_unavailable,
            );

            if self.hold_unavailable {
                eprintln!(
                    "[Recorder][ContinuityHealth] warning=static_tail_snapshot_unavailable tail_gap_ms={}",
                    self.tail_gap_ms_before_hold,
                );
            }
            if self.hold_failed {
                eprintln!(
                    "[Recorder][ContinuityHealth] warning=static_tail_submission_failed tail_gap_ms={}",
                    self.tail_gap_ms_before_hold,
                );
            }
        }
    }

    /// Recorder-facing encoder wrapper that delegates all media work to the existing
    /// diagnostics facade and adds only final static-tail continuity handling.
    pub struct VideoEncoder {
        inner: windows_capture_base::encoder::VideoEncoder,
        continuity: StaticContinuityHealth,
    }

    impl VideoEncoder {
        pub fn new<P: AsRef<Path>>(
            video_settings: VideoSettingsBuilder,
            audio_settings: AudioSettingsBuilder,
            container_settings: ContainerSettingsBuilder,
            path: P,
        ) -> Result<Self, VideoEncoderError> {
            let continuity = StaticContinuityHealth::new(path.as_ref());
            windows_capture_base::encoder::VideoEncoder::new(
                video_settings,
                audio_settings,
                container_settings,
                path,
            )
            .map(|inner| Self { inner, continuity })
        }

        pub fn send_frame(&mut self, frame: &mut Frame) -> Result<(), VideoEncoderError> {
            let timestamp = frame.timestamp().ok().map(|value| value.Duration);
            let snapshot = self
                .continuity
                .should_refresh_snapshot()
                .then(|| self.continuity.snapshot_frame(frame))
                .flatten();
            let result = self.inner.send_frame(frame);
            if result.is_ok() {
                self.continuity.record_video_success(timestamp, snapshot);
            }
            result
        }

        pub fn send_frame_buffer(
            &mut self,
            buffer: &[u8],
            timestamp: i64,
        ) -> Result<(), VideoEncoderError> {
            let snapshot = self
                .continuity
                .should_refresh_snapshot()
                .then(|| buffer.to_vec());
            let result = self.inner.send_frame_buffer(buffer, timestamp);
            if result.is_ok() {
                self.continuity
                    .record_video_success(Some(timestamp), snapshot);
            }
            result
        }

        pub fn send_audio_buffer(
            &mut self,
            buffer: &[u8],
            timestamp: i64,
        ) -> Result<(), VideoEncoderError> {
            self.inner.send_audio_buffer(buffer, timestamp)
        }

        pub fn finish(self) -> Result<(), VideoEncoderError> {
            let capture_ended_at = Instant::now();
            let Self {
                mut inner,
                mut continuity,
            } = self;
            continuity.append_final_hold(&mut inner, capture_ended_at);
            let result = inner.finish();
            continuity.log(result.is_ok(), capture_ended_at);
            result
        }
    }
}
