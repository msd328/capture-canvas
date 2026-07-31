//! Instrumented facade over the MIT-licensed `windows-capture` crate.
//!
//! Capture code continues using the familiar `windows_capture::...` paths. The
//! facade delegates capture and encoding to the upstream crate while adding
//! per-segment delivery, limiter, video, and audio diagnostics.

use parking_lot::Mutex;
use std::cell::RefCell;
use std::sync::Arc;
use std::time::Instant;

#[derive(Default)]
struct CaptureDeliveryTiming {
    target_fps: u32,
    frame_interval_hns: i64,
    next_frame_hns: Option<i64>,
    frames_received: u64,
    frames_rate_limited: u64,
    first_timestamp_hns: Option<i64>,
    last_timestamp_hns: Option<i64>,
    largest_frame_gap_hns: i64,
}

#[derive(Clone, Copy, Default)]
struct CaptureDeliverySnapshot {
    frames_received: u64,
    frames_rate_limited: u64,
    capture_fps: f64,
    largest_frame_gap_ms: f64,
}

#[derive(Default)]
struct CaptureDeliveryHealth {
    timing: Mutex<CaptureDeliveryTiming>,
}

impl CaptureDeliveryHealth {
    fn configure_target_fps(&self, target_fps: u32) {
        let target_fps = target_fps.max(1);
        let mut timing = self.timing.lock();
        timing.target_fps = target_fps;
        timing.frame_interval_hns = (10_000_000i64 / i64::from(target_fps)).max(1);
        timing.next_frame_hns = None;
    }

    fn record_received(&self, timestamp_hns: Option<i64>) {
        let mut timing = self.timing.lock();
        timing.frames_received = timing.frames_received.saturating_add(1);

        let Some(timestamp_hns) = timestamp_hns else {
            return;
        };

        if timing.first_timestamp_hns.is_none() {
            timing.first_timestamp_hns = Some(timestamp_hns);
        }
        if let Some(previous) = timing.last_timestamp_hns {
            let gap = timestamp_hns.saturating_sub(previous).max(0);
            timing.largest_frame_gap_hns = timing.largest_frame_gap_hns.max(gap);
        }
        timing.last_timestamp_hns = Some(timestamp_hns);

        if timing.frame_interval_hns <= 0 {
            return;
        }

        if let Some(next) = timing.next_frame_hns {
            if timestamp_hns < next {
                timing.frames_rate_limited = timing.frames_rate_limited.saturating_add(1);
                return;
            }

            let mut following = next;
            while following <= timestamp_hns {
                following = following.saturating_add(timing.frame_interval_hns);
            }
            timing.next_frame_hns = Some(following);
        } else {
            timing.next_frame_hns = Some(timestamp_hns.saturating_add(timing.frame_interval_hns));
        }
    }

    fn snapshot(&self) -> CaptureDeliverySnapshot {
        let timing = self.timing.lock();
        let timestamp_span_hns = match (timing.first_timestamp_hns, timing.last_timestamp_hns) {
            (Some(first), Some(last)) => last.saturating_sub(first).max(0),
            _ => 0,
        };
        let timestamp_span_seconds = timestamp_span_hns as f64 / 10_000_000.0;
        let capture_fps = if timing.frames_received > 1 && timestamp_span_seconds > 0.0 {
            (timing.frames_received - 1) as f64 / timestamp_span_seconds
        } else {
            0.0
        };

        CaptureDeliverySnapshot {
            frames_received: timing.frames_received,
            frames_rate_limited: timing.frames_rate_limited,
            capture_fps,
            largest_frame_gap_ms: timing.largest_frame_gap_hns as f64 / 10_000.0,
        }
    }
}

thread_local! {
    static ACTIVE_CAPTURE_DELIVERY: RefCell<Option<Arc<CaptureDeliveryHealth>>> = RefCell::new(None);
}

fn active_capture_delivery() -> Option<Arc<CaptureDeliveryHealth>> {
    ACTIVE_CAPTURE_DELIVERY.with(|slot| slot.borrow().clone())
}

pub mod capture {
    use super::{ACTIVE_CAPTURE_DELIVERY, CaptureDeliveryHealth};
    use parking_lot::Mutex;
    use std::sync::{Arc, atomic::AtomicBool};
    use std::thread::JoinHandle;

    use windows_capture_core::capture::GraphicsCaptureApiHandler as CoreGraphicsCaptureApiHandler;
    use windows_capture_core::frame::Frame;
    use windows_capture_core::graphics_capture_api::InternalCaptureControl;
    use windows_capture_core::settings::{GraphicsCaptureItemType, Settings};

    pub use windows_capture_core::capture::{CaptureControlError, GraphicsCaptureApiError};
    pub type Context<Flags> = windows_capture_core::capture::Context<Flags>;

    struct InstrumentedHandler<T: GraphicsCaptureApiHandler> {
        inner: Arc<Mutex<T>>,
        delivery: Arc<CaptureDeliveryHealth>,
    }

    impl<T> CoreGraphicsCaptureApiHandler for InstrumentedHandler<T>
    where
        T: GraphicsCaptureApiHandler + Send + 'static,
        T::Flags: Send,
    {
        type Flags = T::Flags;
        type Error = T::Error;

        fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
            let delivery = Arc::new(CaptureDeliveryHealth::default());
            ACTIVE_CAPTURE_DELIVERY.with(|slot| {
                *slot.borrow_mut() = Some(delivery.clone());
            });

            match T::new(ctx) {
                Ok(inner) => Ok(Self {
                    inner: Arc::new(Mutex::new(inner)),
                    delivery,
                }),
                Err(error) => {
                    ACTIVE_CAPTURE_DELIVERY.with(|slot| {
                        slot.borrow_mut().take();
                    });
                    Err(error)
                }
            }
        }

        fn on_frame_arrived(
            &mut self,
            frame: &mut Frame,
            capture_control: InternalCaptureControl,
        ) -> Result<(), Self::Error> {
            let timestamp = frame.timestamp().ok().map(|value| value.Duration);
            self.delivery.record_received(timestamp);
            self.inner.lock().on_frame_arrived(frame, capture_control)
        }

        fn on_closed(&mut self) -> Result<(), Self::Error> {
            self.inner.lock().on_closed()
        }
    }

    impl<T: GraphicsCaptureApiHandler> Drop for InstrumentedHandler<T> {
        fn drop(&mut self) {
            ACTIVE_CAPTURE_DELIVERY.with(|slot| {
                let should_clear = slot
                    .borrow()
                    .as_ref()
                    .map(|active| Arc::ptr_eq(active, &self.delivery))
                    .unwrap_or(false);
                if should_clear {
                    slot.borrow_mut().take();
                }
            });
        }
    }

    pub struct CaptureControl<T, E>
    where
        T: GraphicsCaptureApiHandler<Error = E> + Send + 'static,
        T::Flags: Send,
        E: Send + Sync,
    {
        inner: windows_capture_core::capture::CaptureControl<InstrumentedHandler<T>, E>,
        callback: Arc<Mutex<T>>,
    }

    impl<T, E> CaptureControl<T, E>
    where
        T: GraphicsCaptureApiHandler<Error = E> + Send + 'static,
        T::Flags: Send,
        E: Send + Sync,
    {
        #[must_use]
        pub fn is_finished(&self) -> bool {
            self.inner.is_finished()
        }

        #[must_use]
        pub fn into_thread_handle(self) -> JoinHandle<Result<(), GraphicsCaptureApiError<E>>> {
            self.inner.into_thread_handle()
        }

        #[must_use]
        pub fn halt_handle(&self) -> Arc<AtomicBool> {
            self.inner.halt_handle()
        }

        #[must_use]
        pub fn callback(&self) -> Arc<Mutex<T>> {
            self.callback.clone()
        }

        pub fn wait(self) -> Result<(), CaptureControlError<E>> {
            self.inner.wait()
        }

        pub fn stop(self) -> Result<(), CaptureControlError<E>> {
            self.inner.stop()
        }
    }

    pub trait GraphicsCaptureApiHandler: Sized {
        type Flags;
        type Error: Send + Sync;

        fn start<I: TryInto<GraphicsCaptureItemType>>(
            settings: Settings<Self::Flags, I>,
        ) -> Result<(), GraphicsCaptureApiError<Self::Error>>
        where
            Self: Send + 'static,
            Self::Flags: Send,
        {
            <InstrumentedHandler<Self> as CoreGraphicsCaptureApiHandler>::start(settings)
        }

        fn start_free_threaded<I: TryInto<GraphicsCaptureItemType> + Send + 'static>(
            settings: Settings<Self::Flags, I>,
        ) -> Result<CaptureControl<Self, Self::Error>, GraphicsCaptureApiError<Self::Error>>
        where
            Self: Send + 'static,
            Self::Flags: Send,
        {
            let inner =
                <InstrumentedHandler<Self> as CoreGraphicsCaptureApiHandler>::start_free_threaded(
                    settings,
                )?;
            let callback = {
                let adapter = inner.callback();
                adapter.lock().inner.clone()
            };
            Ok(CaptureControl { inner, callback })
        }

        fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error>;

        fn on_frame_arrived(
            &mut self,
            frame: &mut Frame,
            capture_control: InternalCaptureControl,
        ) -> Result<(), Self::Error>;

        fn on_closed(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }
    }
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
    use std::sync::Arc;
    use std::time::Instant;

    use super::frame::Frame;
    use super::{CaptureDeliveryHealth, CaptureDeliverySnapshot, active_capture_delivery};

    pub use windows_capture_core::encoder::{
        AudioSettingsBuilder, ContainerSettingsBuilder, ImageEncoder, ImageEncoderPixelFormat,
        ImageFormat, VideoEncoderError, VideoSettingsSubType,
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
        delivery: Option<Arc<CaptureDeliveryHealth>>,
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
        fn new(
            output_path: PathBuf,
            target_fps: u32,
            delivery: Option<Arc<CaptureDeliveryHealth>>,
        ) -> Self {
            let suppress_log = output_path
                .file_name()
                .and_then(|value| value.to_str())
                .map(|value| value.starts_with("recorder-native-warmup-"))
                .unwrap_or(false);

            Self {
                output_path,
                started_at: Instant::now(),
                target_fps: target_fps.max(1),
                delivery,
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

            let delivery = self
                .delivery
                .as_ref()
                .map(|health| health.snapshot())
                .unwrap_or_else(CaptureDeliverySnapshot::default);
            let expected_encoder_attempts = delivery
                .frames_received
                .saturating_sub(delivery.frames_rate_limited);
            let actual_encoder_attempts = self.video_submitted.saturating_add(self.video_failed);
            let processing_deficit =
                expected_encoder_attempts.saturating_sub(actual_encoder_attempts);

            eprintln!(
                "[Recorder][StreamHealth] finalize_ok={} wall_ms={} active_wall_ms={} target_fps={} frames_received={} frames_rate_limited={} frames_submitted={} expected_frames={} frame_deficit={} timeline_coverage_pct={:.1} processing_deficit={} frame_failures={} capture_fps={:.2} effective_fps={:.2} max_capture_gap_ms={:.1} max_frame_gap_ms={:.1} audio_buffers={} audio_failures={} audio_bytes={} path={}",
                finalize_ok,
                self.started_at.elapsed().as_millis(),
                active_wall_ms,
                self.target_fps,
                delivery.frames_received,
                delivery.frames_rate_limited,
                self.video_submitted,
                expected_frames,
                frame_deficit,
                coverage_percent,
                processing_deficit,
                self.video_failed,
                delivery.capture_fps,
                effective_fps,
                delivery.largest_frame_gap_ms,
                largest_frame_gap_ms,
                self.audio_buffers_submitted,
                self.audio_failed,
                self.audio_bytes_submitted,
                self.output_path.display(),
            );

            if delivery.frames_received == 0 {
                eprintln!(
                    "[Recorder][StreamHealth] warning=no_wgc_frames_received path={}",
                    self.output_path.display()
                );
            }
            if self.video_submitted == 0 {
                eprintln!(
                    "[Recorder][StreamHealth] warning=no_video_frames_submitted path={}",
                    self.output_path.display()
                );
            }
            if delivery.largest_frame_gap_ms > 1_000.0 {
                eprintln!(
                    "[Recorder][StreamHealth] warning=large_capture_gap gap_ms={:.1} path={}",
                    delivery.largest_frame_gap_ms,
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
            if processing_deficit > 0 {
                eprintln!(
                    "[Recorder][StreamHealth] warning=capture_processing_deficit expected_attempts={} actual_attempts={} deficit={} path={}",
                    expected_encoder_attempts,
                    actual_encoder_attempts,
                    processing_deficit,
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
            let delivery = active_capture_delivery();
            if let Some(health) = delivery.as_ref() {
                health.configure_target_fps(target_fps);
            }
            let inner = windows_capture_core::encoder::VideoEncoder::new(
                video_settings,
                audio_settings,
                container_settings,
                &output_path,
            )?;
            Ok(Self {
                inner,
                health: EncoderSubmissionHealth::new(output_path, target_fps, delivery),
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
