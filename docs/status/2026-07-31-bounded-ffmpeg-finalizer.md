# Bounded FFmpeg concat finalizer — 2026-07-31

## Scope

This batch hardens the emergency multi-segment FFmpeg concat path that runs only when Windows MediaComposition cannot finalise a Pause/Resume recording.

Roadmap movement:

```text
CTRL-11  🟡  Native/fallback/total finalisation timing implemented
CTRL-13  🟡  FFmpeg concat deadline, kill and reap implemented
FFM-06   🔵 → 🟡  Emergency fallback isolated and bounded; removal still pending
HLT-22   🟡  Cleanup recognises stale FFmpeg candidates
HLT-24   New → 🟡  FFmpeg fallback process/candidate health
SEC-08   🟡  New structured fields are path-free
```

All items remain validation-pending until Windows compilation and runtime evidence are supplied.

## Previous behaviour

The fallback previously:

- wrote directly to the final `<UUID>.mp4` path;
- created `<UUID>.concat.txt` on disk containing absolute segment paths;
- blocked indefinitely through `Command::status()`;
- had no process timeout, termination or reap diagnostics;
- did not separate native and fallback finalisation timing;
- left successful segment files behind.

## New behaviour

### In-memory concat manifest

The concat manifest is written to FFmpeg standard input through `pipe:0`. No path-bearing concat-list file is persisted.

### Isolated output

FFmpeg writes to:

```text
.<UUID>.ffmpeg-finalizing-<process-id>-<nonce>.mp4
```

The final `<UUID>.mp4` is published only after FFmpeg exits successfully and the candidate is confirmed to be a non-empty regular file.

### Bounded process execution

```text
Process deadline: 120 seconds
Polling interval: 25 milliseconds
Publish retries: 0 / 50 / 150 / 300 milliseconds
```

At the deadline Recorder requests process termination and waits to reap the child. If the process cannot be confirmed reaped, the candidate is retained rather than deleted while it may still be owned.

### Publication and cleanup

After success:

1. candidate metadata is validated;
2. same-directory rename is retried with bounded delays;
3. the final UUID MP4 becomes visible;
4. source segments are removed.

A candidate that cannot be published is retained for startup cleanup rather than silently discarded.

### Startup orphan cleanup

The exact temporary-name parser now recognises both:

```text
.<UUID>.native-finalizing-<pid>-<nonce>.mp4
.<UUID>.ffmpeg-finalizing-<pid>-<nonce>.mp4
```

The existing direct-child, regular-file, no-link, 24-hour age and 4,096-entry bounds remain unchanged.

## Diagnostics

Expected fallback lines:

```text
[Recorder][FallbackHealth] backend=ffmpeg-concat stage=begin ok=true segment_count=2 timeout_ms=120000
[Recorder][FallbackHealth] backend=ffmpeg-concat stage=spawn ok=true process_id=...
[Recorder][FallbackHealth] backend=ffmpeg-concat stage=write_manifest ok=true manifest_bytes=...
[Recorder][FallbackHealth] backend=ffmpeg-concat stage=process_exit ok=true exit_code=0 ...
[Recorder][FallbackHealth] backend=ffmpeg-concat stage=validate_candidate ok=true bytes=...
[Recorder][FallbackHealth] backend=ffmpeg-concat stage=publish_candidate ok=true attempt=1 ...
[Recorder][FallbackHealth] backend=ffmpeg-concat stage=complete ok=true timeout_triggered=false cleanup_deferred=false total_ms=...
```

Final method timing:

```text
[Recorder][FinalizationHealth] stage=complete ok=true method=windows-mediacomposition native_ms=... fallback_ms=0 total_ms=...
```

or:

```text
[Recorder][FinalizationHealth] stage=complete ok=true method=ffmpeg-fallback native_ms=... fallback_ms=... total_ms=...
```

No path, username, segment filename or concat-manifest content is included in these structured lines.

## Windows validation

### Compile and test

```powershell
cd C:\Users\DINESH\Desktop\VidRec\capture-canvas
git pull origin feature/windows-native-capture
.\scripts\windows-local-check.ps1
bun run desktop:dev
```

### Normal Pause/Resume test

```text
Record 10 seconds
Pause
Wait 3 seconds
Resume
Record 10 seconds
Stop
```

When MediaComposition succeeds, expect:

```text
method=windows-mediacomposition
fallback_ms=0
```

When native finalisation falls back, expect all `FallbackHealth` success stages and:

```text
method=ffmpeg-fallback
fallback_ms=<non-zero>
cleanup_deferred=false
```

Acceptance:

- final video plays;
- paused wall time is excluded;
- Stop returns;
- only the final UUID MP4 remains after successful publication;
- no `.concat.txt` file is created;
- no `.ffmpeg-finalizing-*` candidate remains after success;
- structured output contains no local path.

### Timeout test

A deliberate timeout test must wait until normal fallback compilation and playback are confirmed. The production deadline should not be reduced merely to force a test. A test-only injection mechanism may be added in a separate batch.

## Security impact

```text
Security impact:
Isolated the emergency FFmpeg concat writer from the final recording path, removed the on-disk path-bearing concat manifest, bounded process execution, and added path-free process/publication diagnostics.

Data accessed:
Generated MP4 segment files, segment metadata and sizes, monotonic timing, child-process status/exit code, and the final UUID filename used to derive a hidden candidate.

Data written:
An in-memory concat manifest, a hidden FFmpeg candidate MP4, the final MP4 after successful same-directory publication, path-free FallbackHealth/FinalizationHealth lines, cleanup parser tests and repository tracking documents.

Network communication added:
None.

New permissions/capabilities:
None.

External processes:
The existing FFmpeg emergency concat process remains. It now has a 120-second deadline, termination request and child reap. No new executable is introduced.

Untrusted inputs:
Generated segment files and metadata, local filesystem results, FFmpeg process results and exit codes, timestamps, and the currently configured FFmpeg executable selection.

Validation added:
Non-empty regular-segment checks, stdin-only manifest submission, isolated unpredictable candidate naming, bounded process polling, timeout termination/reap reporting, candidate regular-file/non-empty validation, bounded same-directory publication retries, post-success segment cleanup, and exact stale-candidate parser coverage.

Secrets involved:
None.

Security tests completed:
Static process-ownership, output-isolation, manifest-lifetime, candidate-publication, cleanup-pattern and structured-log review. Windows compilation and runtime validation remain pending.

Remaining risks:
FFmpeg executable discovery still trusts an environment override, an adjacent executable or PATH and remains tracked by SEC-07. External system-audio compatibility mixing still uses an older unbounded FFmpeg process path. A failed termination request may leave an isolated hidden candidate until startup cleanup after the 24-hour age threshold. Recordings remain unencrypted at rest.
```
