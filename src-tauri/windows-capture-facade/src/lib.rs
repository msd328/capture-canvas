//! Instrumented facade over the MIT-licensed `windows-capture` crate.
//!
//! Capture code continues using the familiar `windows_capture::...` paths. The
//! facade delegates every operation to the upstream crate while wrapping
//! `VideoEncoder` with lightweight per-segment submission counters.

pub mod capture {
    pub use windows_capture_core::capture::*;
}

pub mod d3d11 {
    pub use windows_capture_core::d3d11::*;
}

pub mod frame {
    pub use windows_capture_core::frame::*;
}

pub mod graphics_capture_api {
    pub use windows_capture_core::graphics_capture_api::*;
}

pub mod monitor {
    pub use windows_capture_core::monitor::*;
}

pub mod settings {
    pub use windows_capture_core::settings::*;
}

pub mod window {
    pub use windows_capture_core::window::*;
}

pub mod encoder {
    use std::path::{Path, PathBuf};
    use std::time::Instant;

    use super::frame::Frame;

    pub use windows_capture_core::encoder::{
        AudioSettingsBuilder, ContainerSettingsBuilder, ImageEncoder,
        ImageEncoderPixelFormat, ImageFormat, VideoEncoderError, VideoSettingsSubType,
    };

    /// Drop-in video settings wrapper that retains the requested frame rate for
    /// recorder continuity diagnostics while delegating all encoding settings to
    /// the upstream `windows-capture` builder.
    pub struct VideoSettingsBuilder {
        inner: windows_capture_core::encoder::VideoSettingsBuilder,
        target_fps: u32,
    }

    impl VideoSettingsBuilder {
        pub fn new(width: u32, height: u32) -> Self {
            Self {
                inner: windows_capture_core::encoder::VideoSettingsBuilder::new(width, height),
                target_fps: 60,
            }
        }

        pub fn sub_type(mut self, sub_type: VideoSettingsSubType) -> Self {
            self.inner = self.inner.sub_type(sub_type);
            self
        }

        pub fn bitrate(mut self, bitrate: u32) -> Self {
            self.inner = self.inner.bitrate(bitrate);
            self
        }

        pub fn width(mut self, width: u32) -> Self {
            self.inner = self.inner.width(width);
            self
        }

        pub fn height(mut self, height: u32) -> Self {
            self.inner = self.inner.height(height);
            self
        }

        pub fn frame_rate(mut self, frame_rate: u32) -> Self {
            self.target_fps = frame_rate.max(1);
            self.inner = self.inner.frame_rate(frame_rate);
            self
        }

        pub fn pixel_aspect_ratio(mut self, ratio: (u32, u32)) -> Self {
            self.inner = self.inner.pixel_aspect_ratio(ratio);
            self
        }

        pub fn disabled(mut self, disabled: bool) -> Self {
            self.inner = self.inner.disabled(disabled);
            self
        }

        fn into_inner(self) -> (windows_capture_core::encoder::VideoSettingsBuilder, u32) {
            (self.inner, self.target_fps.max(1))
        }
    }

    struct EncoderSubmissionHealth {
        output_path: PathBuf,
        started_at: Instant,
        target_fps: u32,
        video_submitted: u64,
        video_failed: u64,
        audio_buffers_submitted: u64,
        audio_failed: u64,
        audio_bytes_submitted: u64,
        first_video_timestamp_hns: Option<i64>,
        last_video_timestamp_hns: Option<i64>,
        first_video_wall: Option<Instant>,
        largest_frame_gap_hns: i64,
        suppress_log: bool,
    }

    impl EncoderSubmissionHealth {
        fn new(output_path: PathBuf, target_fps: u32) -> Self {
            let suppress_log = output_path
                .file_name()
                .and_then(|value| value.to_str())
                .map(|value| value.starts_with("recorder-native-warmup-"))
                .unwrap_or(false);

            Self {
                output_path,
                started_at: Instant::now(),
                target_fps: target_fps.max(1),
                video_submitted: 0,
                video_failed: 0,
                audio_buffers_submitted: 0,
                audio_failed: 0,
                audio_bytes_submitted: 0,
                first_video_timestamp_hns: None,
                last_video_timestamp_hns: None,
                first_video_wall: None,
                largest_frame_gap_hns: 0,
                suppress_log,
            }
        }

        fn record_video_success(&mut self, timestamp_hns: Option<i64>) {
            self.video_submitted = self.video_submitted.saturating_add(1);
            if self.first_video_wall.is_none() {
                self.first_video_wall = Some(Instant::now());
            }

            let Some(timestamp_hns) = timestamp_hns else {
                return;
            };

            if self.first_video_timestamp_hns.is_none() {
                self.first_video_timestamp_hns = Some(timestamp_hns);
            }
            if let Some(previous) = self.last_video_timestamp_hns {
                let gap = timestamp_hns.saturating_sub(previous).max(0);
                self.largest_frame_gap_hns = self.largest_frame_gap_hns.max(gap);
            }
            self.last_video_timestamp_hns = Some(timestamp_hns);
        }

        fn record_video_failure(&mut self) {
            self.video_failed = self.video_failed.saturating_add(1);
        }

        fn record_audio_success(&mut self, bytes: usize) {
            self.audio_buffers_submitted = self.audio_buffers_submitted.saturating_add(1);
            self.audio_bytes_submitted = self
                .audio_bytes_submitted
                .saturating_add(u64::try_from(bytes).unwrap_or(u64::MAX));
        }

        fn record_audio_failure(&mut self) {
            self.audio_failed = self.audio_failed.saturating_add(1);
        }

        fn log(&self, finalize_ok: bool, capture_ended_at: Instant) {
            if self.suppress_log {
                return;
            }

            let timestamp_span_hns = match (
                self.first_video_timestamp_hns,
                self.last_video_timestamp_hns,
            ) {
                (Some(first), Some(last)) => last.saturating_sub(first).max(0),
                _ => 0,
            };
            let timestamp_span_seconds = timestamp_span_hns as f64 / 10_000_000.0;
            let effective_fps = if self.video_submitted > 1 && timestamp_span_seconds > 0.0 {
                (self.video_submitted - 1) as f64 / timestamp_span_seconds
            } else {
                0.0
            };
            let largest_frame_gap_ms = self.largest_frame_gap_hns as f64 / 10_000.0;

            // Timestamp-based FPS cannot see a missing tail after the final WGC frame.
            // Compare submissions with wall time measured from the first accepted frame
            // to the instant capture shutdown began. A low coverage value exposes static
            // screen delivery stalls and other missing-frame periods.
            let active_wall_ms = self
                .first_video_wall
                .map(|first| capture_ended_at.duration_since(first).as_millis())
                .unwrap_or(0);
            let expected_frames_u128 = if self.video_submitted == 0 || active_wall_ms == 0 {
                u128::from(self.video_submitted)
            } else {
                active_wall_ms
                    .saturating_mul(u128::from(self.target_fps))
                    .saturating_add(999)
                    / 1_000
            };
            let expected_frames = u64::try_from(expected_frames_u128.max(1)).unwrap_or(u64::MAX);
            let frame_deficit = expected_frames.saturating_sub(self.video_submitted);
            let coverage_percent = if expected_frames > 0 {
                self.video_submitted as f64 * 100.0 / expected_frames as f64
            } else {
                100.0
            };

            eprintln!(
                "[Recorder][StreamHealth] finalize_ok={} wall_ms={} active_wall_ms={} target_fps={} frames_submitted={} expected_frames={} frame_deficit={} timeline_coverage_pct={:.1} frame_failures={} effective_fps={:.2} max_frame_gap_ms={:.1} audio_buffers={} audio_failures={} audio_bytes={} path={}",
                finalize_ok,
                self.started_at.elapsed().as_millis(),
                active_wall_ms,
                self.target_fps,
                self.video_submitted,
                expected_frames,
                frame_deficit,
                coverage_percent,
                self.video_failed,
                effective_fps,
                largest_frame_gap_ms,
                self.audio_buffers_submitted,
                self.audio_failed,
                self.audio_bytes_submitted,
                self.output_path.display(),
            );

            if self.video_submitted == 0 {
                eprintln!(
                    "[Recorder][StreamHealth] warning=no_video_frames_submitted path={}",
                    self.output_path.display()
                );
            }
            if self.largest_frame_gap_hns > 10_000_000 {
                eprintln!(
                    "[Recorder][StreamHealth] warning=large_video_gap gap_ms={:.1} path={}",
                    largest_frame_gap_ms,
                    self.output_path.display()
                );
            }
            if active_wall_ms >= 2_000 && coverage_percent < 90.0 {
                eprintln!(
                    "[Recorder][StreamHealth] warning=low_timeline_coverage expected={} submitted={} deficit={} coverage_pct={:.1} path={}",
                    expected_frames,
                    self.video_submitted,
                    frame_deficit,
                    coverage_percent,
                    self.output_path.display()
                );
            }
            if self.video_failed > 0 || self.audio_failed > 0 {
                eprintln!(
                    "[Recorder][StreamHealth] warning=encoder_submission_failures video={} audio={} path={}",
                    self.video_failed,
                    self.audio_failed,
                    self.output_path.display()
                );
            }
        }
    }

    /// Drop-in wrapper that delegates encoding to `windows-capture` and records
    /// successful/failed video and audio submissions for the current segment.
    pub struct VideoEncoder {
        inner: windows_capture_core::encoder::VideoEncoder,
        health: EncoderSubmissionHealth,
    }

    impl VideoEncoder {
        pub fn new<P: AsRef<Path>>(
            video_settings: VideoSettingsBuilder,
            audio_settings: AudioSettingsBuilder,
            container_settings: ContainerSettingsBuilder,
            path: P,
        ) -> Result<Self, VideoEncoderError> {
            let output_path = path.as_ref().to_path_buf();
            let (video_settings, target_fps) = video_settings.into_inner();
            let inner = windows_capture_core::encoder::VideoEncoder::new(
                video_settings,
                audio_settings,
                container_settings,
                &output_path,
            )?;
            Ok(Self {
                inner,
                health: EncoderSubmissionHealth::new(output_path, target_fps),
            })
        }

        pub fn send_frame(&mut self, frame: &Frame) -> Result<(), VideoEncoderError> {
            let timestamp = frame.timestamp().ok().map(|value| value.Duration);
            let result = self.inner.send_frame(frame);
            if result.is_ok() {
                self.health.record_video_success(timestamp);
            } else {
                self.health.record_video_failure();
            }
            result
        }

        pub fn send_frame_buffer(
            &mut self,
            buffer: &[u8],
            timestamp: i64,
        ) -> Result<(), VideoEncoderError> {
            let result = self.inner.send_frame_buffer(buffer, timestamp);
            if result.is_ok() {
                self.health.record_video_success(Some(timestamp));
            } else {
                self.health.record_video_failure();
            }
            result
        }

        pub fn send_audio_buffer(
            &mut self,
            buffer: &[u8],
            timestamp: i64,
        ) -> Result<(), VideoEncoderError> {
            let result = self.inner.send_audio_buffer(buffer, timestamp);
            if result.is_ok() {
                self.health.record_audio_success(buffer.len());
            } else {
                self.health.record_audio_failure();
            }
            result
        }

        pub fn finish(self) -> Result<(), VideoEncoderError> {
            let Self { inner, health } = self;
            let capture_ended_at = Instant::now();
            let result = inner.finish();
            health.log(result.is_ok(), capture_ended_at);
            result
        }
    }
}
