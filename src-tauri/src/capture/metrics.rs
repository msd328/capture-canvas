use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy)]
pub enum CaptureBackend {
    NativeGpu,
    FfmpegFallback,
}

impl CaptureBackend {
    const fn label(self) -> &'static str {
        match self {
            Self::NativeGpu => "native-wgc-d3d11",
            Self::FfmpegFallback => "wgc-ffmpeg-fallback",
        }
    }
}

pub struct CaptureSessionMetrics {
    backend: CaptureBackend,
    output_path: PathBuf,
    started_at: Instant,
    start_latency: Duration,
    width: u32,
    height: u32,
    fps: u32,
    camera: bool,
    microphone: bool,
    system_audio: bool,
}

impl CaptureSessionMetrics {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        backend: CaptureBackend,
        output_path: &Path,
        start_latency: Duration,
        width: u32,
        height: u32,
        fps: u32,
        camera: bool,
        microphone: bool,
        system_audio: bool,
    ) -> Self {
        Self {
            backend,
            output_path: output_path.to_path_buf(),
            started_at: Instant::now(),
            start_latency,
            width,
            height,
            fps,
            camera,
            microphone,
            system_audio,
        }
    }

    pub fn log_started(&self) {
        eprintln!(
            "[Recorder][Health] backend={} start_ms={} video={}x{}@{} camera={} mic={} system_audio={}",
            self.backend.label(),
            self.start_latency.as_millis(),
            self.width,
            self.height,
            self.fps,
            self.camera,
            self.microphone,
            self.system_audio,
        );

        if self.start_latency > Duration::from_millis(1_500) {
            eprintln!(
                "[Recorder][Health] warning=slow_capture_start backend={} start_ms={}",
                self.backend.label(),
                self.start_latency.as_millis(),
            );
        }
    }

    pub fn log_stopped(self, stop_latency: Duration, succeeded: bool) {
        let wall = self.started_at.elapsed();
        let bytes = std::fs::metadata(&self.output_path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        let seconds = wall.as_secs_f64();
        let average_mbps = if seconds > 0.0 {
            bytes as f64 * 8.0 / seconds / 1_000_000.0
        } else {
            0.0
        };

        eprintln!(
            "[Recorder][Health] backend={} stop_ok={} stop_ms={} wall_ms={} bytes={} avg_mbps={:.2}",
            self.backend.label(),
            succeeded,
            stop_latency.as_millis(),
            wall.as_millis(),
            bytes,
            average_mbps,
        );

        if stop_latency > Duration::from_millis(2_000) {
            eprintln!(
                "[Recorder][Health] warning=slow_capture_stop backend={} stop_ms={}",
                self.backend.label(),
                stop_latency.as_millis(),
            );
        }
        if succeeded && bytes == 0 {
            eprintln!(
                "[Recorder][Health] warning=empty_capture_output backend={} path={}",
                self.backend.label(),
                self.output_path.display(),
            );
        }
    }
}
