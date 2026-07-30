// Intercept the legacy diagnostics emitted by the implementation module before it
// is loaded. The capture/encoder behaviour is unchanged: this shim only renames the
// old frame-count percentage to sample density, derives real timestamp-span coverage,
// suppresses the obsolete density-based warning, and removes local paths from logs.
macro_rules! eprintln {
    (
        "[Recorder][StreamHealth] finalize_ok={} wall_ms={} active_wall_ms={} target_fps={} frames_received={} frames_rate_limited={} frames_submitted={} expected_frames={} frame_deficit={} timeline_coverage_pct={:.1} processing_deficit={} frame_failures={} capture_fps={:.2} effective_fps={:.2} max_capture_gap_ms={:.1} max_frame_gap_ms={:.1} audio_buffers={} audio_failures={} audio_bytes={} path={}",
        $finalize_ok:expr,
        $wall_ms:expr,
        $active_wall_ms:expr,
        $target_fps:expr,
        $frames_received:expr,
        $frames_rate_limited:expr,
        $frames_submitted:expr,
        $expected_frames:expr,
        $frame_deficit:expr,
        $sample_density_pct:expr,
        $processing_deficit:expr,
        $frame_failures:expr,
        $capture_fps:expr,
        $effective_fps:expr,
        $max_capture_gap_ms:expr,
        $max_frame_gap_ms:expr,
        $audio_buffers:expr,
        $audio_failures:expr,
        $audio_bytes:expr,
        $path:expr $(,)?
    ) => {{
        // Evaluate the path expression so the internally retained path remains an
        // intentional field, while never serializing it into diagnostic output.
        let _ = &$path;
        let active_wall_ms = $active_wall_ms;
        let frames_submitted = $frames_submitted;
        let effective_fps = $effective_fps;
        let timeline_span_ms = if frames_submitted > 1
            && effective_fps.is_finite()
            && effective_fps > 0.0
        {
            ((((frames_submitted - 1) as f64 / effective_fps) * 1_000.0).round()) as u128
        } else {
            0
        };
        let timeline_coverage_pct = if active_wall_ms > 0 {
            ((timeline_span_ms as f64 * 100.0) / active_wall_ms as f64).min(100.0)
        } else if frames_submitted > 0 {
            100.0
        } else {
            0.0
        };

        ::std::eprintln!(
            "[Recorder][StreamHealth] finalize_ok={} wall_ms={} active_wall_ms={} timeline_span_ms={} timeline_coverage_pct={:.1} target_fps={} frames_received={} frames_rate_limited={} frames_submitted={} expected_frames={} frame_deficit={} sample_density_pct={:.1} processing_deficit={} frame_failures={} capture_fps={:.2} effective_fps={:.2} max_capture_gap_ms={:.1} max_frame_gap_ms={:.1} audio_buffers={} audio_failures={} audio_bytes={}",
            $finalize_ok,
            $wall_ms,
            active_wall_ms,
            timeline_span_ms,
            timeline_coverage_pct,
            $target_fps,
            $frames_received,
            $frames_rate_limited,
            frames_submitted,
            $expected_frames,
            $frame_deficit,
            $sample_density_pct,
            $processing_deficit,
            $frame_failures,
            $capture_fps,
            effective_fps,
            $max_capture_gap_ms,
            $max_frame_gap_ms,
            $audio_buffers,
            $audio_failures,
            $audio_bytes,
        );

        if active_wall_ms >= 2_000 && timeline_coverage_pct < 90.0 {
            ::std::eprintln!(
                "[Recorder][StreamHealth] warning=low_timeline_coverage active_wall_ms={} timeline_span_ms={} coverage_pct={:.1}",
                active_wall_ms,
                timeline_span_ms,
                timeline_coverage_pct,
            );
        }
    }};
    (
        "[Recorder][StreamHealth] warning=no_wgc_frames_received path={}",
        $path:expr $(,)?
    ) => {
        ::std::eprintln!("[Recorder][StreamHealth] warning=no_wgc_frames_received")
    };
    (
        "[Recorder][StreamHealth] warning=no_video_frames_submitted path={}",
        $path:expr $(,)?
    ) => {
        ::std::eprintln!("[Recorder][StreamHealth] warning=no_video_frames_submitted")
    };
    (
        "[Recorder][StreamHealth] warning=large_capture_gap gap_ms={:.1} path={}",
        $gap_ms:expr,
        $path:expr $(,)?
    ) => {
        ::std::eprintln!(
            "[Recorder][StreamHealth] warning=large_capture_gap gap_ms={:.1}",
            $gap_ms,
        )
    };
    (
        "[Recorder][StreamHealth] warning=large_video_gap gap_ms={:.1} path={}",
        $gap_ms:expr,
        $path:expr $(,)?
    ) => {
        ::std::eprintln!(
            "[Recorder][StreamHealth] warning=large_video_gap gap_ms={:.1}",
            $gap_ms,
        )
    };
    (
        "[Recorder][StreamHealth] warning=capture_processing_deficit expected_attempts={} actual_attempts={} deficit={} path={}",
        $expected_attempts:expr,
        $actual_attempts:expr,
        $deficit:expr,
        $path:expr $(,)?
    ) => {
        ::std::eprintln!(
            "[Recorder][StreamHealth] warning=capture_processing_deficit expected_attempts={} actual_attempts={} deficit={}",
            $expected_attempts,
            $actual_attempts,
            $deficit,
        )
    };
    (
        "[Recorder][StreamHealth] warning=low_timeline_coverage expected={} submitted={} deficit={} coverage_pct={:.1} path={}",
        $expected:expr,
        $submitted:expr,
        $deficit:expr,
        $coverage_pct:expr,
        $path:expr $(,)?
    ) => {{
        // The main StreamHealth arm emits the timestamp-span warning. The legacy
        // warning measured constant-frame sample density and is intentionally muted.
    }};
    (
        "[Recorder][StreamHealth] warning=encoder_submission_failures video={} audio={} path={}",
        $video:expr,
        $audio:expr,
        $path:expr $(,)?
    ) => {
        ::std::eprintln!(
            "[Recorder][StreamHealth] warning=encoder_submission_failures video={} audio={}",
            $video,
            $audio,
        )
    };
    (
        "[Recorder][AvHealth] finalize_ok={} audio_buffers={} audio_bytes={} audio_duration_ms={} video_duration_ms={} media_drift_ms={} startup_offset_ms={} max_audio_submit_gap_ms={} path={}",
        $finalize_ok:expr,
        $audio_buffers:expr,
        $audio_bytes:expr,
        $audio_duration_ms:expr,
        $video_duration_ms:expr,
        $media_drift_ms:expr,
        $startup_offset_ms:expr,
        $max_audio_submit_gap_ms:expr,
        $path:expr $(,)?
    ) => {{
        // Retain the path internally for warm-up classification, but never log it.
        let _ = &$path;
        ::std::eprintln!(
            "[Recorder][AvHealth] finalize_ok={} audio_buffers={} audio_bytes={} audio_duration_ms={} video_duration_ms={} media_drift_ms={} startup_offset_ms={} max_audio_submit_gap_ms={}",
            $finalize_ok,
            $audio_buffers,
            $audio_bytes,
            $audio_duration_ms,
            $video_duration_ms,
            $media_drift_ms,
            $startup_offset_ms,
            $max_audio_submit_gap_ms,
        )
    }};
    (
        "[Recorder][AvHealth] warning=large_media_drift drift_ms={} audio_ms={} video_ms={} path={}",
        $drift_ms:expr,
        $audio_ms:expr,
        $video_ms:expr,
        $path:expr $(,)?
    ) => {
        ::std::eprintln!(
            "[Recorder][AvHealth] warning=large_media_drift drift_ms={} audio_ms={} video_ms={}",
            $drift_ms,
            $audio_ms,
            $video_ms,
        )
    };
    (
        "[Recorder][AvHealth] warning=large_audio_submission_gap gap_ms={} path={}",
        $gap_ms:expr,
        $path:expr $(,)?
    ) => {
        ::std::eprintln!(
            "[Recorder][AvHealth] warning=large_audio_submission_gap gap_ms={}",
            $gap_ms,
        )
    };
    ($($arg:tt)*) => {
        ::std::eprintln!($($arg)*)
    };
}

// Load the instrumentation implementation as a normal Rust module. Using a
// module path keeps the `//!` comments at the top of lib.rs valid as inner
// module documentation, while the crate's Rust 2024 edition retains the
// callback-guard temporary lifetime fix.
pub mod diagnostics;
#[path = "mixer_diagnostics.rs"]
pub mod mixer_diagnostics;

#[allow(unused_imports)]
#[path = "lib.rs"]
mod implementation;

pub use implementation::{capture, d3d11, frame, graphics_capture_api, monitor, settings, window};

pub mod encoder {
    use std::path::{Path, PathBuf};
    use std::time::Instant;

    use crate::diagnostics;
    use crate::frame::Frame;

    pub use crate::implementation::encoder::{
        ContainerSettingsBuilder, ImageEncoder, ImageEncoderPixelFormat, ImageFormat,
        VideoEncoderError, VideoSettingsBuilder, VideoSettingsSubType,
    };

    #[derive(Clone, Copy)]
    struct AudioFormat {
        sample_rate: u32,
        channels: u32,
        bits_per_sample: u32,
    }

    /// Drop-in audio settings wrapper that retains the PCM format used for
    /// submitted-duration and A/V timeline diagnostics.
    pub struct AudioSettingsBuilder {
        inner: crate::implementation::encoder::AudioSettingsBuilder,
        format: AudioFormat,
        disabled: bool,
    }

    impl AudioSettingsBuilder {
        pub fn new() -> Self {
            Self {
                inner: crate::implementation::encoder::AudioSettingsBuilder::new(),
                format: AudioFormat {
                    sample_rate: 48_000,
                    channels: 2,
                    bits_per_sample: 16,
                },
                disabled: false,
            }
        }

        pub fn sample_rate(mut self, sample_rate: u32) -> Self {
            self.inner = self.inner.sample_rate(sample_rate);
            self.format.sample_rate = sample_rate;
            self
        }

        pub fn channel_count(mut self, channels: u32) -> Self {
            self.inner = self.inner.channel_count(channels);
            self.format.channels = channels;
            self
        }

        pub fn bit_per_sample(mut self, bits_per_sample: u32) -> Self {
            self.inner = self.inner.bit_per_sample(bits_per_sample);
            self.format.bits_per_sample = bits_per_sample;
            self
        }

        pub fn disabled(mut self, disabled: bool) -> Self {
            self.inner = self.inner.disabled(disabled);
            self.disabled = disabled;
            self
        }

        fn into_parts(
            self,
        ) -> (
            crate::implementation::encoder::AudioSettingsBuilder,
            Option<AudioFormat>,
        ) {
            let format = (!self.disabled).then_some(self.format);
            (self.inner, format)
        }
    }

    impl Default for AudioSettingsBuilder {
        fn default() -> Self {
            Self::new()
        }
    }

    struct AvSubmissionHealth {
        output_path: PathBuf,
        audio_format: Option<AudioFormat>,
        audio_bytes: u64,
        audio_buffers: u64,
        first_audio_wall: Option<Instant>,
        last_audio_wall: Option<Instant>,
        max_audio_submit_gap_ms: u128,
        first_video_wall: Option<Instant>,
        first_video_timestamp_hns: Option<i64>,
        last_video_timestamp_hns: Option<i64>,
        suppress_log: bool,
    }

    impl AvSubmissionHealth {
        fn new(output_path: PathBuf, audio_format: Option<AudioFormat>) -> Self {
            let suppress_log = output_path
                .file_name()
                .and_then(|value| value.to_str())
                .map(|value| value.starts_with("recorder-native-warmup-"))
                .unwrap_or(false);
            Self {
                output_path,
                audio_format,
                audio_bytes: 0,
                audio_buffers: 0,
                first_audio_wall: None,
                last_audio_wall: None,
                max_audio_submit_gap_ms: 0,
                first_video_wall: None,
                first_video_timestamp_hns: None,
                last_video_timestamp_hns: None,
                suppress_log,
            }
        }

        fn record_video_success(&mut self, timestamp_hns: Option<i64>) {
            if self.first_video_wall.is_none() {
                self.first_video_wall = Some(Instant::now());
            }
            let Some(timestamp_hns) = timestamp_hns else {
                return;
            };
            if self.first_video_timestamp_hns.is_none() {
                self.first_video_timestamp_hns = Some(timestamp_hns);
            }
            self.last_video_timestamp_hns = Some(timestamp_hns);
        }

        fn record_audio_success(&mut self, bytes: usize) {
            let now = Instant::now();
            if self.first_audio_wall.is_none() {
                self.first_audio_wall = Some(now);
            }
            if let Some(previous) = self.last_audio_wall {
                self.max_audio_submit_gap_ms = self
                    .max_audio_submit_gap_ms
                    .max(now.duration_since(previous).as_millis());
            }
            self.last_audio_wall = Some(now);
            self.audio_buffers = self.audio_buffers.saturating_add(1);
            self.audio_bytes = self
                .audio_bytes
                .saturating_add(u64::try_from(bytes).unwrap_or(u64::MAX));
        }

        fn log(&self, finalize_ok: bool) {
            if self.suppress_log {
                return;
            }
            let Some(format) = self.audio_format else {
                return;
            };

            let bytes_per_second = u64::from(format.sample_rate)
                .saturating_mul(u64::from(format.channels))
                .saturating_mul(u64::from(format.bits_per_sample))
                / 8;
            let audio_duration_ms = if bytes_per_second > 0 {
                self.audio_bytes.saturating_mul(1_000) / bytes_per_second
            } else {
                0
            };
            let video_duration_ms = match (
                self.first_video_timestamp_hns,
                self.last_video_timestamp_hns,
            ) {
                (Some(first), Some(last)) => {
                    u64::try_from(last.saturating_sub(first).max(0)).unwrap_or(u64::MAX) / 10_000
                }
                _ => 0,
            };
            let media_drift_ms = i128::from(audio_duration_ms) - i128::from(video_duration_ms);
            let startup_offset_ms = match (self.first_audio_wall, self.first_video_wall) {
                (Some(audio), Some(video)) if audio >= video => {
                    i128::try_from(audio.duration_since(video).as_millis()).unwrap_or(i128::MAX)
                }
                (Some(audio), Some(video)) => {
                    -i128::try_from(video.duration_since(audio).as_millis()).unwrap_or(i128::MAX)
                }
                _ => 0,
            };

            eprintln!(
                "[Recorder][AvHealth] finalize_ok={} audio_buffers={} audio_bytes={} audio_duration_ms={} video_duration_ms={} media_drift_ms={} startup_offset_ms={} max_audio_submit_gap_ms={} path={}",
                finalize_ok,
                self.audio_buffers,
                self.audio_bytes,
                audio_duration_ms,
                video_duration_ms,
                media_drift_ms,
                startup_offset_ms,
                self.max_audio_submit_gap_ms,
                self.output_path.display(),
            );

            if video_duration_ms >= 2_000 && media_drift_ms.abs() > 250 {
                eprintln!(
                    "[Recorder][AvHealth] warning=large_media_drift drift_ms={} audio_ms={} video_ms={} path={}",
                    media_drift_ms,
                    audio_duration_ms,
                    video_duration_ms,
                    self.output_path.display(),
                );
            }
            if self.max_audio_submit_gap_ms > 250 {
                eprintln!(
                    "[Recorder][AvHealth] warning=large_audio_submission_gap gap_ms={} path={}",
                    self.max_audio_submit_gap_ms,
                    self.output_path.display(),
                );
            }
        }
    }

    /// Final facade layer that preserves the existing encoder health wrapper while
    /// correlating successful submissions with camera and A/V timeline diagnostics.
    pub struct VideoEncoder {
        inner: crate::implementation::encoder::VideoEncoder,
        av_health: AvSubmissionHealth,
    }

    impl VideoEncoder {
        pub fn new<P: AsRef<Path>>(
            video_settings: VideoSettingsBuilder,
            audio_settings: AudioSettingsBuilder,
            container_settings: ContainerSettingsBuilder,
            path: P,
        ) -> Result<Self, VideoEncoderError> {
            let output_path = path.as_ref().to_path_buf();
            let (audio_settings, audio_format) = audio_settings.into_parts();
            crate::implementation::encoder::VideoEncoder::new(
                video_settings,
                audio_settings,
                container_settings,
                &output_path,
            )
            .map(|inner| Self {
                inner,
                av_health: AvSubmissionHealth::new(output_path, audio_format),
            })
        }

        pub fn send_frame(&mut self, frame: &Frame) -> Result<(), VideoEncoderError> {
            let timestamp = frame.timestamp().ok().map(|value| value.Duration);
            let result = self.inner.send_frame(frame);
            if result.is_ok() {
                self.av_health.record_video_success(timestamp);
                diagnostics::record_camera_overlay_submission();
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
                self.av_health.record_video_success(Some(timestamp));
                diagnostics::record_camera_overlay_submission();
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
                self.av_health.record_audio_success(buffer.len());
            }
            result
        }

        pub fn finish(self) -> Result<(), VideoEncoderError> {
            let Self { inner, av_health } = self;
            let result = inner.finish();
            av_health.log(result.is_ok());
            diagnostics::finish_camera_segment(result.is_ok());
            result
        }
    }
}
