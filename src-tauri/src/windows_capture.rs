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
        ImageEncoderPixelFormat, ImageFormat, VideoEncoderError, VideoSettingsBuilder,
        VideoSettingsSubType,
    };

    struct EncoderSubmissionHealth {
        output_path: PathBuf,
        started_at: Instant,
        video_submitted: u64,
        video_failed: u64,
        audio_buffers_submitted: u64,
        audio_failed: u64,
        audio_bytes_submitted: u64,
        first_video_timestamp_hns: Option<i64>,
        last_video_timestamp_hns: Option<i64>,
        largest_frame_gap_hns: i64,
        suppress_log: bool,
    }

    impl EncoderSubmissionHealth {
        fn new(output_path: PathBuf) -> Self {
            let suppress_log = output_path
                .file_name()
                .and_then(|value| value.to_str())
                .map(|value| value.starts_with("recorder-native-warmup-"))
                .unwrap_or(false);

            Self {
                output_path,
                started_at: Instant::now(),
                video_submitted: 0,
                video_failed: 0,
                audio_buffers_submitted: 0,
                audio_failed: 0,
                audio_bytes_submitted: 0,
                first_video_timestamp_hns: None,
                last_video_timestamp_hns: None,
                largest_frame_gap_hns: 0,
                suppress_log,
            }
        }

        fn record_video_success(&mut self, timestamp_hns: Option<i64>) {
            self.video_submitted = self.video_submitted.saturating_add(1);
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

        fn log(&self, finalize_ok: bool) {
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

            eprintln!(
                "[Recorder][StreamHealth] finalize_ok={} wall_ms={} frames_submitted={} frame_failures={} effective_fps={:.2} max_frame_gap_ms={:.1} audio_buffers={} audio_failures={} audio_bytes={} path={}",
                finalize_ok,
                self.started_at.elapsed().as_millis(),
                self.video_submitted,
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
            let inner = windows_capture_core::encoder::VideoEncoder::new(
                video_settings,
                audio_settings,
                container_settings,
                &output_path,
            )?;
            Ok(Self {
                inner,
                health: EncoderSubmissionHealth::new(output_path),
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
            let result = inner.finish();
            health.log(result.is_ok());
            result
        }
    }
}
