# Windows runtime validation and WinRT media-path fixes — 2026-07-30

## Evidence received

The Windows development build completed successfully:

```text
Compiling windows-capture-facade
Compiling windows-capture-continuity
Compiling recorder
Finished `dev` profile
Running `target\debug\recorder.exe`
```

The local facade stack, continuity crate, CSP/capability configuration, and recorder
therefore reached a runnable Windows process. Two non-fatal `dead_code` warnings remained
for redacted `output_path` fields.

## Roadmap movement

| ID | Previous | Current | Evidence |
|---|---:|---:|---|
| VID-08 | 🟡 | ✅ | Two segments emitted 516/862 submissions and zero frame failures |
| VID-09 | 🟡 | ✅ | 802/1351 WGC frames, 286/489 rate-limited, zero processing deficit |
| HLT-05 | 🟡 | ✅ | Submitted-frame and failure fields emitted |
| HLT-06 | 🟡 | ✅ | Effective FPS emitted at 29.64 and 29.90 for a 30 FPS target |
| HLT-07 | 🟡 | ✅ | Maximum submitted-frame gap emitted at approximately 62.6 ms |
| HLT-09 | 🟡 | ✅ | WGC received counters emitted |
| HLT-10 | 🟡 | ✅ | FPS-limiter counters emitted |
| HLT-17 | 🟡 | ✅ | Start, Pause, Resume, and Stop `ControlHealth` lines emitted |
| HLT-08 | 🟡 | 🟡 | Audio fields emitted zero because audio was disabled; audio-enabled test required |
| HLT-16 | 🟡 | 🟡 | High-motion coverage validated; mostly-static coverage still required |
| HLT-18 | 🟡 | 🟡 | Continuity fields emitted, but both tails had `hold_needed=false` |
| CTRL-08 | 🟡 | 🟡 | Native finaliser was reached but could not open a WinRT media path; fix pending rerun |
| LIB-06 | 🟡 | 🟡 | First thumbnail attempt failed; path conversion/retry pending rerun |
| SEC-08 | 🟡 | 🟡 | Structured output in the supplied run was path-free; free-form errors remain open |

A successful build or runtime line does not validate camera, audio mixer, A/V drift,
selected-area, or static-tail behaviour that was not exercised in this test.

## Stream-health interpretation

First segment:

```text
active_wall_ms=17417
timeline_span_ms=17375
timeline_coverage_pct=99.8
frames_received=802
frames_rate_limited=286
frames_submitted=516
sample_density_pct=98.7
processing_deficit=0
frame_failures=0
effective_fps=29.64
max_frame_gap_ms=62.6
```

Second segment:

```text
active_wall_ms=28867
timeline_span_ms=28792
timeline_coverage_pct=99.7
frames_received=1351
frames_rate_limited=489
frames_submitted=862
sample_density_pct=99.4
processing_deficit=0
frame_failures=0
effective_fps=29.90
max_frame_gap_ms=62.5
```

These are healthy high-motion results for a 30 FPS target. The limiter reduced a roughly
46 FPS WGC delivery stream to approximately 30 encoded FPS, with no encoder-processing
deficit or submission failure.

The continuity lines showed valid snapshots and matching wall/timestamp spans:

```text
snapshot_available=true
hold_needed=false
hold_submitted=false
hold_failed=false
hold_unavailable=false
```

That is correct when a recent WGC frame already reaches the Pause/Stop request. It does
not validate the static-tail held-frame branch.

## Control latency

Observed command totals:

```text
Start   1294 ms
Pause    820 ms
Resume  1334 ms
Stop    3974 ms
```

Interpretation:

- Pause met the under-one-second target.
- Start and Resume missed the under-one-second target by roughly 0.3 seconds.
- The final Stop missed the under-two-second target because native multi-segment
  finalisation failed and FFmpeg fallback ran.
- Per-segment native stop/finalisation was 816 ms and 1365 ms before the aggregate concat.
- Warm-up completed successfully in 1949 ms, but warm Start still needs refinement.

## Warning and failure diagnosis

### Rust `output_path` warnings

Path redaction removed the final emitted use of two fields, so Rust reported them as
unread even though the diagnostic wrappers still retained them. The redaction macro now
evaluates the internal path expression but never serializes it. This removes the warnings
without disabling `dead_code` lints globally.

### Native MediaComposition failure

The error was:

```text
Windows could not open the media file
```

`std::fs::canonicalize` commonly produces an extended Windows path such as:

```text
\\?\C:\Users\...\segment.mp4
```

WinRT `StorageFile::GetFileFromPathAsync` can reject that representation. The recorder
now converts canonical extended paths to normal DOS/UNC form before calling WinRT:

```text
\\?\C:\...              -> C:\...
\\?\UNC\server\share\... -> \\server\share\...
```

The native finaliser also retries opening newly finalised segment files after 100 and
250 ms before falling back.

### FFmpeg warning

```text
[mp4] track 1: codec frame size is not set
```

This occurred only after native finalisation failed and the emergency stream-copy concat
ran. FFmpeg still returned success and the recorder completed Stop. It is a non-fatal
fallback warning in this run, not the cause of native failure. It should not appear when
MediaComposition succeeds.

### Thumbnail failure

The thumbnail loader used the same direct canonical-path-to-`StorageFile` conversion and
therefore likely hit the same extended-path incompatibility. It now uses the shared normal
path conversion and retries in its background worker at 0, 150, and 400 ms. These retries
do not extend the Stop command.

## Implementation commits

| Commit | Change |
|---|---|
| `17435c8` | Added normal WinRT path conversion and background thumbnail retries |
| `34831f7` | Applied normal path conversion and file-open retries to MediaComposition |
| `ca871a3` | Removed the two redaction-related warnings without globally suppressing lints |
| `a5df908` | Updated evidence-based roadmap statuses and current focus |

## Required rerun

1. Stop the watcher and pull the branch.
2. Run `bun run desktop:dev`.
3. Confirm the two `output_path` warnings are absent.
4. Record for about 10 seconds, Pause, wait 3 seconds, Resume, record another 10 seconds,
   and Stop.
5. Expect:

```text
[Recorder] Native Windows MediaComposition pause/resume finalizer active
```

6. Do not expect:

```text
falling back to FFmpeg concat
codec frame size is not set
```

7. Expect the final Stop total to decrease substantially; the target remains under 2 seconds.
8. Wait up to one second after Stop and expect:

```text
[Recorder][Health] thumbnail_ok=true id=... thumbnail_ms=...
```

9. Play the combined recording and verify the pause interval is absent, both segments are
   present, and duration/audio/video are correct.
10. If native finalisation still fails, capture the new error text. Passing the file-open
    stage may reveal a separate MediaClip decode or render issue.

## Security impact

```text
Security impact:
Normalized already-canonical internal media paths for WinRT APIs and removed path-bearing
context from native segment load/append errors. No filesystem allowlist was broadened.

Data accessed:
Approved recording segments, final MP4 files, Windows thumbnail data, and internal local
path representations.

Data written:
Existing final MP4/thumbnail metadata plus local path-free structured diagnostics and
repository tracking documents.

Network communication added:
None.

New permissions/capabilities:
None.

External processes:
None added. The existing FFmpeg emergency fallback remains and was exercised by the
reported run.

Untrusted inputs:
Canonical filesystem paths, newly finalized MP4 segment availability, WinRT media errors,
and thumbnail bytes.

Validation added:
Extended DOS/UNC path normalization, bounded WinRT open retries, bounded background
thumbnail retries, thumbnail size limits retained, and global dead-code suppression avoided.

Secrets involved:
None.

Security tests completed:
The supplied runtime confirmed structured health output was path-free. The new path and
retry fixes have static review only and require the next Windows run.

Remaining risks:
Free-form backend errors can still reveal local context, the native MediaComposition fix
is not yet runtime validated, FFmpeg executable trust remains open, recordings are not
encrypted at rest, and filesystem race hardening remains pending.
```
