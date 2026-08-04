# Native refresh rotation and startup restoration

Date: 2026-08-04
Status: implemented, Windows and configured-provider validation pending

## Scope

Recorder can now restore a native OIDC access session from a subject-bound Windows Credential Manager refresh credential. The restoration path is native-only, serialized, bounded, signature verified, subject continuous and cancellation-generation protected.

This batch does not call the paid-access API and does not grant `desktop_full_access`.

## Durable credential format

New interactive sign-ins and successful refresh rotations persist a v2 binary envelope containing:

- a fixed Recorder format/version marker;
- the 16-byte UUID subject verified from the signed ID token;
- the opaque refresh-token bytes.

The complete envelope remains within the 2,560-byte Windows generic-credential limit. Temporary envelope and token buffers use best-effort volatile zeroing.

The prior v1 target stored only raw refresh-token bytes. Because it cannot prove which verified subject owns the credential, Recorder reports its non-secret presence but never sends it to the token endpoint. A new interactive sign-in replaces it with v2.

## Refresh request boundary

The refresh flow:

1. accepts only a parsed v2 subject-bound credential;
2. builds a public-client `refresh_token` grant without a client secret;
3. sends it only to the compile-time-pinned HTTPS token endpoint;
4. disables redirects, retries, runtime proxy discovery and response decompression;
5. enforces five-second connect/read bounds and a twelve-second total timeout;
6. accepts only one JSON content type, identity encoding and at most 64 KiB;
7. strictly parses the token response and approved scopes.

## Refreshed identity verification

Before any credential or session replacement, Recorder:

- resolves the exact `kid` and approved RS256/ES256 algorithm from the pinned JWKS endpoint;
- verifies the original compact-JWS signing input with `ring`;
- validates exact issuer, client-ID audience, authorized party and time claims;
- rejects any `nonce` field in a refreshed ID token;
- requires a canonical UUID subject;
- requires the refreshed subject to equal the subject stored in the v2 envelope.

A subject mismatch fails closed and leaves the previous durable credential and in-memory session unchanged.

## Atomic rotation and session replacement

The complete refreshed session candidate is validated before the commit boundary. The native session lock then protects this order:

1. confirm the cancellation generation captured before credential loading/network work is unchanged;
2. transactionally write and read back the rotated v2 credential;
3. restore the previous v2 credential if read-back fails;
4. replace the native-only access session;
5. increment the session generation so concurrent late candidates cannot overwrite the winner.

No fallible operation follows a successful credential write before the in-memory session assignment.

## Startup and retry behavior

Startup launches one serialized native restoration attempt. A no-argument `restore_oidc_session` command provides an explicit retry surface without accepting a token, subject, issuer, endpoint or key from the WebView.

Non-secret status now includes:

- current or legacy refresh-credential presence;
- legacy-v1 presence;
- whether restoration is required;
- whether restoration was attempted or failed;
- whether automatic restoration is enabled.

## Security impact

- Data read: subject-bound v2 refresh credential; legacy-v1 presence and bounded bytes for zeroed inspection/cleanup.
- Data written: rotated v2 subject-bound refresh credential.
- Data deleted: legacy v1 credential on successful interactive persistence; v1 and v2 on explicit local clear.
- Data returned to the WebView: booleans and native-session expiry only.
- Network communication added: bounded HTTPS refresh-token request and existing pinned JWKS retrieval.
- Network destinations: compile-time-pinned token and JWKS endpoints only.
- Redirects: disabled.
- Runtime proxy discovery: disabled.
- Retries: disabled.
- New dependency: none.
- New Tauri command: `restore_oidc_session`, no arguments.
- New Tauri permission: none.
- Token exposure: none.
- Subject exposure: none.
- Paid access change: none.

## Validation still required

- `cargo fmt --all -- --check` and Rust tests;
- Windows `cargo check` against Credential Manager APIs;
- configured Supabase interactive sign-in creating v2;
- restart restoration and refresh rotation;
- forced timeout, malformed response, wrong key, bad signature and subject mismatch;
- clear-during-refresh cancellation;
- concurrent retry serialization;
- legacy-v1 detection and interactive replacement;
- proof that no secret or subject reaches logs or WebView serialization.

## Remaining work

- provider revocation/logout;
- access-token renewal before native-session expiry;
- authenticated `/api/v1/me/access` call;
- React AuthGate and payment-pending state;
- native entitlement enforcement and offline lease.
