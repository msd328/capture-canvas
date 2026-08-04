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

1. Pull the committed lockfile, live-completion, subject-bound refresh rotation, startup-restoration, native `/me/access` and paid recording-start gate batches; run `scripts/windows-local-check.ps1` and `scripts/supabase-local-check.ps1`; resolve any TypeScript, Rust, migration, RLS, RPC, dependency-lock or pgTAP failures.
2. Create and document separate Supabase development, staging and production projects; link only the intended environment and validate the committed migrations against development before any remote staging/production push.
3. Validate the no-argument native authorization-code and subject-bound refresh paths against a configured Supabase provider, including exact JWKS key resolution, RS256/ES256 verification, strict identity/nonce rules, subject continuity, one-time replay rejection, transactional Windows rotation, startup restoration, clear-during-network cancellation and native session expiry. Then implement provider logout/revocation and bounded pre-expiry renewal.
4. Validate a real Supabase OAuth access token against `GET /api/v1/me/access`, including JWKS rotation, timeout, disabled/expired entitlement and cross-user rejection evidence; then validate the native-only access cache and release-build recording-start gate for active, unpaid, disabled, stale and unconfigured states.
5. Implement verified-webhook provisioning of `desktop_full_access`, desktop AuthGate, payment selection, payment-pending flow, access refresh after payment and broader native Rust entitlement enforcement without blocking Stop/finalization/cleanup.
6. Implement the common billing-provider interface and Stripe Checkout/subscriptions first, including signature-verified idempotent webhooks and Customer Portal management.
7. Add PayPal subscriptions and verified webhooks, then PhonePe hosted checkout/UPI AutoPay after production merchant recurring-payment capability is confirmed.
8. Add a short-lived signed offline entitlement lease and validate expiry, clock skew, refresh and revoked/expired account behavior.
9. Implement owner-derived cloud records, authenticated upload-session creation and the direct object-storage adapter.
10. Repeat Pause/Resume and confirm native `FinalizerHealth` success or bounded `FallbackHealth` output plus final candidate publication.
11. Check the first Windows CI run and resolve any remaining frontend, formatting, test, dependency-lock or Windows compilation failures.
12. Capture `ThumbnailHealth` plus `LibraryHealth` evidence for the now-validated live Library refresh path.
13. Validate the stale/recent/final-file cleanup matrix, including stale `.ffmpeg-finalizing-*` candidates.
14. Run the 60-second mostly-static full-source and selected-area duration matrix.
15. Validate camera, submitted A/V drift, and audio-mixer health with camera + microphone + system audio.

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
| HLT-23 | 🟡 | Native file-open and clip-decode timeout/cancellation outcome | Windows compiler accepted bounded open/decode waits and diagnostics; forced timeout/cancellation runtime evidence remain pending |
| HLT-24 | 🟡 | FFmpeg fallback timeout, termination, candidate and publication health | `FallbackHealth` reports segment validation, spawn, in-memory manifest submission, process exit/timeout, kill/reap, candidate validation, publish retries and cleanup deferral; Windows fallback runtime pending |
| HLT-25 | 🟡 | Library revision and persisted-update notification health | Live Library refresh was confirmed working on Windows; exact path-free `LibraryHealth` log evidence remains pending |
| HLT-26 | 🟡 | Native secure-auth storage, subject-bound rotation and clear health | `AuthHealth` reports only stage, support, result, v1/v2 presence, subject-binding, replacement/read-back/rollback/rotation/clear booleans and numeric Windows error codes; v2 rotation and legacy cleanup are implemented, while Windows/configured-provider evidence remains pending |
| HLT-27 | 🟡 | Native OIDC transaction lifecycle and one-time PKCE/nonce take health | Secret-free prepare/status/cancel/probe plus `oidc_pkce_take` diagnostics report expiry, S256 readiness, state matching, nonce retention, replay rejection and typed missing/expired/mismatch outcomes; coordinated unit tests added and Windows runtime evidence pending |
| HLT-28 | 🟡 | Pinned OIDC client configuration and authorization preparation health | `AuthHealth` reports configured/unconfigured state, fixed callback mode, scope count, URL size and native-verifier handling without endpoint, client ID, state, nonce, challenge or URL values; Windows validation pending |
| HLT-29 | 🟡 | Native OIDC browser, callback and coordinated exchange-grant handoff health | Path/token-free stages report bind, listen, callback validation, code expiry and combined callback/PKCE take success or typed missing/expired/mismatch failure; both native owners clear on failure, unit coverage is added and configured Windows validation remains pending |
| HLT-30 | 🟡 | Pinned token/JWKS verification, subject continuity and native-session restoration health | `AuthHealth` reports only configuration booleans, deadlines, cache/rotation outcome, approved algorithm, signature/claim/subject-continuity booleans, no-WebView-token-input completion/refresh stages, serialized startup restoration and atomic credential/session commit outcomes; identifiers and token values remain native and Windows/configured-provider validation is pending |
| HLT-31 | 🟡 | Verified SaaS access-token, entitlement-read and native gate health | `SaasHealth` and `AccessGateHealth` report only stage, typed result, full-access boolean, entitlement count and enforcement decision; native access caching and release recording-start enforcement are implemented, while real JWT/JWKS rotation, RPC timeout, Supabase runtime and Windows active/unpaid/disabled/stale gate evidence remain pending |

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
| REL-14 | 🟡 | Windows CI build/test gate | Complete Windows local validation passed on 2026-07-31 through frozen Bun install, lint, production frontend build, Rust formatting, tests and Windows cargo check; the reviewed Cargo lockfile includes the explicit root `ring 0.17.14` edge, while the current subject-bound refresh/access/gate clean-checkout run and first green hosted run remain pending |

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
| SEC-09 | 🟡 | Automated dependency monitoring and vulnerability review | Weekly npm/Cargo Dependabot, Windows CI, stable line-ending policy and a committed Cargo lockfile are present; current clean-checkout locked validation, native verifier dependency review and the first green hosted dependency/CI cycle remain pending |
| SEC-10 | 🔵 | Secret scanning and repository protection | Secret scanning enabled; test secret is blocked or detected without entering history |
| SEC-11 | ⚪ | Signed executable, installer, updater, and update metadata | Signature verification passes on a clean machine |
| SEC-12 | 🟡 | OS secure storage and native OIDC token lifecycle | Windows Credential Manager readiness/status/clear, state-bound callback code/PKCE/nonce handoff, bounded authorization and refresh exchanges, pinned JWKS cache, exact RS256/ES256 key selection, strict initial/refreshed identity checks, canonical subject continuity, no-argument WebView completion/restoration, native-only access sessions, subject-bound v2 refresh envelopes, atomic rotation/read-back/rollback and serialized startup restoration are implemented; configured-provider/Windows validation, provider logout/revocation, pre-expiry renewal, low-level store consolidation and macOS Keychain support remain |
| SEC-13 | 🟡 | SaaS authentication, object authorisation, tenancy, and rate-limit tests | Pinned asymmetric JWKS verification, issuer/audience/client-ID/time checks, owner-derived parameterless access RPC and cross-user/expired/disabled pgTAP tests are implemented; real-token runtime, upload object authorization and rate limits remain |
| SEC-14 | ⚪ | Optional encrypted local recording storage | Keys are protected by the OS and recovery/deletion behaviour is documented |
| SEC-15 | ⚪ | Independent penetration test and remediation verification | High/critical findings resolved before public release |
| SEC-16 | 🟡 | Supabase RLS and server-role isolation | Public profile/plan policies, private billing schema, explicit grants/revocations, secret-safe readiness and pgTAP tests are implemented; local execution and remote environment validation pending |
| SEC-17 | 🔵 | Payment webhook authenticity and idempotency | Stripe, PayPal and PhonePe signatures are verified from raw requests; duplicate provider event IDs cannot repeat state transitions |
| SEC-18 | 🟡 | Paid entitlement enforcement and offline lease | A release-fail-closed native recording-start guard now requires a fresh backend-confirmed active `desktop_full_access` result while keeping pause/stop/finalization available; React route gate, broader native command coverage, configured-provider negative tests and bounded signed offline lease remain |
| SEC-19 | 🔵 | Billing secret and payment-data boundary | Provider secrets remain server-only; hosted checkout keeps card/UPI credentials outside Recorder; logs exclude tokens, payment credentials and full webhook bodies |

## SaaS and sharing

The target is a paid desktop + SaaS product. Anyone may register, download and install
Recorder, but registration alone does not unlock protected routes or native recording
commands. Full access requires a backend-confirmed active `desktop_full_access`
entitlement. Supabase is the target authentication/database foundation; Stripe, PayPal
and PhonePe are normalized behind one billing service. The detailed architecture record
is `docs/status/2026-08-03-saas-paid-access-architecture.md`.

| ID | Status | Work |
|---|---:|---|
| SAAS-01 | 🟡 | Native state/nonce/PKCE, pinned authorization/token/JWKS trust metadata, browser/loopback capture, coordinated one-time grant consumption, bounded Rustls authorization and refresh exchanges, strict response parsing, exact RS256/ES256 verification, initial nonce validation, refreshed nonce rejection, canonical UUID subject continuity, no-argument completion/restoration commands, native-only access sessions, subject-bound v2 refresh rotation and serialized startup restoration are implemented; configured Supabase runtime validation, provider logout/revocation and pre-expiry renewal remain |
| SAAS-02 | 🟡 | Windows Credential Manager readiness/status/clear, transient native-only PKCE handling, subject-bound v2 refresh envelopes, legacy-v1 detection/cleanup, transactional read-back/rollback rotation, restart restoration status and failure-isolated Settings retry are implemented; Windows/configured-provider validation, low-level store consolidation and macOS Keychain support remain |
| SAAS-03 | 🟡 | Resumable uploads | Upload-session metadata is limited to 64 KiB, must arrive within ten seconds, uses strict UTF-8/JSON/Zod validation and still fails closed; authenticated object-storage adapter pending |
| SAAS-04 | 🔵 | Upload progress/retry |
| SAAS-05 | 🔵 | Shareable links |
| SAAS-06 | 🟡 | Public/private/link-only permissions | Visibility contract exists; server-side ownership and authorization enforcement pending |
| SAAS-07 | ⚪ | Cloud video processing/streaming |
| SAAS-08 | 🟡 | Cloud thumbnails and metadata | Strict cloud recording metadata contract exists; Supabase persistence and cloud library pending |
| SAAS-09 | ⚪ | Comments and reactions |
| SAAS-10 | ⚪ | Team workspaces |
| SAAS-11 | 🔵 | Usage limits |
| SAAS-12 | 🔵 | Multi-provider subscription billing | Stripe, PayPal and PhonePe checkout/webhook adapters normalize into one subscription and entitlement model |
| SAAS-13 | 🔵 | Storage quotas |
| SAAS-14 | 🔵 | Retention/deletion policy |
| SAAS-15 | ⚪ | Administration and abuse tools |
| SAAS-16 | 🟡 | Paid desktop AuthGate and paywall | Release builds now deny new recording starts without a fresh active backend-confirmed `desktop_full_access` cache; React startup/access-check/subscribe/payment-pending routes, payment provider selection and broader native enforcement remain |
| SAAS-17 | 🟡 | Authoritative `/me/access` endpoint and native client | Server-side bounded RS256/ES256 JWKS verification, exact token claims, verified `sub`, parameterless `get_my_access()`, normalized response and cross-user tests plus a pinned bounded native client/cache are implemented; real-token/Supabase runtime and Windows gate evidence remain |
| SAAS-18 | 🟡 | Supabase PostgreSQL and RLS foundation | Local CLI configuration, versioned paid-access migrations, private billing writes, owner-scoped profile reads, secret-safe readiness and pgTAP tests are implemented; execution and remote environment validation pending |
| SAAS-19 | 🔵 | Common billing-provider abstraction | Provider availability, checkout creation, webhook verification, event normalization, cancellation and management sessions |
| SAAS-20 | 🔵 | Signed offline entitlement lease | Short-lived native-verifiable lease supports bounded offline recording and expires closed without revalidation |
| SAAS-21 | 🔵 | Subscription management | Stripe portal plus equivalent PayPal/PhonePe cancellation, renewal and payment-status workflows |

## Accepted paid-access architecture

### User and access flow

```text
Download/install
  → Login or register through Supabase Auth OIDC + PKCE
  → Backend verifies token and reads /api/v1/me/access
  → No desktop_full_access: show Stripe / PayPal / PhonePe selection
  → Backend creates internal checkout tied to verified Supabase user UUID
  → Provider-hosted checkout opens in system browser
  → Verified idempotent webhook updates subscription and entitlement
  → Desktop refreshes /api/v1/me/access
  → React routes and native Rust commands unlock
```

A browser success redirect may trigger a status refresh but must never grant access.

### Canonical identity and payment correlation

The permanent account key is `auth.users.id`, derived from the verified token `sub`.
Email is profile/contact data and is not used to decide who paid. Payment ownership is
resolved only through:

```text
verified user UUID
  ↔ internal billing_checkouts.id
  ↔ provider checkout/order/subscription ID
  ↔ verified provider webhook
  ↔ entitlements(user_id, desktop_full_access)
```

The checkout request body must not contain a trusted user ID. The backend derives it
from the verified access token before creating the internal checkout.

### Access states

| State | Access |
|---|---|
| No session | Login/register only |
| Registered, unpaid | Account, provider selection, support and sign out |
| Checkout pending | Payment-pending and bounded access-status polling |
| Active `desktop_full_access` | Full Recorder, Library, upload and sharing |
| Past due/grace | Explicit product-policy-dependent bounded access |
| Cancelled at period end | Access until verified entitlement expiry |
| Expired/suspended/disabled | Paywall and account management only |

### Target desktop routes

```text
/auth
/access-check
/subscribe
/payment-pending
/
/library
/settings
/billing
```

Route guards are not sufficient by themselves. Protected Tauri commands require a
native entitlement check, and cloud operations require server authorization on every
request.

## Supabase database and identity

| ID | Status | Work | Acceptance evidence |
|---|---:|---|---|
| DB-01 | 🟡 | Supabase project and environment setup | Local CLI configuration, migration/test workflow and secret-ignore rules are committed; separate remote development/staging/production projects and links remain pending |
| DB-02 | 🟡 | Supabase Auth identity mapping | `auth.users.id` foreign keys, profile synchronization and verified OAuth access-token `sub` mapping are implemented; real-token and email-change runtime validation remain |
| DB-03 | 🟡 | Profiles table | Own-row SELECT, display-name-only UPDATE, account-status protection and auth-user synchronization are implemented; pgTAP/runtime validation pending |
| DB-04 | 🟡 | Plans and provider-price catalog | Public plan and private provider-price schemas, provider/currency constraints and uniqueness are implemented; approved production products/prices remain unseeded |
| DB-05 | 🟡 | Provider customer mappings | Unique user/provider and provider/customer relationships are implemented; provider adapter integration pending |
| DB-06 | 🟡 | Internal checkout records | Server-owned checkout UUID, user/plan/provider/status/amount/expiry fields and provider-ID uniqueness are implemented; checkout API pending |
| DB-07 | 🟡 | Normalized subscriptions | Provider subscription IDs, normalized statuses, periods and cancellation fields are implemented; webhook transitions pending |
| DB-08 | 🟡 | Entitlements | Unique user/feature rows, source subscription/provider and validity bounds are implemented; `desktop_full_access` provisioning pending |
| DB-09 | 🟡 | Webhook event idempotency | Unique provider/event IDs, payload hash and processing-state schema are implemented; signed webhook handlers pending |
| DB-10 | 🟡 | Billing audit and reconciliation | Safe billing audit schema and indexes are implemented; transactional writers and scheduled reconciliation remain pending |
| DB-11 | 🟡 | Row Level Security | RLS, owner profile policies, private-table isolation and parameterless owner-derived access RPC tests are implemented; local pgTAP and remote cross-user execution remain pending |
| DB-12 | 🟡 | Service-role isolation | Private-schema grants/revocations, server-only environment contract and `.env`/Supabase state ignores are implemented; bundle/remote secret-isolation validation pending |
| DB-13 | ⚪ | Backup and recovery | Migration rollback, point-in-time recovery, restore drill and retention objectives documented and tested |
| DB-14 | ⚪ | Data lifecycle and deletion | Account deletion coordinates auth user, billing metadata, entitlements, recordings, retention obligations and provider references |

Minimum target relations:

```text
auth.users
  ├── profiles.user_id
  ├── billing_customers.user_id
  ├── billing_checkouts.user_id
  ├── billing_subscriptions.user_id
  └── entitlements.user_id

billing_checkouts
  └── provider_checkout_id

billing_subscriptions
  └── provider_subscription_id

billing_webhook_events
  └── unique(provider, provider_event_id)
```

## Billing and paid entitlement

| ID | Status | Work | Acceptance evidence |
|---|---:|---|---|
| BILL-01 | 🔵 | Paid-only product policy | Registration/download/install remain public; protected desktop functions require active `desktop_full_access` |
| BILL-02 | 🔵 | Provider availability endpoint | Backend returns enabled providers by country, currency, plan and merchant capability |
| BILL-03 | 🔵 | Common checkout endpoint | Authenticated `POST /api/v1/billing/checkout` accepts provider and plan only; user identity comes from verified JWT |
| BILL-04 | 🔵 | Internal checkout correlation | Internal checkout UUID maps verified user to provider checkout/order/subscription identifiers without email matching |
| BILL-05 | 🔵 | Stripe subscription checkout | Hosted Checkout subscription uses approved server-side Price ID and internal checkout/user references |
| BILL-06 | 🔵 | Stripe webhook processing | Raw-body signature verification, event idempotency and transactional subscription/entitlement update pass |
| BILL-07 | 🔵 | Stripe Customer Portal | Authenticated user receives a short-lived management session for invoices, payment method and cancellation |
| BILL-08 | 🔵 | PayPal subscription checkout | Approved plan and internal checkout reference produce a browser approval flow tied to the verified user |
| BILL-09 | 🔵 | PayPal webhook processing | Authenticity verification and idempotent normalized subscription transitions pass |
| BILL-10 | 🔵 | PhonePe hosted checkout | Supported INR/customer flow creates merchant order linked to internal checkout before redirect |
| BILL-11 | 🔵 | PhonePe recurring-payment readiness | Production merchant confirms UPI AutoPay/recurring capability; unsupported one-time-only setup cannot represent an active subscription |
| BILL-12 | 🔵 | PhonePe webhook processing | Server-to-server authenticity and idempotent order/subscription normalization pass |
| BILL-13 | 🔵 | Normalized subscription states | `pending`, `active`, `grace_period`, `past_due`, `suspended`, `cancelled` and `expired` map consistently across providers |
| BILL-14 | 🔵 | `desktop_full_access` provisioning | Only verified webhook/reconciliation state can activate, extend, suspend or expire the entitlement |
| BILL-15 | 🟡 | `/api/v1/me/access` | Verified JWT `sub` receives normalized provider/status/expiry/features through an owner-derived RPC; native bounded client/cache is implemented, while real Supabase token, JWKS rotation and runtime cross-user evidence remain pending |
| BILL-16 | 🔵 | Desktop login and paywall routes | Startup resolves session/access before mounting Recorder; unpaid users cannot reach protected routes |
| BILL-17 | 🟡 | Native Rust entitlement guard | New recording starts now fail closed in release builds without a fresh active backend-confirmed `desktop_full_access`; broader command coverage, configured-provider negative tests and offline lease integration remain |
| BILL-18 | 🔵 | Payment-pending confirmation | Desktop polls access status with a bounded deadline/backoff; redirect parameters cannot unlock the app |
| BILL-19 | 🔵 | Signed offline access lease | Lease includes internal user, features, issue/expiry and is verified natively with tamper/expiry/clock-skew tests |
| BILL-20 | ⚪ | Cancellation and grace policy | Cancel-at-period-end, payment failure, grace duration, suspension and recovery behavior are product-defined and tested |
| BILL-21 | ⚪ | Refund and chargeback handling | Provider disputes/refunds update access according to documented policy without destructive duplicate transitions |
| BILL-22 | ⚪ | Subscription reconciliation jobs | Scheduled provider comparisons repair missed webhooks and alert on unresolved state divergence |
| BILL-23 | ⚪ | Billing support tooling | Safe user/provider lookup, event timeline and manual remediation require audited administrator authorization |

Required API surface:

```text
POST /api/v1/auth/exchange
POST /api/v1/auth/refresh
POST /api/v1/auth/logout

GET  /api/v1/me
GET  /api/v1/me/access

GET  /api/v1/billing/providers
GET  /api/v1/billing/plans
GET  /api/v1/billing/subscription
POST /api/v1/billing/checkout
POST /api/v1/billing/cancel
POST /api/v1/billing/manage

POST /api/v1/webhooks/stripe
POST /api/v1/webhooks/paypal
POST /api/v1/webhooks/phonepe
```

## Paid-access security invariants

1. Never unlock from an email match.
2. Never trust a user ID supplied by the desktop checkout request.
3. Derive ownership from a verified Supabase token `sub` claim.
4. Create the internal checkout before provider redirect.
5. Persist provider IDs against the internal checkout and user.
6. Verify all provider webhooks and process them idempotently and transactionally.
7. Never unlock from a success-page redirect.
8. Only trusted backend code may write subscription and entitlement state.
9. Require `desktop_full_access` at React, native Rust and SaaS API boundaries.
10. Keep Supabase service-role and Stripe/PayPal/PhonePe secrets server-only.
11. Use hosted checkout so Recorder never handles card, PayPal credential or UPI secret data.
12. Exclude access/refresh tokens, payment credentials and full webhook payloads from default logs.

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
- Supabase migrations, RLS, service-role isolation, backup and owner-separation tests pass.
- Stripe, PayPal and PhonePe checkout/webhook paths pass authenticity, idempotency, cancellation and cross-user negative tests for supported markets.
- Login, paywall, `/me/access`, native entitlement guard and signed offline-lease expiry/tamper tests pass.
- Signed installer and updater path tested on a clean Windows machine.
- Privacy policy, data inventory, dependency/licence report, billing terms, refund policy and incident-response contact completed.

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
| 2026-07-31 | `16cb8b7..1f52838` | Added method/media-type checks, a 64 KiB byte ceiling, ten-second body deadline, strict UTF-8/JSON/Zod validation and fail-closed upload-session rejection; SAAS-03 remains 🟡 and SEC-13 remains ⚪ |
| 2026-08-01 | `c0539a8..384d8b0` | Added native ten-minute OIDC state/nonce/S256 transaction state, native-only verifier handling, expiry/replay tests, secret-free readiness diagnostics and Settings validation; SAAS-01/02, SEC-12 and HLT-27 → 🟡 while SEC-13 remains ⚪ |
| 2026-08-02 | `1a4442c..8189d1c` | Added compile-time-pinned OIDC client metadata, strict HTTPS/native-redirect/scope validation, standards-compliant authorization URL construction, Settings readiness status and HLT-28; SAAS-01 and SEC-12 remain 🟡 while SEC-13 remains ⚪ |
| 2026-08-02 | `2b08248..27ffcec` | Added native Windows browser launch, bounded numeric-loopback callback interception, strict HTTP/Host/query/state validation, native code expiry, Settings controls and HLT-29; SAAS-01 and SEC-12 remain 🟡 while SEC-13 remains ⚪ |
| 2026-08-02 | `684742d..9efbba6` | Added an all-or-none compile-time token endpoint, issuer, audience and JWKS trust contract, combined readiness and a backend browser-flow gate; SAAS-01, SEC-12 and HLT-30 remain 🟡 while SEC-13 remains ⚪ |
| 2026-08-03 | `f8109fb` | Documented the Supabase identity/database foundation, Stripe/PayPal/PhonePe adapters, internal checkout-to-user correlation, verified-webhook entitlement model, paid AuthGate, native access enforcement and offline lease; DB-01–14, BILL-01–23, SAAS-12/16–21 and SEC-16–19 added for tracking |
| 2026-08-03 | `0109d17..d1c67dd` | Added local Supabase configuration, paid-access migrations, private-schema/RLS/grant isolation, profile synchronization, billing/entitlement/webhook schemas, secret-safe readiness, environment hygiene and pgTAP validation; SAAS-18, SEC-16 and DB-01–12 → 🟡 pending execution |
| 2026-08-03 | `d3ccf76..1873a9c` | Added bounded asymmetric Supabase access-token verification, owner-derived parameterless access RPC, normalized `/me/access`, cross-user/disabled/expired pgTAP tests, readiness gating and HLT-31; SAAS-17, BILL-15 and SEC-13 → 🟡 pending execution |
| 2026-08-03 | `8bb3d43..358b5d0` | Added strict native authorization-code/refresh form builders, bounded strict token-response parsing, non-secret exchange readiness/probe UI and formatting fixes; SAAS-01 and SEC-12 remain 🟡 because network exchange and token validation are disabled |
| 2026-08-03 | `217971b..a3df04a` | Added one-time native callback-grant take, `grantTaken` lifecycle, replay rejection, expiry handling, secret-free health and unit coverage; SAAS-01, SEC-12 and HLT-29 remain 🟡 pending coordinated PKCE handoff and Windows validation |
| 2026-08-03 | `741aa0b..97f7bb1` | Added state-bound coordinated callback-code/PKCE/nonce consumption, clear-both failure handling, native-only nonce zeroing, typed diagnostics and unit coverage; SAAS-01, SEC-12 and HLT-27/29 remain 🟡 pending Windows validation and HTTPS exchange |
| 2026-08-03 | `d821429..d1b325b` | Added a private bounded Rustls token transport with no redirects/retries/runtime proxy, strict response framing, 5/5/12-second deadlines, 64 KiB streaming limit, native-only unverified token ownership and Settings probes; SAAS-01, SEC-12 and HLT-30 remain 🟡 pending Windows validation and JWKS-backed ID-token verification |
| 2026-08-03 | `76d25ee..d5b8fca` | Added bounded pinned JWKS retrieval, five-minute cache, one forced rotation refresh, strict RS256/ES256 public-key parsing, duplicate-key rejection, strict ID-token header parsing and exact `kid`/algorithm selection; SAAS-01, SEC-12, HLT-30 and REL-14 remain 🟡 pending a committed Cargo lockfile and Windows/configured-provider validation |
| 2026-08-03 | `d77e1c7..5462614` | Added bounded strict ID-token payload parsing, exact issuer/client audience, canonical UUID subject, multi-audience `azp`, time/lifetime and constant-time nonce checks plus negative unit coverage; claims remain explicitly unverified until RS256/ES256 signature validation, so SAAS-01, SEC-12, HLT-30 and REL-14 remain 🟡 |
| 2026-08-03 | `76970ca..52531d3` | Added explicit locked `ring 0.17.14` verifier ownership, exact compact-JWS signing-input verification for RS256 and ES256, fixed positive/tamper/wrong-key vectors, verified native identity promotion and security documentation; SAAS-01, SEC-12, HLT-30 and REL-14 remain 🟡 pending lockfile regeneration, Windows tests and configured-provider evidence |
| 2026-08-03 | `ad208e5` | Regenerated the reviewed Cargo lockfile to add the explicit root `ring 0.17.14` dependency edge and applied Rust formatting without changing the resolved verifier package version/source/checksum; REL-14 and SEC-09 remain 🟡 pending the current locked run and hosted CI |
| 2026-08-04 | `be607ab..6ae16b1` | Added no-argument live native OIDC completion/status commands, one-time Settings completion, signed identity promotion into an expiring native-only access session, clear-memory integration, no-WebView-token-input diagnostics and security documentation; SAAS-01/02, SEC-12, HLT-30 and REL-14 remain 🟡 pending Windows and configured-provider evidence |
| 2026-08-04 | `b5e50c7..9e09558` | Added a staged private Windows refresh-credential transaction with 2,560-byte bounds, read-back verification, previous-value restoration/new-value deletion, secret-buffer clearing, typed diagnostics, cross-platform failure tests and security documentation; SAAS-01/02, SEC-12, HLT-26/30 and REL-14 remain 🟡 because live persistence and Windows validation are pending |
| 2026-08-04 | `1906885..9408bf9` | Reordered verified completion into a lock-protected refresh-credential/session commit: the complete session candidate is validated before any credential write, clear/logout generation is checked before persistence, write/read-back rollback remains active and no fallible install step follows a successful credential write; SAAS-01/02, SEC-12, HLT-26/30 and REL-14 remain 🟡 pending Windows, cancellation-race and configured-provider validation |
| 2026-08-04 | `00b6b6f..5b8be57` | Added startup refresh-credential reconciliation, non-secret restoration-required status, complete local clear across dedicated/legacy credential targets and zeroed temporary read buffers; SAAS-01/02, SEC-12, HLT-26/30 and REL-14 remain 🟡 pending Windows validation |
| 2026-08-04 | `4b872eb..f41e034` | Added subject-bound v2 refresh envelopes, bounded public-client refresh exchange, strict refreshed ID-token verification, nonce absence and subject-continuity checks, atomic credential rotation/native-session replacement, serialized startup restoration, no-argument retry, legacy-v1 fail-closed handling and security/Settings status; SAAS-01/02, SEC-12, HLT-26/30 and REL-14 remain 🟡 pending Windows and configured-provider evidence |
| 2026-08-04 | `dd3902c..c70d81e` | Added a release-fail-closed native recording-start gate backed only by the short-lived verified `/me/access` cache, debug opt-in/opt-out policy, non-secret `AccessGateHealth`, denial tests for unpaid/disabled/stale states and the safety invariant that pause/stop/finalization remain available; SEC-18, SAAS-16, BILL-17 and HLT-31 → 🟡 pending Windows/configured-provider validation |
