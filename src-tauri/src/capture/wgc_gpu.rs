use crate::recording::types::{CaptureKind, CaptureTarget, RecordingConfig};
use anyhow::{anyhow, Result};
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use windows_capture::capture::{CaptureControl, Context as CaptureContext, GraphicsCaptureApiHandler};
use windows_capture::encoder::{
    AudioSettingsBuilder, ContainerSettingsBuilder, VideoEncoder, VideoSettingsBuilder,
    VideoSettingsSubType,
};
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    GraphicsCaptureItemType, MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};
use windows_capture::window::Window;

#[derive(Clone)]
struct GpuFlags {
    output_path: PathBuf,
    width: u32,
    height: u32,
    fps: u32,
}

struct GpuFrameEncoder {
    encoder: Option<VideoEncoder>,
    frame_interval_hns: i64,
    next_frame_hns: Option<i64>,
}

impl GpuFrameEncoder {
    fn finish(&mut self) -> Result<(), String> {
        let Some(encoder) = self.encoder.take() else {
            return Ok(());
        };
        encoder
            .finish()
            .map_err(|error| format!("Unable to finalize native Windows H.264 encoder: {error}"))
    }
}

impl GraphicsCaptureApiHandler for GpuFrameEncoder {
    type Flags = GpuFlags;
    type Error = String;

    fn new(ctx: CaptureContext<Self::Flags>) -> Result<Self, Self::Error> {
        let fps = ctx.flags.fps.clamp(1, 60);
        let pixels = u64::from(ctx.flags.width) * u64::from(ctx.flags.height);
        let mut bitrate = if pixels <= 1920 * 1200 {
            12_000_000
        } else if pixels <= 2560 * 1440 {
            20_000_000
        } else {
            35_000_000
        };
        if fps > 30 {
            bitrate = bitrate.saturating_mul(3) / 2;
        }

        let encoder = VideoEncoder::new(
            VideoSettingsBuilder::new(ctx.flags.width, ctx.flags.height)
                .sub_type(VideoSettingsSubType::H264)
                .bitrate(bitrate)
                .frame_rate(fps),
            AudioSettingsBuilder::new().disabled(true),
            ContainerSettingsBuilder::new(),
            &ctx.flags.output_path,
        )
        .map_err(|error| format!("Unable to initialize native Windows H.264 encoder: {error}"))?;

        Ok(Self {
            encoder: Some(encoder),
            frame_interval_hns: (10_000_000i64 / i64::from(fps)).max(1),
            next_frame_hns: None,
        })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        _capture_control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        let timestamp = frame
            .timestamp()
            .map_err(|error| format!("Unable to read WGC frame timestamp: {error}"))?
            .Duration;

        // WGC can deliver at the desktop refresh rate. Decimate before the encoder
        // so a 120/144 Hz display does not create an unbounded queue for a 30/60 fps recording.
        if let Some(next) = self.next_frame_hns {
            if timestamp < next {
                return Ok(());
            }
            let mut following = next;
            while following <= timestamp {
                following = following.saturating_add(self.frame_interval_hns);
            }
            self.next_frame_hns = Some(following);
        } else {
            self.next_frame_hns = Some(timestamp.saturating_add(self.frame_interval_hns));
        }

        self.encoder
            .as_mut()
            .ok_or_else(|| "Native Windows encoder is already finalized".to_string())?
            .send_frame(frame)
            .map_err(|error| format!("Unable to submit D3D11 frame to native Windows encoder: {error}"))
    }

    fn on_closed(&mut self) -> Result<(), Self::Error> {
        self.finish()
    }
}

pub struct NativeGpuVideoCapture {
    control: Option<CaptureControl<GpuFrameEncoder, String>>,
}

impl NativeGpuVideoCapture {
    pub fn stop(mut self) -> Result<()> {
        let Some(control) = self.control.take() else {
            return Ok(());
        };

        // Keep a handle to the encoder state, stop WGC so no more frames can arrive,
        // then flush/finalize the MediaTranscoder-backed MP4 encoder.
        let callback = control.callback();
        let capture_result = control
            .stop()
            .map_err(|error| anyhow!("Unable to stop Windows Graphics Capture: {error}"));
        let finish_result = callback.lock().finish().map_err(anyhow::Error::msg);

        capture_result?;
        finish_result?;
        Ok(())
    }
}

fn start_item<T>(
    item: T,
    config: &RecordingConfig,
    width: u32,
    height: u32,
    output_path: &Path,
) -> Result<NativeGpuVideoCapture>
where
    T: TryInto<GraphicsCaptureItemType> + Send + 'static,
{
    let fps = config.fps.clamp(1, 60);
    let settings = Settings::new(
        item,
        CursorCaptureSettings::Default,
        DrawBorderSettings::Default,
        SecondaryWindowSettings::Default,
        MinimumUpdateIntervalSettings::Default,
        DirtyRegionSettings::Default,
        ColorFormat::Bgra8,
        GpuFlags {
            output_path: output_path.to_path_buf(),
            width,
            height,
            fps,
        },
    );

    let control = GpuFrameEncoder::start_free_threaded(settings)
        .map_err(|error| anyhow!("Unable to start native WGC/D3D11 encoder: {error}"))?;

    Ok(NativeGpuVideoCapture {
        control: Some(control),
    })
}

pub fn start_native_gpu_video_capture(
    config: &RecordingConfig,
    target: &CaptureTarget,
    width: u32,
    height: u32,
    output_path: &Path,
) -> Result<NativeGpuVideoCapture> {
    match target.kind {
        CaptureKind::Display => {
            let raw = target
                .id
                .strip_prefix("monitor-")
                .ok_or_else(|| anyhow!("Invalid monitor id"))?;
            let handle = usize::from_str_radix(raw, 16).map_err(|_| anyhow!("Invalid monitor id"))?;
            let monitor = Monitor::from_raw_hmonitor(handle as *mut c_void);
            start_item(monitor, config, width, height, output_path)
        }
        CaptureKind::Window => {
            let raw = target
                .id
                .strip_prefix("window-")
                .ok_or_else(|| anyhow!("Invalid window id"))?;
            let handle = usize::from_str_radix(raw, 16).map_err(|_| anyhow!("Invalid window id"))?;
            let window = Window::from_raw_hwnd(handle as *mut c_void);
            if !window.is_valid() {
                return Err(anyhow!("Selected window is no longer capturable"));
            }
            start_item(window, config, width, height, output_path)
        }
    }
}
