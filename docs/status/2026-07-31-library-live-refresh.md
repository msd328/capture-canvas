# Automatic library refresh — 2026-07-31

## Scope

Implemented `LIB-08` so the local recording library can update while the page remains open after asynchronous thumbnail generation, startup thumbnail backfill, recording completion, rename, or deletion.

## Changes

- Added a monotonically increasing in-memory library revision.
- Added a condition-variable wake-up after successfully persisted library changes.
- Added `get_library_snapshot`, returning the current revision and recording list.
- Added `wait_for_library_update`, which waits for a newer revision for at most 25 seconds and then returns a fresh snapshot.
- Kept the wait off the Tauri command thread through `spawn_blocking`.
- Updated recording completion, rename, deletion, thumbnail generation, and thumbnail backfill to notify library watchers only after metadata persistence succeeds.
- Updated the library page to maintain one local long-poll while mounted, update React state only when the revision changes, stop starting new waits after unmount, and retry command failures after one second.
- Added path-free `LibraryHealth` notification output containing only a fixed reason and revision.
- Added no frontend package, Tauri plugin, network request, or filesystem capability.

## Expected runtime evidence

Open the Library page while a recording without a thumbnail is visible. The placeholder should be replaced when native thumbnail generation completes, without navigating away or restarting Recorder.

Expected backend output after a newly completed recording:

```text
[Recorder][LibraryHealth] stage=notify reason=recording_added revision=<n>
[Recorder][ThumbnailHealth] ok=true ... stage=complete ...
[Recorder][LibraryHealth] stage=notify reason=thumbnail_ready revision=<n+1>
```

Startup backfill should use:

```text
[Recorder][LibraryHealth] stage=notify reason=thumbnail_backfill revision=<n>
```

Rename and deletion use `recording_renamed` and `recording_deleted` respectively.

## Validation status

```text
LIB-08  🟡  Revision snapshots and bounded wait channel implemented; Windows compile and live thumbnail refresh pending
HLT-25  🟡  Path-free library revision notifications implemented; runtime evidence pending
```

## Security impact

```text
Security impact:
Added two read-only Tauri commands and an in-process wait/notification channel for already-authorised local recording metadata. No media bytes, local paths, account data, or credentials are added to diagnostic output.

Data accessed:
The existing in-memory recording metadata list, its persisted recordings.json representation, and a monotonically increasing in-memory revision.

Data written:
Existing recording metadata persistence remains unchanged. The revision exists only in memory. Documentation and source files were updated in the repository.

Network communication added:
None.

New permissions/capabilities:
None. No Tauri capability or plugin change.

External processes:
None.

Untrusted inputs:
The frontend-provided afterRevision value is deserialised as u64. The frontend cannot choose the wait duration; every wait is fixed at a maximum of 25 seconds.

Validation added:
Library updates are announced only after metadata persistence succeeds. The frontend applies a snapshot only when its revision changes and stops creating new waits after route unmount.

Secrets involved:
None.

Security tests completed:
Static review of revision ordering, lost-wake prevention, fixed timeout, path-free diagnostics, and route-unmount behaviour.

Remaining risks:
A compromised frontend could invoke multiple concurrent bounded wait commands and temporarily occupy multiple blocking workers. The normal library UI creates one waiter per mounted page. A server-side concurrent-wait cap should be added if this command surface is exposed to less-trusted web content. Windows compile/runtime validation remains pending.
```
