# Native OIDC access-session boundary

Date: 2026-08-03
Status: Implemented; Windows compilation and configured-provider validation pending
Roadmap: SAAS-01, SEC-12, HLT-30, REL-14

## Delivered

Recorder now contains a private native access-session boundary that can be reached only after:

1. the coordinated one-time callback code, PKCE verifier and nonce handoff succeeds;
2. the bounded authorization-code token exchange succeeds;
3. the ID-token signature is verified against the exact pinned JWKS key;
4. issuer, audience, subject, timestamps and nonce validation succeed.

Only then may an access token be copied into the native in-memory session runtime.

The session runtime:

- stores the verified subject UUID and access token in native memory only;
- limits access-token input to 16 KiB and rejects whitespace/control characters;
- expires at the earlier of the token-response lifetime and verified ID-token expiry;
- enforces a maximum 24-hour access-session lifetime;
- clears expired sessions before status or token access;
- exposes access-token bytes only to crate-private native closures;
- best-effort zeroes token memory when replaced, expired or cleared;
- reports only non-secret active/expiry/native-only status fields.

## Deliberately still blocked

- no Tauri command invokes the live verified exchange yet;
- no React or WebView API can read the session or token;
- no refresh token is persisted;
- no refresh rotation, revocation or logout network lifecycle exists;
- no `/api/v1/me/access` request consumes the access token yet;
- no paid entitlement or native recorder-command gate is enabled.

The refresh token returned by a successful exchange is still dropped and zeroed with the unverified exchange object. This is intentional until secure persistence and rotation are implemented together.

## Tests

Unit coverage includes:

- installation only with a valid verified-identity expiry and bounded access token;
- session expiry at the shorter token/identity deadline;
- native subject/token access through a crate-private closure;
- rejection of expired identities and whitespace-bearing tokens;
- explicit clearing and replay-safe missing-session behavior;
- serialization-safe status values that never contain subject or token bytes;
- a shared test guard for the process-global runtime.

## Security impact

Data read:
- a fully verified native identity;
- one bounded access token;
- the token response lifetime.

Data written:
- one short-lived native in-memory access session;
- repository source and documentation.

Network added:
- none beyond the previously implemented private token and JWKS transports.

Persistent secrets added:
- none.

Tauri commands or capabilities added:
- none.

Logs:
- active/native-only/persistence booleans and typed failure codes only;
- no subject, token, nonce, issuer, audience or key material.

## Validation required

Run on Windows after pulling the batch:

```powershell
cd src-tauri
cargo generate-lockfile
cargo fmt --all
cd ..

.\scripts\windows-local-check.ps1
```

The roadmap items remain yellow until formatting, tests, `cargo check --locked`, and configured Supabase behavior are evidenced.
