# Bounded native MediaComposition render — 2026-07-30

## Roadmap movement

| ID | Previous | Current | Validation still required |
|---|---:|---:|---|
| CTRL-13 | 🔵 | 🟡 | Windows compilation, normal Pause/Resume success, and deliberate timeout/cancellation evidence |
| CTRL-11 | 🟡 | 🟡 | Normal render timing plus separate FFmpeg fallback timing |
| HLT-21 | New | 🟡 | Timeout/cancel/settle/publish fields on Windows |
| REL-10 | 🔵 | 🔵 | Startup cleanup of stale hidden candidates and existing part files |
| SEC-08 | 🟡 | 🟡 | Confirm new timeout/cancellation output remains path-free |

A code commit does not move these items to ✅.

## Implementation commits

| Commit | Change |
|---|---|
| `fcbdecb` | Added isolated hidden candidate rendering and initial render deadline/cancellation flow |
| `614e176` | Corrected nested-loop cancellation control flow and candidate cleanup ownership |
| `716d157` | Updated the implementation roadmap with CTRL-13/HLT-21 and remaining limits |
| `478d7f8` | Updated the security baseline and hidden-candidate data inventory |
| `2227351` | Added the direct matching `windows-future 0.2` dependency after the Windows build exposed E0432 |
| `a3a7c23` | Replaced the invalid generated-namespace import with `windows_future::AsyncStatus` |
| `e3dc50b` | Recorded the compiler evidence and kept CTRL-13/HLT-21 validation pending |

## Windows compiler evidence

The first Windows build of this batch stopped before runtime with:

```text
error[E0432]: unresolved import `windows::Foundation::AsyncStatus`
```

The recorder uses `windows = 0.61`. In this generation, the async operation support type
is supplied by the companion `windows-future` crate rather than the generated
`windows::Foundation` module. The branch now declares the matching Windows-only direct
dependency and imports:

```rust
use windows_future::AsyncStatus;
```

This fix is not yet compiler-validated. No timeout, candidate, recording, or fallback logic
was changed while correcting the binding.

## Design

MediaComposition no longer renders directly into the final UUID MP4. It renders into a
hidden unique candidate in the same approved Recordings directory:

```text
.<uuid>.native-finalizing-<pid>-<nonce>.mp4
```

The leading dot keeps the candidate outside the current asset-protocol allow path and
outside normal UUID library metadata. Same-directory publication uses an atomic rename
when Windows releases the candidate.

The render phase uses:

```text
render deadline:       20 seconds
status poll interval:  25 milliseconds
cancellation grace:     2 seconds
publish retry delays:   0 / 50 / 150 / 300 milliseconds
```

At the render deadline, the recorder requests WinRT cancellation and polls for a terminal
Completed, Canceled, or Error state. If the operation is still Started when the grace
period expires, the candidate is not deleted because Windows may still own it. Native
finalisation returns a path-free error and the existing FFmpeg fallback may write the
separate final UUID path.

## Expected normal output

```text
[Recorder][FinalizerHealth] backend=windows-mediacomposition stage=begin ok=true segment_count=2 render_timeout_ms=20000 cancellation_grace_ms=2000
[Recorder][FinalizerHealth] ... stage=create_candidate ok=true ...
[Recorder][FinalizerHealth] ... stage=render_start ok=true ...
[Recorder][FinalizerHealth] ... stage=render_result ok=true ... timeout_triggered=false
[Recorder][FinalizerHealth] ... stage=publish_candidate ok=true ...
[Recorder][FinalizerHealth] ... stage=complete ok=true ... cleanup_deferred=false
```

No `render_timeout`, `cancel_render`, or `cancel_settle` line is expected during a normal
short Pause/Resume recording.

## Expected timeout output

A genuine long/stuck render should emit:

```text
stage=render_timeout ok=false timeout_ms=20000
stage=cancel_render ok=true|false
stage=cancel_settle ok=true terminal_status=completed|canceled|error
```

or, when Windows does not settle during the grace period:

```text
stage=cancel_settle ok=false terminal_status=started grace_ms=2000 cleanup_deferred=true
stage=complete ok=false cleanup_deferred=true
```

The final recording path remains independent from the unsettled candidate, so fallback is
not permitted to race the native operation for the same output file.

## Validation checklist

1. Stop the current watcher and pull `feature/windows-native-capture`.
2. Run `bun run desktop:dev`.
3. Confirm E0432 for `windows::Foundation::AsyncStatus` is gone.
4. Confirm there are no Rust errors involving `AsyncStatus`, `Status`, `GetResults`,
   `ErrorCode`, `Cancel`, or `Close`.
5. Confirm there are no new compiler warnings.
6. Record about ten seconds, Pause, wait three seconds, Resume, record another ten seconds,
   and Stop.
7. Confirm the final MP4 is playable and duration excludes the paused wall time.
8. Capture all `FinalizerHealth` lines.
9. Normal expectation: `timeout_triggered=false`, `publish_candidate ok=true`,
   `complete ok=true`, and `cleanup_deferred=false`.
10. Confirm no hidden `.native-finalizing-*.mp4` remains after normal success.
11. Confirm `ControlHealth operation=stop` still completes.
12. Search copied output for `path=`, `C:\Users\`, and `/Users/`.
13. Run `git status --short`; a local Cargo lockfile may refresh because the previously
    transitive `windows-future` package is now declared directly.
14. Do not deliberately force a 20-second timeout until the normal path compiles and plays.

## Acceptance criteria

`HLT-21` can become ✅ after Windows emits the configured deadline and normal
publish/complete fields without path leakage.

`CTRL-13` remains 🟡 after a normal success. It can become ✅ only after a controlled test
shows that a stuck/long render returns after the deadline plus cancellation grace, fallback
produces a playable final file, and stale candidate cleanup is implemented and verified.

## Known limits

- The hard deadline currently covers `RenderToFileAsync`, not StorageFile open waits,
  `MediaClip::CreateFromFileAsync(...).get()`, or FFmpeg fallback execution.
- A timeout that remains unsettled may leave captured screen content in a hidden candidate.
- Startup candidate cleanup is tracked under REL-10 and is the immediate follow-up.
- The fixed 20-second deadline may reject legitimate long MediaComposition renders; this
  will be reassessed from runtime timing evidence and may be replaced by direct remux.
- FFmpeg discovery and execution trust remain open under SEC-07.

## Security impact

```text
Security impact:
Added a direct declaration for the async-support crate already used by windows 0.61 and
corrected a compile-time type path. Runtime media and security behaviour are unchanged.

Data accessed:
No new data. The bounded finalizer continues to access recording segments, WinRT async
status/error state, monotonic timing and the UUID-derived hidden candidate.

Data written:
Cargo dependency metadata, the existing hidden candidate/final MP4 behaviour, path-free
FinalizerHealth output and repository tracking documents.

Network communication added:
None.

New permissions/capabilities:
None.

External processes:
None added. Existing FFmpeg fallback remains.

Untrusted inputs:
No new input class. Generated MP4 segments, WinRT status/HRESULT values, filesystem
operation results and MediaComposition output remain untrusted.

Validation added:
Compile-time dependency/version alignment for `AsyncStatus`; existing deadline,
cancellation, terminal-state polling, isolated destination and cleanup checks remain.

Secrets involved:
None.

Security tests completed:
Compiler evidence identified the invalid import. Static dependency, source-diff and
permission/network review completed; corrected Windows compilation remains pending.

Remaining risks:
The corrected import and subsequent WinRT method calls are not yet Windows-compiled.
Unsettled cancellation can leave a hidden candidate containing captured content. Startup
cleanup and bounds for WinRT open/decode and FFmpeg fallback remain open. Recordings are
not encrypted at rest.
```
