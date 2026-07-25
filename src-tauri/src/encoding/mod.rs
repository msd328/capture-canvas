//! Encoding + muxing.
//!
//! Target: H.264 (Main / Level 4.1) video + AAC-LC audio, in an MP4
//! container. 30 FPS default (see RecordingConfig.fps).
//!
//! Implementation options:
//!   * `ffmpeg-next` crate linked against a bundled FFmpeg static build.
//!   * `ffmpeg` binary bundled as a Tauri sidecar and driven over stdin.
//!
//! Whichever path we choose, the surface exposed here should be:
//!
//! ```ignore
//! pub struct Encoder { /* ... */ }
//! impl Encoder {
//!     pub fn open(path: &Path, config: &RecordingConfig) -> Result<Self>;
//!     pub fn push_video_frame(&mut self, frame: VideoFrame) -> Result<()>;
//!     pub fn push_audio_frame(&mut self, frame: AudioFrame) -> Result<()>;
//!     pub fn finalize(self) -> Result<EncodedOutput>;
//! }
//! ```
//!
//! For Phase 1 this file is intentionally empty aside from the contract
//! above.
