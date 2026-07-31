# SaaS foundation boundary — 2026-07-31

## Scope

Established the first fail-closed SaaS boundary without selecting an authentication, database, deployment, or object-storage vendor.

## Changes

- Added versioned shared TypeScript/Zod contracts for authenticated users, cloud recording metadata, recording visibility, upload-session creation, upload-session responses, capability discovery, and structured API errors.
- Added strict limits for title length, MP4 content type, UUID identifiers, duration, SHA-256 digests, API request size, and a 20 GiB absolute recording-upload ceiling.
- Added `GET /api/v1/health`.
- Added `GET /api/v1/capabilities`.
- Added fail-closed environment detection for OIDC issuer/audience and object-storage origin/bucket configuration.
- Capability responses expose booleans and limits only; provider URLs, audience values, bucket names, tokens, and secrets are not returned.
- Added a reserved `/api/v1/upload-sessions` boundary that returns `503 not_configured` until both authentication and storage are configured, then returns `501 not_implemented` until authenticated upload creation is implemented.
- Routed only `/api/v1` requests through the SaaS handler. All other requests continue through the existing TanStack Start SSR entry.
- Preserved the existing Windows Credential Manager status/probe/clear implementation already present on the branch. No real account token is accepted or returned by the current public Tauri command surface.

## Runtime contract

Without SaaS environment configuration:

```text
GET /api/v1/health         -> 200 { apiVersion: "v1", status: "ok" }
GET /api/v1/capabilities   -> 200 configured=false
POST /api/v1/upload-sessions -> 503 not_configured
```

Required future environment names are deliberately provider-neutral:

```text
RECORDER_AUTH_ISSUER
RECORDER_AUTH_AUDIENCE
RECORDER_UPLOAD_ORIGIN
RECORDER_UPLOAD_BUCKET
RECORDER_MAX_UPLOAD_BYTES   optional, clamped to 1 MiB–20 GiB
```

An issuer or upload origin is considered configured only when it is a valid HTTPS URL.

## Validation status

```text
SAAS-01  🟡  Authentication protocol and capability contract present; provider integration and login flow pending
SAAS-02  🟡  Windows Credential Manager boundary present; Windows probe/build validation and real token exchange pending
SAAS-03  🟡  Resumable upload-session request/response contract present; authenticated storage adapter pending
SAAS-06  🟡  Private/unlisted/public contract present; server-side authorization enforcement pending
SAAS-08  🟡  Cloud recording metadata contract present; persistence and cloud library pending
SEC-12   🟡  OS secure-store boundary present; Windows runtime probe and production token lifecycle pending
SEC-13   ⚪  Server-side tenancy, authorization, and rate-limit implementation not yet present
```

## Security impact

```text
Security impact:
Added a versioned public capability/health API and shared SaaS data contracts. The API fails closed for upload creation and does not authenticate users, issue tokens, accept media, or write cloud data yet.

Data accessed:
Provider-neutral environment configuration names and values required only to determine configured/not-configured state. Existing SSR requests continue unchanged.

Data written:
None at runtime. Repository source, contracts, status documentation, and roadmap tracking were updated.

Network communication added:
Two same-origin read-only HTTP endpoints are now available when the server runtime is deployed. No outbound network request was added.

New permissions/capabilities:
None. No Tauri capability or plugin change.

External processes:
None.

Untrusted inputs:
Request method and URL path for `/api/v1`. No request body is consumed by the current implementation. Future upload-session bodies must be bounded before parsing.

Validation added:
HTTPS-only configuration detection, strict Zod domain limits, structured error identifiers, no-store JSON responses, `nosniff`, and fail-closed upload-session behavior.

Secrets involved:
Environment values may contain deployment configuration, but their values are never returned. No account token or storage credential is accepted by the new endpoints.

Security tests completed:
Static review of SSR route isolation, environment-value non-disclosure, fail-closed configuration behavior, schema limits, and absence of outbound network or persistence.

Remaining risks:
The endpoints need production build/runtime validation. Authentication signature/issuer/audience verification, PKCE callback handling, refresh-token lifecycle, database tenancy, upload URL signing, object-key ownership, rate limiting, quotas, replay protection, request-body limits, audit logging, and deletion authorization are not implemented. The existing Windows Credential Manager implementation remains pending Windows compile/probe evidence.
```
