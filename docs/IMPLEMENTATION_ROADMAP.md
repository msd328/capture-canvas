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

1. Pull the combined SaaS foundation, run `scripts/windows-local-check.ps1`, validate **Settings → Cloud account → Check secure storage**, and verify `/api/v1/health` plus `/api/v1/capabilities`.
2. Repeat Pause/Resume and confirm native `FinalizerHealth` success or bounded `FallbackHealth` output plus final candidate publication.
3. Check the first Windows CI run and resolve any remaining frontend, formatting, test, dependency-lock or Windows compilation failures.
4. Select the real SaaS API origin and OIDC provider; add only the chosen HTTPS origin to CSP before enabling desktop cloud traffic.
5. Implement native OIDC authorization-code/PKCE transaction handling, state/nonce verification, token exchange, rotation, logout and revocation.
6. Implement the authenticated upload-session API and direct object-storage adapter from the shared upload contracts.
7. Capture `ThumbnailHealth` plus `LibraryHealth` evidence for the now-validated live Library refresh path.
8. Validate the stale/recent/final-file cleanup matrix, including stale `.ffmpeg-finalizing-*` candidates.
9. Run the 60-second mostly-static full-source and selected-area duration matrix.
10. Validate camera, submitted A/V drift, and audio-mixer health with camera + microphone + system audio.

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
| CTRL-11 | 🟡 | Stop-stage timing breakdown | `FinalizationHealth` now separates native, FFmpeg fallback and total finalisation time; Windows Pause/Resume runtime evidence remains pending |
| CTRL-12 | 🔵 | Pause/Resume without re-encoding | Timestamp rebasing/direct remux |
| CTRL-13 | 🟡 | Bounded native and emergency finalisation | WinRT open/decode/render bounds compile; FFmpeg concat now has a 120-second deadline with kill/reap, isolated candidate output and bounded publication retries; Windows fallback and forced-timeout validation remain |
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
| HLT-21 | 🟡 | Native render timeout, cancellation and isolated-candidate outcome | Windows compiler accepted render timeout/cancellation fields and Recorder started; forced-timeout, cancellation-settle and publication runtime evidence remain pending |
| HLT-22 | 🟡 | Startup orphan-cleanup summary and safety counters | `CleanupHealth` recognises part/mixed/system/native-finalizer and FFmpeg-finalizer candidates; stale/recent/final-file Windows matrix remains pending |
| HLT-23 | 🟡 | Native file-open and clip-decode timeout/cancellation outcome | Windows compiler accepted bounded open/decode waits and diagnostics; forced timeout/cancellation runtime evidence remains pending |
| HLT-24 | 🟡 | FFmpeg fallback timeout, termination, candidate and publication health | `FallbackHealth` reports segment validation, spawn, in-memory manifest submission, process exit/timeout, kill/reap, candidate validation, publish retries and cleanup deferral; Windows fallback runtime pending |
| HLT-25 | 🟡 | Library revision and persisted-update notification health | Live Library refresh was confirmed working on Windows; exact path-free `LibraryHealth` log evidence remains pending |
| HLT-26 | 🟡 | Native secure-auth storage status and readiness health | `AuthHealth` emits only stage, support, status, result and numeric error code fields; Windows compile/probe evidence pending |

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
| LIB-08 | ✅ | Automatic UI refresh after thumbnail completion | User confirmed on Windows that the Library updates without navigation or restart after the revision-snapshot batch |
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
| REL-10 | 🟡 | Orphaned segment/candidate cleanup | Startup cleanup now recognises exact UUID-named native and FFmpeg finalizer candidates plus part/mixed/system files; recent-file preservation observed and stale deletion matrix remains pending |
| REL-11 | 🔵 | Crash-recovery metadata | Active session journal |
| REL-12 | ⚪ | Recover playable output after crash | Recovery workflow |
| REL-13 | ⚪ | Diagnostic log rotation | Bounded log storage |
| REL-14 | 🟡 | Windows CI build/test gate | Complete Windows local validation passed on 2026-07-31 through frozen Bun install, lint, production frontend build, Rust formatting, tests and Windows cargo check; first green hosted run remains pending |

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
| FFM-06 | 🟡 | Replace emergency segment-concat fallback with native finalisation | Existing emergency fallback is isolated, bounded and diagnosable; complete removal still requires stable MediaComposition runtime validation |
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
| SEC-02 | 🟡 | Strict production Content Security Policy | Production/dev CSP and security headers configured; Windows dev app starts; packaged UI validation remains |
| SEC-03 | 🟡 | Replace wildcard asset-protocol scope with recording-only access | Scope limited to `$HOME/Recordings/*.mp4`; playback and arbitrary-file rejection tests pending |
| SEC-04 | 🟡 | Canonical recording-path allowlist for create/delete/open/thumbnail/upload | UUID/filename/root/regular-file checks implemented for current local operations; negative and filesystem-race tests pending |
| SEC-05 | 🟡 | Minimal Tauri capabilities and plugin set | Main-window core-only capability and plugin removal compile/run in Windows dev; packaged/runtime permission tests remain |
| SEC-06 | 🟡 | Comprehensive Rust command-input validation and limits | UUID, title, FPS, device ID, output-path and settings validation added; dimensions/crops/source IDs/negative test suite remain |
| SEC-07 | 🔵 | Remove or authenticate external executable discovery | Production never executes an unverified PATH/environment-selected FFmpeg binary |
| SEC-08 | 🟡 | Structured diagnostic-log privacy and redaction | `FallbackHealth` and `FinalizationHealth` add only stage, timing, counts, process outcome and file-size fields; free-form backend errors and exported reports still require review |
| SEC-09 | 🟡 | Automated dependency monitoring and vulnerability review | Weekly npm/Cargo Dependabot, Windows CI, stable line-ending policy and a successful complete Windows local validation cycle are present; first green hosted dependency/CI cycle remains pending |
| SEC-10 | 🔵 | Secret scanning and repository protection | Secret scanning enabled; test secret is blocked or detected without entering history |
| SEC-11 | ⚪ | Signed executable, installer, updater, and update metadata | Signature verification passes on a clean machine |
| SEC-12 | 🟡 | OS secure storage for future account tokens | Windows Credential Manager status/probe/clear boundary is implemented with fixed targets and no secret-return command; Windows runtime and macOS Keychain support remain |
| SEC-13 | ⚪ | SaaS authentication, object authorisation, tenancy, and rate-limit tests | Cross-user/cross-workspace access attempts are rejected server-side |
| SEC-14 | ⚪ | Optional encrypted local recording storage | Keys are protected by the OS and recovery/deletion behaviour is documented |
| SEC-15 | ⚪ | Independent penetration test and remediation verification | High/critical findings resolved before public release |

## SaaS and sharing

The mid-August target is a focused desktop + SaaS MVP. Authentication, secure desktop token storage, direct object-storage upload, progress/retry, cloud metadata and share permissions are prioritised; cloud transcoding, teams, billing, comments and advanced administration remain later work.

| ID | Status | Work |
|---|---:|---|
| SAAS-01 | 🟡 | Provider-neutral OIDC/PKCE, capability, visibility and upload metadata contracts; provider selection and native token exchange pending |
| SAAS-02 | 🟡 | Windows Credential Manager readiness/status/clear boundary and failure-isolated Settings UI; Windows validation and real login integration pending |
| SAAS-03 | 🟡 | Resumable uploads | Strict upload-session contract and fail-closed API route exist; authenticated object-storage adapter pending |
| SAAS-04 | 🔵 | Upload progress/retry |
| SAAS-05 | 🔵 | Shareable links |
| SAAS-06 | 🟡 | Public/private/link-only permissions | Visibility contract exists; server-side ownership and authorization enforcement pending |
| SAAS-07 | ⚪ | Cloud video processing/streaming |
| SAAS-08 | 🟡 | Cloud thumbnails and metadata | Strict cloud recording metadata contract exists; database persistence and cloud library pending |
| SAAS-09 | ⚪ | Comments and reactions |
| SAAS-10 | ⚪ | Team workspaces |
| SAAS-11 | 🔵 | Usage limits |
| SAAS-12 | ⚪ | Subscription billing |
| SAAS-13 | 🔵 | Storage quotas |
| SAAS-14 | 🔵 | Retention/deletion policy |
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
| 2026-07-29 | `8a8990e..d8496d8` | Added per-source mixer underrun, queue-drop and peak-depth diagnostics for both Windows capture backends; AUD-08, AUD-09 and HLT-12 → 🟡 |
| 2026-07-29 | `78e758f` | Added command/engine boundary timings and failure-stage diagnostics; CTRL-10, CTRL-11 and HLT-17 → 🟡 |
| 2026-07-29 | `1b3026b..a7ffa55` | Added a final static-tail hold anchored to the user Pause/Stop request; VID-10, REL-03 and HLT-18 → 🟡 |
| 2026-07-30 | `cb94dd5..af4994b` | Reinterpreted timeline health from timestamp span, retained sample density separately, and removed local paths from structured capture/A-V health output; HLT-16 and SEC-08 remain 🟡 pending Windows validation |
| 2026-07-30 | `17435c8..ca871a3` | Windows build/runtime validated encoder/WGC/control diagnostics; fixed WinRT extended-path handling for native concat/thumbnails and removed redaction-related warnings |
| 2026-07-30 | `6090651..9520869` | Added path-free native MediaComposition stage/HRESULT diagnostics and typed thumbnail failure-stage reporting; HLT-19 and HLT-20 → 🟡 |
| 2026-07-30 | `fcbdecb..614e176` | Added isolated candidate rendering, a 20-second MediaComposition render deadline, cancellation request, two-second settle grace and deferred-cleanup reporting; CTRL-13 and HLT-21 → 🟡 |
| 2026-07-30 | `de1b43a..5298720` | Added bounded background cleanup for exact UUID-named stale part/mixed/system/native-finalizer files; REL-10 and HLT-22 → 🟡 |
| 2026-07-30 | `4b63feb` | Added a least-privilege Windows GitHub Actions gate for frozen frontend dependencies, lint/build, Rust formatting, tests and cargo check; REL-14 → 🟡 |
| 2026-07-30 | `32ad827..a4bf436` | Bounded WinRT StorageFile open and MediaClip decode waits with cancellation settling and path-free diagnostics; CTRL-13 and HLT-23 → 🟡 |
| 2026-07-30 | `34ea234..7e750d5` | Added a local Windows validation script, scoped the known macro warning expectation, and recorded bounded-wait compile plus recent-artifact evidence; CTRL-13, HLT-21/23, REL-10 and REL-14 remain 🟡 |
| 2026-07-31 | `832e222..3e58792` | Added bounded isolated FFmpeg concat fallback, native/fallback timing, path-free fallback health and stale candidate cleanup; CTRL-11/13, FFM-06, HLT-22/24 and SEC-08 remain 🟡 pending Windows validation |
| 2026-07-31 | `61ff6f5..5b6d015` | Added explicit cross-platform line-ending policy, actionable Windows validation failures and recorded the 6,899-error Prettier baseline; REL-14 and SEC-09 remain 🟡 pending frontend normalization and a green run |
| 2026-07-31 | `dccc925..25393d1` | Fixed camera preview cleanup ownership, allowed six stable Fast Refresh helper exports and recorded a successful complete Windows local gate; REL-14 and SEC-09 remain 🟡 pending a zero-warning rerun and hosted CI |
| 2026-07-31 | `51baedd..fa7e0e9` | Added bounded library revision snapshots, persisted-change notifications and automatic UI refresh; LIB-08 and HLT-25 → 🟡 pending Windows validation |
| 2026-07-31 | `da131b3..0f83836` | Added provider-neutral OIDC/upload contracts, disabled-by-default SaaS configuration, Windows Credential Manager readiness commands and Settings UI; LIB-08 → ✅, SAAS-01/02, SEC-12 and HLT-26 → 🟡 |
| 2026-07-31 | `b219b02..2a20e70` | Added versioned SaaS API contracts, fail-closed health/capability routing and a reserved upload-session boundary; SAAS-03/06/08 → 🟡 pending build/runtime and authenticated-adapter validation |
| 2026-07-31 | `71c4bf3..03146e1` | Isolated secure-auth readiness failures from recorder/device settings; SAAS-02 remains 🟡 pending Windows compile and Credential Manager probe evidence |
