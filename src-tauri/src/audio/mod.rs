//! Audio capture — microphone and (optionally) system audio.
//!
//! Microphone: cross-platform via `cpal`.
//! System audio: macOS 13+ ScreenCaptureKit audio; Windows WASAPI loopback;
//! Linux PipeWire. Older macOS returns an "unsupported" error surfaced to
//! the UI as `SystemAudioUnsupported`.

use crate::recording::types::MicrophoneInfo;

pub fn enumerate_microphones() -> Vec<MicrophoneInfo> {
    // TODO(native): use cpal::default_host().input_devices().
    vec![
        MicrophoneInfo { id: "mic-default".into(), name: "System Default".into(), is_default: true },
        MicrophoneInfo { id: "mic-builtin".into(), name: "MacBook Pro Microphone".into(), is_default: false },
    ]
}

/// Returns true when system audio capture is implemented for the current OS.
pub fn system_audio_supported() -> bool {
    // TODO(native): detect per-OS availability at runtime.
    cfg!(target_os = "windows")
}
