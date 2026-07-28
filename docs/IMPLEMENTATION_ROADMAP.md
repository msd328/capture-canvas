# Capture Canvas implementation roadmap

This document is the source of truth for recorder implementation status on
`feature/windows-native-capture`.

## Update policy

Every implementation batch must update this file in the same commit series as the
code change. Status is based on evidence, not intent:

- ✅ **Validated** — implemented and verified by a successful Windows build/runtime test.
- 🟡 **Implemented, validation pending** — code is present, but the current Windows build or runtime behaviour has not yet been confirmed.
- 🔵 **Next** — selected for the next implementation batches.
- ⚪ **Planned** — accepted roadmap work, not currently being implemented.
- ⛔ **Postponed** — deliberately deferred until a prerequisite is complete.

A pushed commit alone does not move an item to ✅. Compiler output, runtime logs, or CI
must provide the validation evidence.

## Current focus

1. Validate the instrumented `windows_capture` facade and timeline coverage diagnostics on Windows.
2. Complete WGC frame-delivery/FPS-limiter, camera, and audio-mixer counters.
3. Verify and fix static-screen duration continuity.
4. Reduce warm Start, Pause, Resume, and Stop latency.
5. Remove the remaining FFmpeg compatibility paths.

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
| VID-07 | 🟡 | Background H.264/AAC encoder warm-up | Compare first Start latency before/after warm-up |
| VID-08 | 🟡 | Encoder submission counters | Instrumented facade compile/runtime pending |
| VID-09 | 🔵 | WGC received/skipped/encoded counters | Log received, rate-limited, encoded, failed counts |
| VID-10 | 🔵 | Static-screen duration continuity | 60 seconds static produces approximately 60 seconds output; timeline deficit diagnostics are now available |
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
| AUD-08 | 🔵 | Mixer underrun counters | Separate mic/system underrun counts |
| AUD-09 | 🔵 | Queue overflow/drop counters | Log bounded-queue drops |
| AUD-10 | 🔵 | Audio/video drift measurement | Report drift in milliseconds |
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
| CAM-07 | 🔵 | Camera received/applied/drop counters | Camera health line emitted at Stop |
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
| CTRL-06 | 🟡 | Encoder warm-up for first Start | Runtime latency comparison pending |
| CTRL-07 | ✅ | Pause produces independent MP4 segments | Pause/Resume architecture active |
| CTRL-08 | 🟡 | Windows MediaComposition primary finaliser | Output quality/duration matrix pending |
| CTRL-09 | ✅ | FFmpeg emergency concat fallback | Fallback implemented |
| CTRL-10 | 🔵 | Start-stage timing breakdown | Resolve/device/audio/camera/encoder timing |
| CTRL-11 | 🔵 | Stop-stage timing breakdown | Capture/audio/finaliser/library timing |
| CTRL-12 | 🔵 | Pause/Resume without re-encoding | Timestamp rebasing/direct remux |
| CTRL-13 | 🔵 | Bounded native finalisation timeout | Stop cannot wait indefinitely |
| CTRL-14 | ⚪ | Cancel while Starting | Safe cancellation |

## Health diagnostics

| ID | Status | Work | Acceptance evidence |
|---|---:|---|---|
| HLT-01 | ✅ | Backend name | Health log emitted |
| HLT-02 | ✅ | Encoder startup duration | `start_ms` emitted |
| HLT-03 | ✅ | Stop/finalisation duration | `stop_ms` emitted |
| HLT-04 | ✅ | Output size and average bitrate | Bytes/Mbps emitted |
| HLT-05 | 🟡 | Submitted frames and failures | Facade runtime pending |
| HLT-06 | 🟡 | Effective encoded FPS | Facade runtime pending |
| HLT-07 | 🟡 | Largest submitted-frame gap | Facade runtime pending |
| HLT-08 | 🟡 | Audio buffers and bytes submitted | Facade runtime pending |
| HLT-09 | 🔵 | WGC frames received | Capture-handler counter |
| HLT-10 | 🔵 | FPS-limited frames skipped | Capture-handler counter |
| HLT-11 | 🔵 | Camera frames received/applied | Camera/capture counters |
| HLT-12 | 🔵 | Mixer underruns and queue overflows | Mixer counters |
| HLT-13 | 🔵 | Audio/video drift | Millisecond drift report |
| HLT-14 | ⚪ | Exportable diagnostic report | Copy/save support bundle |
| HLT-15 | ⚪ | User-friendly health summary | Non-technical UI status |
| HLT-16 | 🟡 | Expected-frame timeline coverage and deficit | Windows compile/runtime pending; log `expected_frames`, `frame_deficit`, and `timeline_coverage_pct` |

## Recording library

| ID | Status | Work | Acceptance evidence |
|---|---:|---|---|
| LIB-01 | ✅ | Local recording metadata | Persisted under AppData |
| LIB-02 | ✅ | Local video playback | MP4 plays in application |
| LIB-03 | ✅ | Rename recording | Metadata updates |
| LIB-04 | ✅ | Delete recording | File/card removed |
| LIB-05 | ✅ | Open recording location | Explorer selects file |
| LIB-06 | 🟡 | Native Windows video thumbnails | Windows runtime validation pending |
| LIB-07 | 🟡 | Background thumbnail backfill | Existing-library validation pending |
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
| REL-03 | 🔵 | Static-screen test | Correct duration with minimal changes |
| REL-04 | 🔵 | High-motion 60 FPS test | Stable frame pacing |
| REL-05 | 🔵 | Multi-monitor/DPI matrix | Different scaling/resolutions |
| REL-06 | 🔵 | Window resize handling | Defined resize behaviour |
| REL-07 | 🔵 | Window close/minimise handling | Clear error/finalised output |
| REL-08 | 🔵 | Camera/microphone disconnect handling | No hang/corruption |
| REL-09 | 🔵 | Low-disk-space check | Refuse safely before/during capture |
| REL-10 | 🔵 | Orphaned segment cleanup | Stale parts removed/recovered |
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
| FFM-09 | 🔵 | Remove FFmpeg from production packaging |

## Distribution

| ID | Status | Work |
|---|---:|---|
| DIST-01 | ⚪ | Production Tauri build configuration |
| DIST-02 | ⚪ | Windows installer |
| DIST-03 | ⚪ | Code-signing certificate |
| DIST-04 | ⚪ | Signed executable and installer |
| DIST-05 | ⚪ | Automatic updater |
| DIST-06 | ⚪ | Stable/beta release channels |
| DIST-07 | ⚪ | Consent-based crash reporting |
| DIST-08 | ⚪ | Privacy policy and licence notices |
| DIST-09 | ⚪ | Exact dependency/licence audit |
| DIST-10 | ⚪ | Clean-machine installation test |

## SaaS and sharing

SaaS work starts after the desktop recorder passes the reliability gate.

| ID | Status | Work |
|---|---:|---|
| SAAS-01 | ⚪ | Authentication |
| SAAS-02 | ⚪ | Secure desktop token storage |
| SAAS-03 | ⚪ | Resumable uploads |
| SAAS-04 | ⚪ | Upload progress/retry |
| SAAS-05 | ⚪ | Shareable links |
| SAAS-06 | ⚪ | Public/private/link-only permissions |
| SAAS-07 | ⚪ | Cloud video processing/streaming |
| SAAS-08 | ⚪ | Cloud thumbnails and metadata |
| SAAS-09 | ⚪ | Comments and reactions |
| SAAS-10 | ⚪ | Team workspaces |
| SAAS-11 | ⚪ | Usage limits |
| SAAS-12 | ⚪ | Subscription billing |
| SAAS-13 | ⚪ | Storage quotas |
| SAAS-14 | ⚪ | Retention/deletion policy |
| SAAS-15 | ⚪ | Administration and abuse tools |

---

## Production-readiness gate

The desktop recorder is not production-ready until all of the following pass:

- Full display, full window, and selected-area recording.
- Camera + microphone + system audio together.
- Pause/Resume with correct duration and no visible quality loss.
- 30 FPS and 60 FPS output.
- At least one one-hour recording.
- Static-screen duration continuity.
- No noticeable audio/video drift.
- Warm Start, Pause, Resume, and Stop meet latency targets.
- No production FFmpeg dependency.
- Crash-safe temporary files and recovery behaviour.
- Signed installer tested on a clean Windows machine.

## Target control latency

| Operation | Target |
|---|---:|
| Warm Start | under 1 second |
| Pause | under 1 second |
| Resume | under 1 second |
| Single-segment Stop | under 2 seconds |

## Change log

| Date | Commit | Roadmap update |
|---|---|---|
| 2026-07-28 | `c688bc7` baseline | Added formal tracking after encoder submission instrumentation |
| 2026-07-28 | `02f950e` | Added repository roadmap and validation policy |
| 2026-07-28 | `d0b4708` | Added expected-frame timeline coverage and deficit diagnostics; HLT-16 → 🟡 |
