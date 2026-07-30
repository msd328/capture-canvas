# Variable-frame timeline health and structured-log privacy — 2026-07-30

## Roadmap movement

| ID | Previous | Current | Validation still required |
|---|---:|---:|---|
| HLT-16 | 🟡 | 🟡 | Windows build plus static/high-motion `StreamHealth` comparison |
| SEC-08 | 🔵 | 🟡 | Confirm current structured health output contains no local paths; review free-form backend errors separately |

A code commit does not move either item to ✅.

## Implementation commits

| Commit | Change |
|---|---|
| `cb94dd5` | Added the diagnostics shim that derives timestamp-span coverage, retains sample density separately, suppresses the obsolete density warning, and removes paths from StreamHealth/AvHealth output |
| `af4994b` | Removed the output path from the empty-capture warning |
| `08ab370` | Updated the master implementation roadmap |
| `43c11af` | Updated the security baseline and added the batch security record |

## Timeline interpretation

The previous `timeline_coverage_pct` was actually a constant-frame sample-density
calculation:

```text
submitted frames / (active wall time × target FPS)
```

That is useful for high-motion constant-frame analysis but it is not a valid duration
continuity measure for a variable-frame screen source. A static source can legitimately
submit only a few samples while the final sample timestamp still spans the complete
recording.

The new `StreamHealth` line reports both signals:

```text
timeline_span_ms
  Submitted video timestamp span.

timeline_coverage_pct
  timeline_span_ms / active_wall_ms, capped at 100%.
  This is the continuity/duration health signal.

sample_density_pct
  Submitted frame count / target-FPS frame expectation.
  This is retained as a motion/sample-density diagnostic.
```

Only timestamp-based `timeline_coverage_pct` triggers
`warning=low_timeline_coverage`.

## Expected StreamHealth output

```text
[Recorder][StreamHealth] finalize_ok=true wall_ms=... active_wall_ms=60000 timeline_span_ms=59950 timeline_coverage_pct=99.9 target_fps=30 frames_received=... frames_rate_limited=... frames_submitted=... expected_frames=1800 frame_deficit=... sample_density_pct=... processing_deficit=0 frame_failures=0 capture_fps=... effective_fps=... max_capture_gap_ms=... max_frame_gap_ms=... audio_buffers=... audio_failures=0 audio_bytes=...
```

A mostly static recording may correctly show:

```text
sample_density_pct=5.0
timeline_coverage_pct=99.9
```

That is healthy when the MP4 duration and playback are correct.

A real continuity problem should show:

```text
timeline_coverage_pct < 90.0
[Recorder][StreamHealth] warning=low_timeline_coverage active_wall_ms=... timeline_span_ms=... coverage_pct=...
```

## Structured-log redaction

These current structured families should not emit a recording path:

```text
StreamHealth
AvHealth
ControlHealth
ContinuityHealth
CameraHealth
AudioMixerHealth
capture start/stop Health warnings
thumbnail health
```

The current batch specifically rewrites the legacy path-bearing `StreamHealth` and
`AvHealth` formats and removes the path from `empty_capture_output`.

This does not claim that every free-form backend error is redacted. Error strings,
future crash reports, support bundles, and exported diagnostics remain part of the
open SEC-08 review.

## Validation checklist

1. Stop the existing watcher, pull the branch, and run `bun run desktop:dev`.
2. Confirm the local facade compiles without macro-resolution or formatting errors.
3. Run a 20-second high-motion full-display recording.
4. Confirm `sample_density_pct` and `timeline_coverage_pct` are both plausible and no false continuity warning appears.
5. Run the 60-second mostly-static full-display test from the static-continuity batch.
6. Confirm MP4 duration is 59–61 seconds and `timeline_coverage_pct` is at least 98%.
7. Expect `sample_density_pct` to be allowed below 90% on the static test.
8. Repeat the static test with selected-area capture.
9. Repeat with microphone + system audio and compare `AvHealth` with playback sync.
10. Search the copied runtime output for:

```text
path=
C:\Users\
/Users/
```

11. Confirm none of the structured health lines contain those values.
12. Confirm ordinary UI errors still reach the local UI when an operation fails.

## Acceptance criteria

`HLT-16` can become ✅ when:

```text
high-motion recording: plausible sample density and timeline coverage
mostly-static recording: MP4 59–61 seconds
timeline_coverage_pct >= 98%
no false low-timeline warning caused only by low sample density
```

`SEC-08` remains 🟡 after this batch. It can become ✅ only after:

```text
structured runtime logs are verified path-free
free-form backend errors are reviewed/redacted
future diagnostic export/support-bundle design applies the same policy
```

## Static review limits

- The diagnostics shim relies on Rust macro textual scope reaching the nested
  implementation module. The Windows compiler is authoritative.
- Timestamp span is reconstructed from submitted-frame count and effective FPS already
  calculated by the underlying health implementation; rounding can differ by about one
  millisecond.
- The existing frame-count fields (`expected_frames` and `frame_deficit`) remain for
  density analysis and must not be interpreted as a static-screen duration failure.
- No media samples, encoder settings, capture cadence, queue behaviour, or output files
  are changed by this batch.

## Security impact

```text
Security impact:
Removed local filesystem paths from current structured recorder health output and
prevented low frame density from being mislabeled as a timeline-continuity failure.

Data accessed:
Existing frame counters, timestamps, wall-clock durations, encoder/audio counters, and
output-path values already used internally by the recording pipeline.

Data written:
Path-free local health diagnostics and repository tracking documentation.

Network communication added:
None.

New permissions/capabilities:
None.

External processes:
None added.

Untrusted inputs:
WGC timestamps, frame/audio submission counts, filesystem paths, and backend results.

Validation added:
Timestamp-span continuity percentage, separate sample-density percentage, path-free
structured warning formats, and a runtime path-leak search checklist.

Secrets involved:
None.

Security tests completed:
Static format-string and diagnostics-boundary review only. Windows compilation and
runtime log inspection remain pending.

Remaining risks:
Free-form backend errors may still include local context. Recordings remain unencrypted
at rest. Future crash reporting, support bundles, and exported diagnostics require
explicit redaction and user-consent design.
```
