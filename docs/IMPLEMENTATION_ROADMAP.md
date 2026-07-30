# Capture Canvas implementation roadmap

This document is the source of truth for recorder implementation status on
`feature/windows-native-capture`.

## Update policy

Every implementation batch must update this file in the same commit series as the
code change. Status is based on evidence, not intent:

- ✅ **Validated** — implemented and verified by a successful build, runtime test, security test, or CI evidence appropriate to the item.
- 🟡 **Implemented, validation pending** — code or policy is present, but its current build/runtime/security behaviour has not yet been confirmed.
- 🔵 **Next** — selected for the next implementation batches.
- ⚪ **Planned** — accepted roadmap work, not currently being implemented.
- ⛔ **Postponed** — deliberately deferred until a prerequisite is complete.

A pushed commit alone does not move an item to ✅. Compiler output, runtime logs, CI,
negative tests, or documented review evidence must provide validation.

Every implementation update must also include the security-impact block defined in
`docs/SECURITY_BASELINE.md`, explicitly using `None` for fields that do not apply.

## Current focus

1. Rebuild the latest branch and confirm one path-free `CleanupHealth` startup line.
2. Run the stale/recent/final-file cleanup matrix and validate that only 24-hour-old recorder-owned temporary files are deleted.
3. Repeat Pause/Resume and confirm normal `FinalizerHealth` output plus successful candidate publication.
4. Run one single-segment recording and capture `ThumbnailHealth` to identify the native thumbnail failure stage or confirm success.
5. Evaluate safe bounds for WinRT open/decode and FFmpeg fallback phases after cleanup validation.
6. Validate CSP, restricted asset playback, the main-window capability, and plugin removal on Windows.
7. Run the 60-second mostly-static full-source and selected-area duration matrix.
8. Validate camera, submitted A/V drift, and audio-mixer health with camera + microphone + system audio.
9. Remove the remaining FFmpeg compatibility paths.

---

## Capture sources

| ID | Status | Work | Acceptance evidence |
|---|---:|---|---|
| CAP-01 | ✅ | Enumerate real Windows displays | Real multi-monitor list observed |
| CAP-02 | ✅ | Enumerate capturable application windows | Real top-level windows observed |
| CAP-03 | ✅ | Filter hidden, minimized, DWM-cloaked, tool, child, and Recorder-owned windows | Invalid/background entries removed |
| CAP-04 | ✅ | Refresh application-window list | Open/closed apps update without restart |
| CAP-05 | ✅ | Full-display recording | Playable MP4 produced |
| CAP-06 | ✅ | Full-application-window recording | Playable MP4 produced |
| CAP-07 | ✅ | Display/window source preview | Preview displayed before recording |
| CAP-08 | ✅ | Drag-to-select partial recording area | Visual crop selection works |
| CAP-09 | 🟡 | Native selected-area H.264 encoding | Windows compile and full A/V runtime matrix pending |
| CAP-10 | ✅ | Cursor capture | Cursor visible in output |
| CAP-11 | ✅ | 30/60 FPS configuration | Both options selectable and recorded |
| CAP-12 | ✅ | Multi-monitor coordinate and DPI handling | Three-display setup enumerated correctly |

## Video and encoding

| ID | Status | Work | Acceptance evidence |
|---|---:|---|---|
| VID-01 | ✅ | Windows Graphics Capture input | WGC recording works |
| VID-02 | ✅ | Native Windows H.264 encoding | Native MP4 output works |
| VID-03 | ✅ | Direct D3D11 surface encoding for full-source capture | Native GPU backend active |
| VID-04 | 🟡 | Native BGRA buffer encoding for selected-area capture | Windows validation pending |
| VID-05 | ✅ | Resolution-dependent bitrate selection | Health logs show expected output bitrate range |
| VID-06 | ✅ | FPS limiting on high-refresh displays | Requested FPS is bounded |
| VID-07 | 🟡 | Background H.264/AAC encoder warm-up | Warm-up completed successfully in 1949 ms; before/after Start-latency comparison remains |
| VID-08 | ✅ | Encoder submission counters | Windows run emitted 516 and 862 submitted frames with zero submission failures |
| VID-09 | ✅ | WGC received/skipped/encoded counters | Windows run emitted 802/1351 received, 286/489 rate-limited, and zero processing deficit |
| VID-10 | 🟡 | Static-screen duration continuity | Final unchanged tail is held to the Pause/Stop request timestamp through one throttled BGRA snapshot; 60-second Windows duration/playback validation pending |
| VID-11 | 🔵 | Pure D3D11 selected-area crop | No CPU BGRA crop copy |
| VID-12 | ⚪ | Hardware encoder capability reporting | Show selected hardware/software encoder path |
| VID-13 | ⚪ | Smaller/balanced/high-quality presets | Quality and bitrate presets validated |

## Audio

| ID | Status | Work | Acceptance evidence |
|---|---:|---|---|
| AUD-01 | ✅ | Native microphone enumeration | Real devices shown |
| AUD-02 | ✅ | Native microphone capture | Microphone audible in output |
| AUD-03 | ✅ | Native system-audio capture | Desktop audio audible in output |
| AUD-04 | ✅ | Microphone/system-audio mixer | Both sources audible together |
| AUD-05 | ✅ | 48 kHz stereo mixed output | Mixer log shows 48 kHz/2 channel output |
| AUD-06 | ✅ | Microphone gain and system-audio ducking | Voice remains prioritised |
| AUD-07 | ✅ | Mixer prebuffer and partial queue preservation | Initial/mid-stream audio loss fixed |
| AUD-08 | 🟡 | Mixer underrun counters | Separate mic/system silence-substitution counters implemented; Windows mixed-audio runtime validation pending |
| AUD-09 | 🟡 | Queue overflow/drop counters | Per-source bounded-queue dropped-frame counters and peak depths implemented; stress validation pending |
| AUD-10 | 🟡 | Submitted audio/video drift measurement | `AvHealth` reports PCM duration, video timestamp span, signed drift, startup offset and audio submission gaps; Windows runtime/playback comparison pending |
| AUD-11 | 🔵 | Long-duration drift correction | One-hour drift stays within target |
| AUD-12 | 🔵 | Device-disconnect recovery | Clear error or recovery without hang |
| AUD-13 | ⚪ | Live microphone level meter | Responsive level display |
| AUD-14 | ⚪ | Microphone test/playback screen | Record and play a test sample |
| AUD-15 | ⚪ | Optional noise suppression/automatic gain | User-controlled processing |

## Camera

| ID | Status | Work | Acceptance evidence |
|---|---:|---|---|
| CAM-01 | ✅ | Native Windows camera enumeration | Real cameras shown |
| CAM-02 | 🟡 | MediaCapture/MediaFrameReader camera input | Multi-device Windows validation pending |
| CAM-03 | ✅ | 320×180 BGRA overlay contract | Correct overlay dimensions |
| CAM-04 | ✅ | Persistent D3D11 camera texture | Per-frame texture allocation removed |
| CAM-05 | ✅ | Camera with microphone/system-audio combinations | Previously observed working combinations |
| CAM-06 | 🟡 | FFmpeg/DirectShow camera compatibility fallback | Fallback retained but distribution policy unresolved |
| CAM-07 | 🟡 | Camera received/applied/source-miss counters | `CameraHealth` implemented for native/fallback sources and both encoder paths; Windows camera runtime log pending |
| CAM-08 | 🔵 | Direct GPU camera-frame path | Avoid per-frame SoftwareBitmap CPU copy |
| CAM-09 | 🔵 | Camera disconnect/reconnect handling | No recorder hang on disconnect |
| CAM-10 | ⚪ | Camera position selector | Four-corner placement |
| CAM-11 | ⚪ | Camera size selector | Small/medium/large sizes |
| CAM-12 | ⚪ | Circular mask and border | Configurable presentation |
| CAM-13 | ⚪ | Live camera preview | Preview before Start |
| CAM-14 | ⚪ | Remove FFmpeg camera fallback | All supported cameras use native path |

## Recorder controls and finalisation

| ID | Status | Work | Acceptance evidence |
|---|---:|---|---|
| CTRL-01 | ✅ | Immediate optimistic button states | UI changes on click |
| CTRL-02 | ✅ | Starting/Pausing/Resuming/Saving states | Transitions displayed |
| CTRL-03 | ✅ | Double-click/race prevention | Conflicting operations blocked |
| CTRL-04 | ✅ | Native operations off Tauri event thread | Commands use blocking workers |
| CTRL-05 | ✅ | Reduce timer-driven React renders | Timer updates at 250 ms |
| CTRL-06 | 🟡 | Encoder warm-up for first Start | Warm-up succeeded, but Start remained about 1.3 seconds and comparison/refinement remain |
| CTRL-07 | ✅ | Pause produces independent MP4 segments | Pause/Resume architecture active |
| CTRL-08 | 🟡 | Windows MediaComposition primary finaliser | Normal-path conversion, bounded open retries, isolated hidden candidate output, per-segment checks, stage/HRESULT diagnostics and render-reason reporting implemented; Windows rerun pending |
| CTRL-09 | ✅ | FFmpeg emergency concat fallback | Runtime fallback produced the final recording after native finalisation failed |
| CTRL-10 | 🟡 | Start-stage timing breakdown | Runtime emitted Start/Resume engine totals around 1.29/1.33 seconds; internal resolve/device/audio/camera/encoder split and target improvement remain |
| CTRL-11 | 🟡 | Stop-stage timing breakdown | Native MediaComposition emits open/decode/append/render/timeout/cancel/publish timings; FFmpeg fallback phase timing remains to be separated |
| CTRL-12 | 🔵 | Pause/Resume without re-encoding | Timestamp rebasing/direct remux |
| CTRL-13 | 🟡 | Bounded native finalisation timeout | MediaComposition render is polled for 20 seconds with two-second cancellation settling through an isolated candidate; first Windows compile found an `AsyncStatus` namespace mismatch, corrected through direct `windows-future 0.2`; rebuild and open/decode/fallback bounds remain |
| CTRL-14 | ⚪ | Cancel while Starting | Safe cancellation |

## Health diagnostics

| ID | Status | Work | Acceptance evidence |
|---|---:|---|---|
| HLT-01 | ✅ | Backend name | Health log emitted |
| HLT-02 | ✅ | Encoder startup duration | `start_ms` emitted |
| HLT-03 | ✅ | Stop/finalisation duration | `stop_ms` emitted |
| HLT-04 | ✅ | Output size and average bitrate | Bytes/Mbps emitted |
| HLT-05 | ✅ | Submitted frames and failures | Runtime emitted submitted-frame totals and zero frame failures for both segments |
| HLT-06 | ✅ | Effective encoded FPS | Runtime emitted 29.64 and 29.90 FPS against a 30 FPS target |
| HLT-07 | ✅ | Largest submitted-frame gap | Runtime emitted maximum frame gaps of about 62.6 ms |
| HLT-08 | 🟡 | Audio buffers and bytes submitted | Fields compiled and emitted zero with audio disabled; audio-enabled validation remains |
| HLT-09 | ✅ | WGC frames received | Runtime emitted 802 and 1351 received frames |
| HLT-10 | ✅ | FPS-limited frames skipped | Runtime emitted 286 and 489 rate-limited frames |
| HLT-11 | 🟡 | Camera frames received/applied/source misses | Per-segment `CameraHealth` line and warnings implemented; Windows camera runtime log pending |
| HLT-12 | 🟡 | Mixer underruns and queue overflows | Per-segment `AudioMixerHealth` line and warnings implemented for mic+system mixing; Windows validation pending |
| HLT-13 | 🟡 | Submitted A/V duration drift and startup offset | Per-segment `AvHealth` line implemented; compare signed drift and playback on Windows |
| HLT-14 | ⚪ | Exportable diagnostic report | Copy/save support bundle |
| HLT-15 | ⚪ | User-friendly health summary | Non-technical UI status |
| HLT-16 | 🟡 | Variable-frame-aware timestamp timeline coverage and sample density | Runtime high-motion segments produced 99.8%/99.7% timeline coverage and 98.7%/99.4% density without false warnings; mostly-static validation remains |
| HLT-17 | ✅ | Recorder control-phase timing and failure stage | Windows runtime emitted successful Start, Pause, Resume and Stop timing lines |
| HLT-18 | 🟡 | Static-tail continuity and held-frame outcome | Runtime emitted valid continuity fields and snapshots for both segments; final tails were active so `hold_needed=false`; static hold validation remains |
| HLT-19 | 🟡 | Native finalizer stage, retry, HRESULT and render-reason diagnostics | `FinalizerHealth` implemented for destination/segment metadata, open, decode, append and render stages; Windows Pause/Resume rerun pending |
| HLT-20 | 🟡 | Native thumbnail failure-stage and retry diagnostics | `ThumbnailHealth` implemented for WinRT open/request/read stages and asynchronous backfill; Windows single-segment and fallback-output reruns pending |
| HLT-21 | 🟡 | Native render timeout, cancellation and isolated-candidate outcome | Timeout diagnostics remain implemented; first build stopped at E0432 before runtime, and the matching `windows-future::AsyncStatus` binding is now used; compile/runtime validation pending |
| HLT-22 | 🟡 | Startup orphan-cleanup summary and safety counters | `CleanupHealth` reports bounded scan, exact matches, removals, recent/rejected entries, failures and age policy; Windows negative test pending |

## Recording library

| ID | Status | Work | Acceptance evidence |
|---|---:|---|---|
| LIB-01 | ✅ | Local recording metadata | Persisted under AppData |
| LIB-02 | ✅ | Local video playback | MP4 plays in application |
| LIB-03 | ✅ | Rename recording | Metadata updates |
| LIB-04 | ✅ | Delete recording | File/card removed |
| LIB-05 | ✅ | Open recording location | Explorer selects file |
| LIB-06 | 🟡 | Native Windows video thumbnails | Normal-path conversion, bounded retries, typed path-free failure stages and worker timing diagnostics are implemented; rerun pending |
| LIB-07 | 🟡 | Background thumbnail backfill | Per-item failure stages and generated/failed totals implemented; existing-library validation pending |
| LIB-08 | 🔵 | Automatic UI refresh after thumbnail completion | Thumbnail appears without page restart |
| LIB-09 | 🔵 | Search and sorting | Title/date/duration sorting |
| LIB-10 | ⚪ | Folders and collections | Organisational UI |
| LIB-11 | ⚪ | Recording details panel | Technical/media metadata |
| LIB-12 | ⚪ | Repair missing/corrupt metadata | Rebuild from local MP4 files |
| LIB-13 | ⚪ | Storage usage and cleanup | Usage summary and cleanup tools |

## Reliability

| ID | Status | Work | Acceptance evidence |
|---|---:|---|---|
| REL-01 | 🔵 | 30-minute recording test | Playable, correct duration/A/V |
| REL-02 | 🔵 | Two-hour recording test | Stable memory, correct duration/A/V |
| REL-03 | 🟡 | Static-screen test | Final-tail hold implemented; full-source and selected-area 60-second mostly-static MP4s must remain within one second of requested duration and play correctly |
| REL-04 | 🔵 | High-motion 60 FPS test | Stable frame pacing |
| REL-05 | 🔵 | Multi-monitor/DPI matrix | Different scaling/resolutions |
| REL-06 | 🔵 | Window resize handling | Defined resize behaviour |
| REL-07 | 🔵 | Window close/minimise handling | Clear error/finalised output |
| REL-08 | 🔵 | Camera/microphone disconnect handling | No hang/corruption |
| REL-09 | 🔵 | Low-disk-space check | Refuse safely before/during capture |
| REL-10 | 🟡 | Orphaned segment/candidate cleanup | Background startup scan removes only exact UUID-named part/mixed/system/native-finalizer files older than 24 hours; Windows stale/recent/final-file matrix pending |
| REL-11 | 🔵 | Crash-recovery metadata | Active session journal |
| REL-12 | ⚪ | Recover playable output after crash | Recovery workflow |
| REL-13 | ⚪ | Diagnostic log rotation | Bounded log storage |

## Desktop UX

| ID | Status | Work |
|---|---:|---|
| UX-01 | ✅ | Responsive recording controls |
| UX-02 | ✅ | Source selection |
| UX-03 | ✅ | Area-selection UI |
| UX-04 | 🔵 | Global Start/Stop hotkey |
| UX-05 | 🔵 | Pause/Resume hotkey |
| UX-06 | 🔵 | System tray controls |
| UX-07 | 🔵 | Compact recording controller |
| UX-08 | 🔵 | Optional countdown |
| UX-09 | 🔵 | Audio level meters |
| UX-10 | 🔵 | Camera preview/position controls |
| UX-11 | ⚪ | Cursor-click highlighting |
| UX-12 | ⚪ | Keystroke display |
| UX-13 | ⚪ | Do-not-disturb integration |
| UX-14 | ⚪ | Recording-complete notification |

## Remove FFmpeg from the product

| ID | Status | Work |
|---|---:|---|
| FFM-01 | ✅ | Screen encoding no longer requires FFmpeg |
| FFM-02 | ✅ | Selected-area encoding no longer requires FFmpeg |
| FFM-03 | ✅ | Thumbnail extraction no longer requires FFmpeg |
| FFM-04 | 🟡 | Camera normally uses native Windows capture |
| FFM-05 | 🔵 | Remove FFmpeg camera fallback |
| FFM-06 | 🔵 | Replace emergency segment-concat fallback |
| FFM-07 | 🔵 | Remove external-system-audio compatibility path |
| FFM-08 | 🔵 | Remove FFmpeg discovery/sidecar logic |
