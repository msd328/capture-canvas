# Native OIDC refresh-credential transaction

Date: 2026-08-04
Status: Primitive implemented; live completion wiring and Windows validation pending
Roadmap: SAAS-01, SAAS-02, SEC-12, HLT-26, HLT-30, REL-14

## Delivered

Recorder now contains a private Windows Credential Manager replacement primitive for the future durable OIDC refresh token.

The primitive:

1. accepts only a non-empty opaque value no larger than the 2,560-byte Windows generic-credential limit;
2. rejects ASCII control characters;
3. reads and retains the previous credential in a native secret buffer;
4. writes the proposed replacement to the existing Recorder refresh-token target;
5. reads the credential back and compares it without logging either value;
6. restores the previous credential when read-back fails or differs;
7. deletes the newly written credential when no previous credential existed;
8. best-effort zeroes previous and read-back buffers when dropped;
9. reports only typed stage, replacement, read-back and rollback booleans.

Cross-platform unit coverage exercises bounds, successful replacement, failed read-back rollback and initial-write failure preservation without touching a real credential store.

## Deliberately not active yet

The live `complete_oidc_sign_in` command does not call this primitive. A successful provider exchange still:

- verifies the ID-token signature and claims;
- installs the access token in native memory;
- drops and zeroes the refresh token;
- returns `refreshTokenPersisted=false`.

This staging avoids changing durable sign-in semantics before Windows compilation and transaction tests are confirmed. The next wiring batch must validate the access-session candidate before writing, call the transaction exactly once, and commit the memory session only after verified persistence succeeds.

## Security impact

Data read:
- existing Recorder refresh credential, when present;
- one proposed opaque refresh credential when the primitive is called in a future batch.

Data written:
- none in the current live sign-in path;
- the private primitive can transactionally replace the existing Windows credential when called by native Rust.

Network added:
- None.

Persistent storage added:
- no new target; the primitive uses `Recorder/app.recorder.desktop/saas-refresh-token/v1`, which is already used by secure-auth status and clear operations.

Dependencies added:
- None.

Tauri commands or capabilities added:
- None.

WebView data exposure:
- None. The primitive is crate-private and accepts no command or frontend input.

Logs:
- typed stage/result/rollback fields and Windows numeric error codes only;
- no token, subject, issuer, audience, endpoint or key data.

External processes:
- None.

Remaining risks:
- the Credential Manager target and low-level Win32 calls currently exist in both the original secure-auth module and this staged transaction module; consolidation should occur when live persistence is wired;
- rollback write/delete success is checked, but configured Windows read-back and forced-failure evidence remain pending;
- refresh rotation, restoration, provider revocation and logout network calls do not exist;
- durable identity still must not imply `desktop_full_access`.

## Validation required

```powershell
cd C:\Users\DINESH\Desktop\VidRec\capture-canvas

git pull --rebase --autostash origin feature/windows-native-capture

cd src-tauri
cargo fmt --all
cd ..

.\scripts\windows-local-check.ps1
```

Required evidence before live wiring:

- the new rollback unit tests pass;
- `cargo check --locked` accepts the Windows Credential Manager calls;
- no new warning appears;
- the complete frontend build remains green;
- a dedicated probe or configured-provider run confirms write/read-back/delete against the Recorder target without printing secret material.
