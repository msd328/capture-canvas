# Recorder — Tauri backend

This directory holds the Rust/Tauri backend for the Recorder desktop app.
It is **not** built by Lovable's web preview — the preview runs the React
UI against a mock `desktop` service (`src/services/mock-desktop.ts`).

## Build locally

```bash
# Prereqs: Rust (rustup), Tauri prerequisites for your OS, bun/npm.
cd apps/desktop        # or the project root, depending on your layout
bun install
cargo install tauri-cli --version "^2.0"
cargo tauri dev
```

## Layout

```
src-tauri/src/
  main.rs              — Tauri app entry, command registration
  commands/            — Thin wrappers over the recording engine (see below)
  recording/           — RecordingEngine, RecordingConfig, RecordingOutput
  capture/             — Screen/window capture (per-OS backends)
  audio/               — Microphone + system audio capture
  camera/              — Webcam capture
  encoding/            — H.264/AAC encoding, MP4 muxing (FFmpeg)
  state/               — App state, settings persistence, recordings index
```

## Command contract (must match `src/services/desktop.ts`)

Every command below is expected by the frontend:

| Command | Args | Returns |
| --- | --- | --- |
| `list_displays` | — | `Vec<DisplayInfo>` |
| `list_windows` | — | `Vec<WindowInfo>` |
| `list_microphones` | — | `Vec<MicrophoneInfo>` |
| `list_cameras` | — | `Vec<CameraInfo>` |
| `start_recording` | `config: RecordingConfig` | `{ id: String }` |
| `pause_recording` | — | — |
| `resume_recording` | — | — |
| `stop_recording` | — | `RecordingOutput` |
| `get_recordings` | — | `Vec<RecordingOutput>` |
| `get_recording` | `id: String` | `Option<RecordingOutput>` |
| `delete_recording` | `id: String` | — |
| `rename_recording` | `id: String, title: String` | `RecordingOutput` |
| `open_recording_location` | `id: String` | — |
| `get_settings` | — | `RecorderSettings` |
| `update_settings` | `settings: RecorderSettings` | `RecorderSettings` |

Events emitted from Rust → JS:
- `mic-level` — `f32` in `[0.0, 1.0]`, ~30/s while recording setup or preview is open.
- `recording-status` — one of `preparing | countdown | recording | paused | stopping | error`.

## Implementation notes

- **Screen capture**: macOS → ScreenCaptureKit (macOS 12.3+); Windows → Windows.Graphics.Capture / DXGI Desktop Duplication; Linux → PipeWire + xdg-desktop-portal.
- **Microphone**: `cpal` for cross-platform PCM capture.
- **System audio**: macOS 13+ ScreenCaptureKit provides audio; earlier macOS requires a virtual audio driver (out of scope for Phase 1 — surface a friendly message). Windows: WASAPI loopback. Linux: PipeWire.
- **Camera**: `nokhwa` crate or per-OS APIs.
- **Encoding / muxing**: FFmpeg via `ffmpeg-next` or by spawning an `ffmpeg` sidecar bundled with the app. Target H.264 (baseline/main) + AAC in an MP4 container. 30 FPS default.
- **Window exclusion (recording controls not in capture)**: macOS `NSWindow.sharingType = .none`; Windows `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)`.
- **Permissions**: request TCC screen recording, microphone, camera on macOS; capture permission on Windows via the picker; portal permissions on Linux.
- **Local persistence**: JSON index file under the app data dir (`tauri::path::BaseDirectory::AppData`), plus MP4s in the user-chosen output directory (default: `~/Recordings`).

The frontend never calls these commands directly — always through
`src/services/desktop.ts`, which selects the Tauri transport at runtime.
