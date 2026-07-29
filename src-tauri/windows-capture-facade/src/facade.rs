// Load the instrumentation implementation as a normal Rust module. Using a
// module path keeps the `//!` comments at the top of lib.rs valid as inner
// module documentation, while the crate's Rust 2024 edition retains the
// callback-guard temporary lifetime fix.
pub mod diagnostics;

#[allow(unused_imports)]
#[path = "lib.rs"]
mod implementation;

pub use implementation::{capture, d3d11, frame, graphics_capture_api, monitor, settings, window};

pub mod encoder {
    use std::path::Path;

    use crate::diagnostics;
    use crate::frame::Frame;

    pub use crate::implementation::encoder::{
        AudioSettingsBuilder, ContainerSettingsBuilder, ImageEncoder,
        ImageEncoderPixelFormat, ImageFormat, VideoEncoderError, VideoSettingsBuilder,
        VideoSettingsSubType,
    };

    /// Final facade layer that preserves the existing encoder health wrapper while
    /// correlating successful encoded frames with an active camera source.
    pub struct VideoEncoder {
        inner: crate::implementation::encoder::VideoEncoder,
    }

    impl VideoEncoder {
        pub fn new<P: AsRef<Path>>(
            video_settings: VideoSettingsBuilder,
            audio_settings: AudioSettingsBuilder,
            container_settings: ContainerSettingsBuilder,
            path: P,
        ) -> Result<Self, VideoEncoderError> {
            crate::implementation::encoder::VideoEncoder::new(
                video_settings,
                audio_settings,
                container_settings,
                path,
            )
            .map(|inner| Self { inner })
        }

        pub fn send_frame(&mut self, frame: &Frame) -> Result<(), VideoEncoderError> {
            let result = self.inner.send_frame(frame);
            if result.is_ok() {
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
                diagnostics::record_camera_overlay_submission();
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
            let result = self.inner.finish();
            diagnostics::finish_camera_segment(result.is_ok());
            result
        }
    }
}
