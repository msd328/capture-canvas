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

1. Rebuild the recorder after the E0753 facade fix and the first security-hardening batch.
2. Validate CSP, restricted asset playback, the main-window capability, and plugin removal on Windows.
3. Run negative tests against tampered recording metadata, invalid UUIDs, custom output paths, and out-of-root files.
4. Validate timeline coverage, WGC delivery, camera health, and submitted A/V drift diagnostics on Windows.
5. Implement audio-mixer underrun/queue-drop counters and complete remaining Rust command validation.
6. Verify and fix static-screen duration continuity.
7. Reduce warm Start, Pause, Resume, and Stop latency.
8. Remove the remaining FFmpeg compatibility paths.

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
| VID-08 | 🟡 | Encoder submission counters | E0753 from the include-based entry was replaced by a normal module path in `942e99c`; rebuild/runtime pending |
| VID-09 | 🟡 | WGC received/skipped/encoded counters | Central facade counter implemented; validate `frames_received`, `frames_rate_limited`, submissions and failures |
| VID-10 | 🔵 | Static-screen duration continuity | 60 seconds static produces approximately 60 seconds output; timeline deficit diagnostics are available |
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
| HLT-05 | 🟡 | Submitted frames and failures | E0753 entry fix committed; rebuild/runtime pending |
| HLT-06 | 🟡 | Effective encoded FPS | E0753 entry fix committed; rebuild/runtime pending |
| HLT-07 | 🟡 | Largest submitted-frame gap | E0753 entry fix committed; rebuild/runtime pending |
| HLT-08 | 🟡 | Audio buffers and bytes submitted | E0753 entry fix committed; rebuild/runtime pending |
| HLT-09 | 🟡 | WGC frames received | Facade handler wrapper implemented; rebuild/runtime pending after E0753 fix |
| HLT-10 | 🟡 | FPS-limited frames skipped | Timestamp/target-FPS limiter mirror implemented; rebuild/runtime pending after E0753 fix |
| HLT-11 | 🟡 | Camera frames received/applied/source misses | Per-segment `CameraHealth` line and warnings implemented; Windows runtime validation pending |
| HLT-12 | 🔵 | Mixer underruns and queue overflows | Mixer counters |
| HLT-13 | 🟡 | Submitted A/V duration drift and startup offset | Per-segment `AvHealth` line implemented; compare signed drift and playback on Windows |
| HLT-14 | ⚪ | Exportable diagnostic report | Copy/save support bundle |
| HLT-15 | ⚪ | User-friendly health summary | Non-technical UI status |
| HLT-16 | 🟡 | Expected-frame timeline coverage and deficit | Facade rebuild/runtime pending after E0753 entry fix |

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

## Security and privacy

The detailed policy, trust boundaries, current data inventory, and mandatory status-update template are in `docs/SECURITY_BASELINE.md`.

| ID | Status | Work | Acceptance evidence |
|---|---:|---|---|
| SEC-01 | 🟡 | Threat model, data inventory, trust boundaries, and security review policy | Baseline document committed; architecture review and future data-flow updates pending |
| SEC-02 | 🟡 | Strict production Content Security Policy | Production/dev CSP and security headers configured; Windows dev and packaged UI validation pending |
| SEC-03 | 🟡 | Replace wildcard asset-protocol scope with recording-only access | Scope limited to `$HOME/Recordings/*.mp4`; playback and arbitrary-file rejection tests pending |
| SEC-04 | 🟡 | Canonical recording-path allowlist for create/delete/open/thumbnail/upload | UUID/filename/root/regular-file checks implemented for current local operations; negative and filesystem-race tests pending |
| SEC-05 | 🟡 | Minimal Tauri capabilities and plugin set | Main-window core-only capability added and fs/shell/dialog/opener runtime/dependencies removed; schema/runtime validation pending |
| SEC-06 | 🟡 | Comprehensive Rust command-input validation and limits | UUID, title, FPS, device ID, output-path and settings validation added; dimensions/crops/source IDs/negative test suite remain |
| SEC-07 | 🔵 | Remove or authenticate external executable discovery | Production never executes an unverified PATH/environment-selected FFmpeg binary |
| SEC-08 | 🔵 | Diagnostic-log privacy and redaction | Thumbnail path logs reduced; complete capture-health path redaction still required |
| SEC-09 | 🟡 | Automated dependency monitoring and vulnerability review | Weekly npm/Cargo Dependabot configuration committed; first successful update/alert cycle pending |
| SEC-10 | 🔵 | Secret scanning and repository protection | Secret scanning enabled; test secret is blocked or detected without entering history |
| SEC-11 | ⚪ | Signed executable, installer, updater, and update metadata | Signature verification passes on a clean machine |
| SEC-12 | ⚪ | OS secure storage for future account tokens | Tokens use Windows Credential Manager/macOS Keychain and never JSON/localStorage/logs |
| SEC-13 | ⚪ | SaaS authentication, object authorisation, tenancy, and rate-limit tests | Cross-user/cross-workspace access attempts are rejected server-side |
| SEC-14 | ⚪ | Optional encrypted local recording storage | Keys are protected by the OS and recovery/deletion behaviour is documented |
| SEC-15 | ⚪ | Independent penetration test and remediation verification | High/critical findings resolved before public release |

## SaaS and sharing

SaaS work starts after the desktop recorder passes the reliability and critical-security gates.

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
- SEC-02 through SEC-10 completed and validated.
- Signed installer and updater path tested on a clean Windows machine.
- Privacy policy, data inventory, dependency/licence report, and incident-response contact completed.

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
| 2026-07-28 | `4c18cde` | First namespace repair after E0433 build failure; second Windows build exposed root `capture` collision |
| 2026-07-28 | `840efeb..fcc6b10` | Replaced the conflicting crate-root alias with a local external `windows_capture` facade crate; VID-08 and HLT-05–08/16 remain 🟡 pending rebuild |
| 2026-07-28 | `e949aba..b52ae4a` | Added central WGC delivery, FPS-limiter, capture-gap and processing-deficit counters; VID-09 and HLT-09/10 → 🟡 |
| 2026-07-28 | `885ba57..7f051ed` | Fixed E0597 callback guard lifetime through the Rust 2024 facade entry wrapper; validation remained pending |
| 2026-07-29 | `942e99c` | Replaced `include!` with a normal module path after E0753 inner-documentation errors; facade validation remains pending |
| 2026-07-29 | `41cddbf..7f93bbc` | Added the security baseline and weekly npm/Cargo dependency monitoring; SEC-01 and SEC-09 → 🟡 |
| 2026-07-29 | `a96773e..16a6595` | Added canonical UUID/file/root guards, command validation, settings normalisation, startup metadata filtering, and protected thumbnail/open/delete paths; SEC-04/06 → 🟡 |
| 2026-07-29 | `86c3a8b..15093a5` | Added main-window capability, removed unused Tauri plugins, enabled strict CSP, and restricted the asset protocol; SEC-02/03/05 → 🟡 |
| 2026-07-29 | `eaa5edc` | Updated the security baseline with implemented controls, validation requirements, and remaining risks |
| 2026-07-29 | `857c123..7c0ae5e` | Added shared native/fallback camera source counters and encoder-correlated overlay submissions; CAM-07 and HLT-11 → 🟡 |
| 2026-07-29 | `9a95245..9adc100` | Added submitted PCM/video timeline duration, startup-offset and audio-submission-gap diagnostics; AUD-10 and HLT-13 → 🟡 |
