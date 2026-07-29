# Static-screen continuity — 2026-07-29

## Roadmap movement

| ID | Previous | Current | Validation still required |
|---|---:|---:|---|
| VID-10 | 🔵 | 🟡 | Windows build, 60-second static recordings, playback, and duration comparison |
| REL-03 | 🔵 | 🟡 | Full-source and selected-area mostly-static reliability matrix |
| HLT-18 | New | 🟡 | Confirm `ContinuityHealth` fields and held-frame outcome per segment |

A code commit does not move these items to ✅.

## Implementation commits

| Commit | Change |
|---|---|
| `1b3026b` | Added the final recorder-facing continuity facade crate |
| `310d858` | Added throttled BGRA snapshot retention and final static-tail submission |
| `eb17aae` | Routed the recorder dependency through the continuity facade |
| `5c9421e` | Anchored the final held frame to the user Pause/Stop request time |
| `a7ffa55` | Recorded the Pause/Stop request before native shutdown begins |
| `baa4e5e` | Updated the master implementation roadmap |

## Behaviour

The recorder now keeps one most-recent packed BGRA snapshot per active segment.

- Full-source D3D11 recording refreshes the snapshot through one GPU-to-CPU readback
  at most once per second. Normal video frames still use the existing direct D3D11
  encoder path.
- Selected-area recording copies its already-normalized BGRA buffer at most once per
  second.
- At Pause or Stop, the command boundary records the user request time before native
  capture shutdown begins.
- If the final accepted video timestamp is more than 100 ms behind that request time,
  the latest snapshot is submitted once at the request timestamp before finalisation.
- Existing StreamHealth and AvHealth wrappers observe the held submission through the
  normal encoder API.
- Warm-up clips suppress continuity output.

This addresses the final unchanged tail. If a static interval occurs in the middle of
recording and later content changes, the later WGC timestamp already spans that interval;
the extra held frame is needed specifically when recording ends during the unchanged tail.

## Expected output

```text
[Recorder][ContinuityHealth] finalize_ok=true active_wall_ms=... video_span_ms=... tail_gap_ms_before_hold=... snapshot_refreshes=... snapshot_available=true hold_needed=true hold_submitted=true hold_failed=false hold_unavailable=false
```

For continuously changing content, a normal line may instead show:

```text
hold_needed=false
hold_submitted=false
```

Warnings:

```text
[Recorder][ContinuityHealth] warning=static_tail_snapshot_unavailable ...
[Recorder][ContinuityHealth] warning=static_tail_submission_failed ...
```

The new health lines deliberately omit output paths and device names.

## Validation checklist

1. `bun run desktop:dev` compiles the continuity crate, diagnostics facade, and recorder.
2. Record a full display for 60 seconds:
   - move content for 5 seconds;
   - leave the screen unchanged for approximately 50 seconds;
   - move content for 2 seconds;
   - leave it unchanged and Stop at 60 seconds.
3. Confirm the MP4 duration is within one second of 60 seconds and playback holds the
   last visible image rather than ending early.
4. Capture the `ContinuityHealth`, `StreamHealth`, `AvHealth`, and `ControlHealth` lines.
5. Repeat the 60-second test with no microphone or system audio.
6. Repeat with microphone + system audio and confirm playback remains synchronized.
7. Repeat with selected-area capture and expect the same duration behaviour.
8. Pause during a static tail, wait three seconds, Resume, then Stop during another
   static tail. Expect one continuity line per active segment.
9. Run a 20-second high-motion recording and confirm `hold_needed=false` or only a very
   small final correction, with no visible final-frame regression.
10. Confirm the first successful snapshot does not materially increase Start latency
    and that later one-second refreshes do not create visible capture stalls.

## Acceptance criteria

`VID-10`, `REL-03`, and `HLT-18` can become ✅ only when both full-source and
selected-area tests show:

```text
requested duration: 60 seconds
MP4 duration: 59–61 seconds
finalize_ok=true
snapshot_available=true
hold_failed=false
hold_unavailable=false
playback: correct final image and no corruption
```

When the final static tail exceeds 100 ms, `hold_submitted=true` is expected.

## Interpretation limits

- This is a final-tail continuity correction, not constant-frame-rate duplication.
- The output may still contain a low number of encoded samples during a static interval;
  players should display the previous sample until the held timestamp.
- The GPU snapshot is intentionally throttled to once per second. The final held image
  can therefore be up to approximately one second older than the last visual update if
  WGC stops immediately after a change.
- A held buffer submission currently passes through the existing camera-overlay
  submission counter, so a camera-enabled segment can report one additional applied
  overlay frame at finalisation.
- This batch does not add a timeout to encoder finalisation and does not correct
  long-duration audio clock drift.

## Security impact

```text
Security impact:
Added an in-memory final-frame retention layer. No capture content is written anywhere
outside the existing MP4 pipeline.

Data accessed:
One BGRA screen frame at most once per second, WGC timestamps, monotonic Pause/Stop
request time, and encoder results.

Data written:
The existing MP4 output, local redacted continuity diagnostics, roadmap documentation,
and this audit record.

Network communication added:
None.

New permissions/capabilities:
None.

External processes:
None added.

Untrusted inputs:
Captured frame dimensions, row pitch, timestamps, raw BGRA buffers, and Windows encoder
results.

Validation added:
Checked dimension/buffer arithmetic, row-padding removal, required vertical orientation,
a 100 ms final-tail threshold, snapshot-unavailable handling, submission-failure handling,
and Stop-request timestamp anchoring.

Secrets involved:
None.

Security tests completed:
Static ownership, overflow, path-free logging, and dependency-layer review only. Windows
compilation and runtime validation remain pending.

Remaining risks:
The retained frame contains screen pixels in process memory until segment finalisation.
Recordings remain unencrypted at rest. Existing older StreamHealth/AvHealth lines still
contain output paths under SEC-08. The GPU readback and buffer submission require Windows
performance and driver validation.
```
