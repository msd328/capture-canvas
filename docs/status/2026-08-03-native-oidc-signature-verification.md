# Native OIDC ID-token signature verification

Date: 2026-08-03
Status: Implemented; Windows compilation and configured-provider validation pending
Roadmap: SAAS-01, SEC-12, HLT-30, REL-14

## Delivered

Recorder now contains a private native ID-token verification boundary that:

- resolves the exact RS256 or ES256 public key through the bounded, pinned JWKS resolver;
- verifies the original compact-JWS `header.payload` bytes rather than a reserialized representation;
- verifies RS256 with RSA PKCS#1 v1.5 and SHA-256 using a 2048–4096-bit validated modulus;
- verifies ES256 with a validated P-256 point and the required 64-byte fixed `r || s` signature format;
- rejects missing, malformed, padded, oversized or additional compact-JWT signature segments;
- validates issuer, audience/authorized party, canonical UUID subject, expiration, issue time, not-before, authentication time and the authorization nonce only after signature acceptance;
- promotes claims to a `VerifiedNativeIdentity` that contains only the internal subject and bounded timestamps;
- retains all access, refresh and ID-token bytes in native-only, unpersisted state.

The verifier is crate-private. It is not registered as a Tauri command, is not callable from React, does not establish a session and cannot grant `desktop_full_access`.

## Dependency boundary

`ring = 0.17.14` is declared explicitly for maintained RS256/ES256 verification. The same version was already present in the committed lockfile through the Rustls stack, so no registry package version, source or checksum is intentionally changed. Cargo must still regenerate the root `recorder` dependency list once after this manifest change before `cargo --locked` can pass.

## Tests

Native unit coverage includes:

- a fixed valid 2048-bit RS256 vector;
- rejection after payload tampering;
- a fixed valid P-256 ES256 `r || s` vector;
- rejection with a different P-256 public key;
- rejection of empty, padded and incorrectly sized signature segments.

The static vectors were independently checked against standard RSA PKCS#1 v1.5/SHA-256 and P-256 ECDSA/SHA-256 implementations before being committed.

## Security impact

Data read:
- one unverified ID token;
- the expected native nonce;
- one already validated JWKS public key.

Data written:
- a short-lived `VerifiedNativeIdentity` value in native memory;
- repository source, dependency metadata and documentation.

Network added:
- none beyond the previously implemented bounded token and JWKS transports.

Persistent secrets added:
- none.

Tauri commands or capabilities added:
- none.

Logs:
- typed verification stages and booleans only;
- no token, signature, key material, nonce, subject, issuer or audience value.

## Deliberately still blocked

- the authorization callback does not yet invoke the live token exchange;
- an exchanged token bundle is not yet passed through this verifier;
- no access token is held as a native session;
- no refresh token is written to Windows Credential Manager;
- no refresh rotation, revocation or logout lifecycle exists;
- the desktop does not yet call `/api/v1/me/access`;
- no AuthGate, payment paywall or native entitlement guard is enabled.

## Validation required

Run on Windows after pulling the batch:

```powershell
cd src-tauri
cargo generate-lockfile
cargo fmt --all
cd ..

.\scripts\windows-local-check.ps1
```

The roadmap items remain yellow until formatting, tests, `cargo check --locked`, and configured Supabase RS256/ES256 behavior are evidenced.
