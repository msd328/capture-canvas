# Camera health diagnostics batch — 2026-07-29

## Roadmap movement

| ID | Previous | Current | Validation still required |
|---|---:|---:|---|
| CAM-07 | 🔵 | 🟡 | Windows build plus native-camera and fallback-camera recordings |
| HLT-11 | 🔵 | 🟡 | Confirm one `CameraHealth` line per camera-enabled segment |

`AUD-08`, `AUD-09`, and `HLT-12` remain 🔵 and are the next diagnostics sub-batch.
A code commit does not move these items to ✅.

## Implementation commits

| Commit | Change |
|---|---|
| `857c123` | Added process-local per-segment camera diagnostics state |
| `8082f91` | Exposed diagnostics from the local `windows_capture` facade |
| `b5deefb` | Correlated successful encoded frames with active camera overlays |
| `7c0ae5e` | Counted native/fallback camera frames, source misses, errors, and source stop |
| `f72a6b4` | Updated the master roadmap; CAM-07 and HLT-11 → 🟡 |

## Health output

A camera-enabled segment should emit:

```text
[Recorder][CameraHealth]
backend=native-mediacapture|ffmpeg-dshow
finalize_ok=true|false
source_stopped=true|false
wall_ms=...
frames_received=...
overlay_frames=...
acquire_misses=...
source_errors=...
receive_fps=...
max_receive_gap_ms=...
```

Warnings are emitted for no received frames, camera source errors, and receive gaps over one second.

`overlay_frames` is the number of successfully submitted encoded screen frames after the first usable camera frame. Both current recorder backends reuse the latest camera BGRA frame for each such submission. A very small output that intentionally skips the overlay can make this counter an overestimate; that case remains part of runtime validation.

## Validation checklist

1. `bun run desktop:dev` compiles without Rust or Tauri configuration errors.
2. A camera-disabled recording emits no `CameraHealth` line.
3. A native-camera recording emits exactly one `CameraHealth` line after Stop.
4. `backend=native-mediacapture` appears when MediaCapture succeeds.
5. `frames_received` and `overlay_frames` are both greater than zero.
6. `source_errors=0` during a normal recording.
7. `receive_fps` is plausible for the selected camera, normally near 30 FPS.
8. Camera overlay remains visible and stable in the output.
9. Pause/Resume emits one camera-health line per active segment.
10. If a camera requires fallback, `backend=ffmpeg-dshow` and the same counters appear.
11. Existing `StreamHealth` and security/path behaviour remain unchanged.

## Security impact

```text
Security impact:
Added local operational counters for camera delivery and encoder-overlay submissions.
No captured pixels, device identifiers, local paths, or recording content are logged.

Data accessed:
Camera frame arrival events, encoder submission outcomes, monotonic timing, and backend label.

Data written:
Local stderr health logs and repository tracking documents only.

Network communication added:
None.

New permissions/capabilities:
None.

External processes:
None added. The pre-existing FFmpeg/DirectShow camera compatibility process remains.

Untrusted inputs:
Camera frames and camera-source errors from Windows MediaCapture or FFmpeg/DirectShow.

Validation added:
Per-segment received-frame, source-miss, source-error, overlay-submission, FPS, and gap counters.

Secrets involved:
None.

Security tests completed:
Static diff review only. Windows compilation and runtime validation remain pending.

Remaining risks:
Counters are process-local and assume the recorder's current single-active-session model.
Overlay submissions can overestimate visible overlays for outputs too small to draw the camera.
Captured media remains unencrypted at rest, and the FFmpeg fallback trust issue remains SEC-07.
```
