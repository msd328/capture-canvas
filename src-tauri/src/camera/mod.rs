//! Webcam capture. Provides frames to the encoder and (optionally) a
//! low-latency preview stream to the frontend for the setup screen.

use crate::recording::types::CameraInfo;

pub fn enumerate_cameras() -> Vec<CameraInfo> {
    // TODO(native): enumerate via `nokhwa` or per-OS APIs.
    vec![
        CameraInfo { id: "cam-facetime".into(), name: "FaceTime HD Camera".into(), is_default: true },
    ]
}
