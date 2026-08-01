# Native OIDC PKCE transaction boundary — 2026-08-01

## Scope

Implemented the provider-neutral native transaction state required before Recorder can open an OIDC authorization-code flow. This batch does not select an identity provider, construct an authorization URL, intercept a callback, exchange a code, validate an ID token, store a real refresh token, or make a network request.

## Changes

- Added one in-memory pending OIDC transaction to the native secure-auth store.
- Added 43-character URL-safe state, nonce, and PKCE verifier generation from existing native UUID-v4 randomness.
- Added RFC 7636 S256 challenge generation and a known-vector unit test.
- Added a ten-minute transaction deadline based on monotonic time, with a UTC expiry returned only for display/authorization-request construction.
- Starting a new transaction replaces the previous transaction and drops its verifier.
- Added best-effort volatile overwrite of verifier bytes when native transaction material is dropped.
- Added constant-time comparison for equal-length state values.
- A mismatched state does not consume the valid pending transaction.
- An expired transaction is cleared and rejected.
- A successful state match consumes the transaction once; replay receives a missing-transaction result.
- Added native prepare, status, cancel, and isolated security-probe commands.
- Prepare returns only state, nonce, S256 challenge, challenge method, and expiry. It never returns the verifier.
- Added a Settings action named **Check sign-in security**. The probe uses isolated temporary state, confirms S256, state round-trip, nonce retention, replay rejection, and native-only verifier handling, then leaves no pending login transaction.
- Added path-free and secret-free `AuthHealth` output for prepare, status, cancel, and probe stages.
- Added no package, Tauri plugin, external executable, network origin, CSP exception, filesystem path, or cloud persistence.

## Expected Windows evidence

Run the local validation gate, start Recorder, open **Settings → Cloud account**, and select **Check sign-in security**.

Expected toast:

```text
Local PKCE and one-time sign-in state checks passed
```

Expected backend output:

```text
[Recorder][AuthHealth] stage=oidc_probe ok=true s256=true state_round_trip=true nonce_retained=true replay_rejected=true verifier_kept_native=true
```

The output must not contain state, nonce, verifier, authorization code, token, URL, email, user ID, or local path values.

## Validation status

```text
SAAS-01  🟡  Native PKCE/state/nonce transaction generation and one-time lifecycle implemented; provider authorization URL, callback interception and token exchange pending
SAAS-02  🟡  PKCE verifier remains native and transient; Windows compile/probe and real refresh-token write/rotation lifecycle pending
SEC-12   🟡  Native verifier is not returned, logged or persisted and receives best-effort overwrite on drop; production token lifecycle and macOS secure storage pending
SEC-13   ⚪  Local state expiry and replay boundary is present, but signed-token verification, issuer/audience/nonce claim checks, server ownership, tenancy and rate limiting remain unimplemented
HLT-27   🟡  Secret-free OIDC prepare/status/cancel/probe diagnostics implemented; Windows runtime evidence pending
```

## Security impact

```text
Security impact:
Added transient native OIDC transaction state and a non-secret readiness probe. The PKCE verifier remains inside Rust memory, expires after ten minutes, is consumed once, and is never returned through Tauri. Cloud authentication is still disabled because no provider, callback handler, token endpoint, API origin or CSP allowance is configured.

Data accessed:
Existing native process memory, UUID-v4 random values, monotonic and UTC clocks, and the existing secure-auth mutex. The isolated readiness probe accesses only temporary in-memory transaction values.

Data written:
One optional pending OIDC transaction in native process memory. No OIDC transaction field is written to JSON, localStorage, Credential Manager, the filesystem, or a server. Repository source, status documentation and roadmap tracking were updated.

Network communication added:
None.

New permissions/capabilities:
None.

External processes:
None.

Untrusted inputs:
The future callback state value. The internal state-consumption boundary uses equal-length constant-time comparison, rejects expiry, preserves the valid transaction after a mismatch, and consumes a successful match once. No callback command is exposed until token exchange can be implemented atomically.

Validation added:
RFC 7636 S256 known-vector coverage; generated-token length/character tests; successful one-time state consumption; replay rejection; mismatch preservation; expiry clearing; a native isolated readiness probe; and secret-free structured diagnostics.

Secrets involved:
The PKCE verifier is transient secret material. It remains native, is not serializable or debuggable, is never included in command responses or logs, and receives a best-effort volatile overwrite when dropped. No real authorization code, access token, ID token or refresh token is handled by this batch.

Security tests completed:
Static review of transaction replacement, expiry, replay behavior, mismatch handling, verifier non-disclosure, best-effort memory clearing, probe isolation, diagnostic fields, unchanged CSP, and absence of network/persistence additions.

Remaining risks:
Windows compilation and runtime probe evidence are pending. The local SHA-256 implementation is deliberately restricted to PKCE S256 and covered by the RFC vector, but should receive independent review or be replaced with a vetted platform/library implementation before public production release. UUID-v4 randomness depends on the existing uuid crate's operating-system randomness path. Callback interception, authorization-code validation, token endpoint TLS, token signature/issuer/audience/expiry verification, ID-token nonce verification, refresh rotation, revocation, API allowlisting, server-side ownership, quotas, rate limits and audit logging are not implemented. Best-effort memory overwrite cannot protect against a process already running as the same OS user or copies made outside the wrapper.
```
