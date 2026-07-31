# Bounded SaaS upload-session request boundary — 2026-07-31

## Scope

Hardened the reserved `POST /api/v1/upload-sessions` boundary before connecting authentication, metadata persistence, or object storage.

## Changes

- Enforced `POST` for upload-session creation and returns `405 method_not_allowed` with `Allow: POST, OPTIONS` for other methods.
- Requires `application/json` and rejects compressed request bodies.
- Rejects invalid, negative, mismatched, or oversized `Content-Length` values.
- Limits the actual streamed request body to 64 KiB even when `Content-Length` is absent or dishonest.
- Applies a fixed ten-second application-level body-read deadline and cancels the reader on timeout.
- Decodes JSON with fatal UTF-8 validation.
- Parses JSON without echoing malformed input in responses or logs.
- Applies the strict `CreateUploadSessionRequestSchema` contract.
- Enforces the configured recording-size limit after schema validation.
- Keeps the route fail-closed: unconfigured deployments return `503`, and configured deployments still return `501` until authenticated ownership checks, persistence, and signed storage URLs exist.
- Added structured error codes for method, media-type, and request-timeout rejection.

## Expected API behavior

```text
GET  /api/v1/upload-sessions                       -> 405 method_not_allowed
POST /api/v1/upload-sessions without JSON          -> 415 unsupported_media_type
POST with Content-Length > 65536                   -> 413 payload_too_large
POST whose streamed body exceeds 65536 bytes       -> 413 payload_too_large
POST whose body takes more than 10 seconds          -> 408 request_timeout
POST with malformed UTF-8 or JSON                   -> 400 bad_request
POST with unknown/missing/invalid contract fields   -> 400 bad_request
POST with a valid contract but no cloud config      -> 503 not_configured
POST with valid contract and placeholder config     -> 501 not_implemented
```

## Validation status

```text
SAAS-03  🟡  Upload-session request contract, byte limit and read deadline implemented; authenticated storage adapter and runtime tests pending
SEC-13   ⚪  Token verification, tenancy, ownership, authorization, quotas and rate limiting are still not implemented
```

## Security impact

```text
Security impact:
Reduced denial-of-service and parser ambiguity at the public upload-session control endpoint. The endpoint still cannot issue upload URLs or create cloud records.

Data accessed:
HTTP method, selected request headers, at most 64 KiB of request-body bytes, validated upload metadata, and provider-neutral capability configuration.

Data written:
None at runtime. Repository source, status documentation, and roadmap tracking are updated.

Network communication added:
None outbound. The existing same-origin API route now consumes bounded POST metadata when invoked.

New permissions/capabilities:
None.

External processes:
None.

Untrusted inputs:
Method, Content-Type, Content-Encoding, Content-Length, streamed request bytes, JSON values, UUIDs, title, duration, file size, SHA-256 digest, and visibility.

Validation added:
Method allowlist, media-type check, compression rejection, declared and actual byte limits, ten-second body deadline, Content-Length consistency, fatal UTF-8 decoding, JSON parsing, strict Zod validation, and configured upload-size enforcement.

Secrets involved:
None. Authorization headers and provider secrets are not read by this batch.

Security tests completed:
Static review of bounded byte accumulation, timeout cancellation, generic error responses, strict-schema rejection, configuration fail-closed behavior, and absence of upload URL generation or persistence.

Remaining risks:
No signed-token verification, issuer/audience validation, replay protection, owner-derived object key, metadata transaction, upload URL signing, rate limiting, quota accounting, audit trail, or deletion authorization exists yet. Hosting-level connection and header timeouts are still required in addition to the ten-second application body deadline. Windows/frontend production build and deployed API runtime validation remain pending.
```
