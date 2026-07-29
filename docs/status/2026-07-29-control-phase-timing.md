# Recorder control-phase timing — 2026-07-29

## Roadmap movement

| ID | Previous | Current | Validation still required |
|---|---:|---:|---|
| CTRL-10 | 🔵 | 🟡 | Windows build and successful Start timing line; native engine sub-phases remain a refinement |
| CTRL-11 | 🔵 | 🟡 | Windows build and successful Stop timing line; internal capture/audio/finaliser split remains a refinement |
| HLT-17 | New | 🟡 | Confirm success/failure `ControlHealth` fields for Start/Pause/Resume/Stop |

A code commit does not move these items to ✅.

## Implementation commits

| Commit | Change |
|---|---|
| `78e758f` | Added Start/Pause/Resume/Stop command and engine-boundary timing diagnostics |
| `e430766` | Updated the master implementation roadmap |
| `97edaaa` | Removed backend error text from failure health lines for log privacy |

## Success output

Start:

```text
[Recorder][ControlHealth] operation=start ok=true validation_ms=... engine_ms=... total_ms=...
```

Pause and Resume:

```text
[Recorder][ControlHealth] operation=pause ok=true engine_ms=... total_ms=...
[Recorder][ControlHealth] operation=resume ok=true engine_ms=... total_ms=...
```

Stop:

```text
[Recorder][ControlHealth] operation=stop ok=true engine_ms=... path_validation_ms=... library_insert_ms=... library_persist_ms=... thumbnail_schedule_ms=... total_ms=...
```

## Failure output

```text
[Recorder][ControlHealth] operation=<operation> ok=false stage=<stage> total_ms=...
```

Possible stages include:

```text
validation
title_validation
engine
worker_join
path_validation
library_persist
```

The original error is still returned to the local UI. It is deliberately excluded
from `ControlHealth` to avoid repeating paths, device names, or other sensitive local
context in diagnostic logs.

## Measurement boundaries

- Start `validation_ms` covers Rust-side recording configuration and title validation.
- Start `engine_ms` covers the blocking `RecordingEngine::start` call, including source
  resolution, output setup, native capture/encoder startup, camera attachment, and
  audio startup.
- Pause/Resume `engine_ms` covers their complete blocking engine operations.
- Stop `engine_ms` covers capture/audio shutdown, segment mixing/finalisation, file
  verification, metadata construction, and the current best-effort inline thumbnail
  call inside `RecordingEngine::stop`.
- Stop then measures path validation, in-memory library insertion, `recordings.json`
  persistence, and asynchronous thumbnail-worker scheduling separately.
- `thumbnail_schedule_ms` measures only worker creation/scheduling, not background
  thumbnail generation.

## Validation checklist

1. `bun run desktop:dev` compiles without new Rust errors or warnings from this batch.
2. Start one full-display recording and capture the Start line.
3. Pause once and capture the Pause line.
4. Resume and capture the Resume line.
5. Stop and capture the Stop line.
6. Confirm the recording appears in the library and plays.
7. Repeat with selected-area capture.
8. Repeat with camera + microphone + system audio.
9. Confirm no `ControlHealth` line contains a local file path or device name.
10. Compare totals with roadmap targets: warm Start/Pause/Resume under 1 second and
    single-segment Stop under 2 seconds.

## Interpretation limits

- `engine_ms` is deliberately aggregated in this batch. It does not yet distinguish
  source resolution, camera startup, audio startup, encoder startup, capture stop,
  media finalisation, or inline thumbnail generation from one another.
- Existing capture-level `start_ms`/`stop_ms`, StreamHealth, CameraHealth,
  AudioMixerHealth, and AvHealth lines should be correlated with `ControlHealth`.
- Scheduler load and Windows device-driver latency can affect these wall-clock values.
- The timing code is passive and does not impose timeouts or alter media behaviour.

## Security impact

```text
Security impact:
Added passive wall-clock phase measurements and redacted failure-stage diagnostics.

Data accessed:
Monotonic process time, operation result, validation result, and local library
operation completion.

Data written:
Local stderr timing lines and repository tracking documentation.

Network communication added:
None.

New permissions/capabilities:
None.

External processes:
None added.

Untrusted inputs:
Existing recording configuration and backend operation results.

Validation added:
Success timing fields, explicit failure-stage fields, and failure-log privacy review.

Secrets involved:
None.

Security tests completed:
Static source review only. Windows compilation and runtime log review remain pending.

Remaining risks:
Other pre-existing health lines still include local output paths under SEC-08.
Recordings remain unencrypted at rest. This batch measures latency but does not add
bounded native finalisation timeouts or resolve any measured bottleneck.
```
