use crate::recording::types::{CaptureKind, CapturePreview, CaptureTarget};
use anyhow::{anyhow, Result};
use base64::Engine;
use parking_lot::Mutex;
use std::ffi::c_void;
use std::sync::Arc;
use windows_capture::capture::{Context as CaptureContext, GraphicsCaptureApiHandler};
use windows_capture::encoder::{ImageEncoder, ImageEncoderPixelFormat, ImageFormat};
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    GraphicsCaptureItemType, MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};
use windows_capture::window::Window;

struct PreviewFrame {
    jpeg: Vec<u8>,
    width: u32,
    height: u32,
}

type PreviewResult = Arc<Mutex<Option<Result<PreviewFrame, String>>>>;

#[derive(Clone)]
struct PreviewFlags {
    result: PreviewResult,
}

struct PreviewCapture {
    result: PreviewResult,
}

impl GraphicsCaptureApiHandler for PreviewCapture {
    type Flags = PreviewFlags;
    type Error = String;

    fn new(ctx: CaptureContext<Self::Flags>) -> Result<Self, Self::Error> {
        Ok(Self {
            result: ctx.flags.result,
        })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        capture_control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        if self.result.lock().is_some() {
            capture_control.stop();
            return Ok(());
        }

        let encoded = (|| -> Result<PreviewFrame, String> {
            let width = frame.width();
            let height = frame.height();
            let mut scratch = Vec::new();
            let buffer = frame
                .buffer()
                .map_err(|error| format!("Unable to map source preview frame: {error}"))?;
            let pixels = buffer.as_nopadding_buffer(&mut scratch);
            let expected = width as usize * height as usize * 4;
            if pixels.len() != expected {
                return Err(format!(
                    "Unexpected source preview size: got {}, expected {expected}",
                    pixels.len()
                ));
            }

            let jpeg = ImageEncoder::new(ImageFormat::Jpeg, ImageEncoderPixelFormat::Bgra8)
                .map_err(|error| format!("Unable to initialize Windows JPEG encoder: {error}"))?
                .encode(pixels, width, height)
                .map_err(|error| format!("Unable to encode source preview: {error}"))?;

            Ok(PreviewFrame {
                jpeg,
                width,
                height,
            })
        })();

        *self.result.lock() = Some(encoded);
        capture_control.stop();
        Ok(())
    }

    fn on_closed(&mut self) -> Result<(), Self::Error> {
        let mut result = self.result.lock();
        if result.is_none() {
            *result = Some(Err(
                "The selected source closed before a preview frame was available".to_string(),
            ));
        }
        Ok(())
    }
}

fn capture_item<T>(item: T) -> Result<CapturePreview>
where
    T: TryInto<GraphicsCaptureItemType> + Send + 'static,
{
    let result: PreviewResult = Arc::new(Mutex::new(None));
    let settings = Settings::new(
        item,
        CursorCaptureSettings::Default,
        DrawBorderSettings::Default,
        SecondaryWindowSettings::Default,
        MinimumUpdateIntervalSettings::Default,
        DirtyRegionSettings::Default,
        ColorFormat::Bgra8,
        PreviewFlags {
            result: result.clone(),
        },
    );

    PreviewCapture::start(settings)
        .map_err(|error| anyhow!("Unable to capture source preview: {error}"))?;

    let frame = result
        .lock()
        .take()
        .ok_or_else(|| anyhow!("Windows capture ended without producing a preview frame"))?
        .map_err(anyhow::Error::msg)?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(frame.jpeg);

    Ok(CapturePreview {
        data_url: format!("data:image/jpeg;base64,{encoded}"),
        width: frame.width,
        height: frame.height,
    })
}

pub fn capture_source_preview(target: &CaptureTarget) -> Result<CapturePreview> {
    match target.kind {
        CaptureKind::Display => {
            let raw = target
                .id
                .strip_prefix("monitor-")
                .ok_or_else(|| anyhow!("Invalid monitor id"))?;
            let handle =
                usize::from_str_radix(raw, 16).map_err(|_| anyhow!("Invalid monitor id"))?;
            capture_item(Monitor::from_raw_hmonitor(handle as *mut c_void))
        }
        CaptureKind::Window => {
            let raw = target
                .id
                .strip_prefix("window-")
                .ok_or_else(|| anyhow!("Invalid window id"))?;
            let handle =
                usize::from_str_radix(raw, 16).map_err(|_| anyhow!("Invalid window id"))?;
            let window = Window::from_raw_hwnd(handle as *mut c_void);
            if !window.is_valid() {
                return Err(anyhow!("Selected window is no longer capturable"));
            }
            capture_item(window)
        }
    }
}
