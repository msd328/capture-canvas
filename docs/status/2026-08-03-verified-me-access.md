# Verified Supabase account-access boundary

Date: 2026-08-03

## Scope

This batch implements the first authoritative paid-access read path. It verifies a
Supabase OAuth access token against compile-time/server-pinned trust metadata, derives
the account exclusively from the verified `sub` UUID, and queries a parameterless
Supabase RPC for the caller's normalized subscription and entitlement state.

It does not exchange an authorization code, persist a refresh token, activate an
entitlement, process a payment, unlock the Recorder UI, or authorize a native recording
command.

## API

```text
GET /api/v1/me/access
Authorization: Bearer <Supabase OAuth access token>
```

The request has no body and cannot include a trusted user ID or email address. The
backend derives ownership only from the cryptographically verified access-token `sub`
claim.

A successful response follows this shape:

```json
{
  "userId": "11111111-1111-4111-8111-111111111111",
  "authenticated": true,
  "accountStatus": "active",
  "subscription": {
    "provider": "stripe",
    "status": "active",
    "currentPeriodEnd": "2026-09-03T00:00:00.000Z"
  },
  "entitlements": ["desktop_full_access"],
  "fullAccess": true
}
```

An authenticated but unpaid user receives an empty entitlement list and
`fullAccess=false`.

## Access-token trust contract

The server requires all of the following:

```text
RECORDER_SUPABASE_URL
RECORDER_SUPABASE_SECRET_KEY
RECORDER_AUTH_ISSUER
RECORDER_AUTH_AUDIENCE
RECORDER_AUTH_JWKS_URI
RECORDER_AUTH_CLIENT_ID
RECORDER_AUTH_ALLOWED_ALGORITHMS
```

Production URLs require HTTPS. Numeric-loopback HTTP is allowed only for local
Supabase development. The JWKS URI must share the configured issuer origin and use the
exact `/auth/v1/.well-known/jwks.json` path.

The verifier accepts only explicitly pinned asymmetric algorithms currently supported
by this implementation:

```text
RS256
ES256
```

HS256 is deliberately unsupported because the SaaS API must not receive or distribute
the Supabase JWT signing secret.

## JWT validation

Before the access RPC runs, the verifier requires:

- exactly three bounded base64url JWT segments;
- fatal UTF-8 and valid JSON decoding;
- no duplicate top-level header or claim names;
- only `alg`, `kid`, and optional `typ` header fields;
- one approved signing algorithm;
- one unique matching JWKS `kid` and algorithm;
- a signing-use key with compatible RSA or P-256 material;
- a valid WebCrypto signature;
- exact issuer, audience, OAuth client ID, and `authenticated` role;
- a canonical UUID subject;
- valid `iat`, optional `nbf`, and `exp` values;
- no access-token lifetime longer than 24 hours;
- a 60-second maximum clock-skew allowance.

The JWKS response is limited to 64 KiB and sixteen keys. The complete request,
including streamed response-body reads, has a five-second deadline. A successful set is
cached in memory for five minutes and refreshed once when a key is missing, rotated, or
a signature does not verify.

## Owner-derived database RPC

The new function is:

```sql
public.get_my_access()
```

It accepts no arguments. `auth.uid()` is the only user selector. The function returns:

```text
account_status
subscription_provider
subscription_status
current_period_end
entitlements
full_access
```

`full_access` is true only when:

```text
account_status = active
AND
an active desktop_full_access entitlement exists
AND
valid_from is not in the future
AND
valid_until is absent or still in the future
```

The RPC is `SECURITY DEFINER`, uses an empty search path, is stable, rejects missing
authentication, and is executable only by `authenticated` and `service_role`. The
private billing tables remain unavailable for direct client reads.

The SaaS server supplies its secret key only as the Supabase gateway API key. The
original user bearer token remains the authorization context so `auth.uid()` resolves
to the verified caller rather than the service role.

## Failure behavior

```text
Missing or malformed Authorization header       → 401 not_authenticated
Invalid signature or token claims               → 401 not_authenticated
Incomplete server trust configuration           → 503 not_configured
Supabase RPC timeout/invalid response/failure    → 502 internal_error
Unsupported method                              → 405 method_not_allowed
```

Detailed JWT, provider, token, user, email, endpoint, and Supabase response values are
not returned in errors or written to structured health logs.

## Diagnostics

Path- and token-free examples:

```text
[Recorder][SaasHealth] stage=me_access ok=false code=not_authenticated
[Recorder][SaasHealth] stage=me_access ok=false code=invalid_token
[Recorder][SaasHealth] stage=me_access ok=false code=not_configured
[Recorder][SaasHealth] stage=me_access ok=false code=access_lookup_failed
[Recorder][SaasHealth] stage=me_access ok=true full_access=false entitlement_count=0
```

## Tests added

The pgTAP suite checks:

- the RPC exists and accepts zero arguments;
- it is stable and security-definer;
- authenticated may execute it while anonymous may not;
- unauthenticated execution fails closed;
- the authenticated account status is returned;
- a current desktop entitlement grants access;
- another user's entitlement cannot cross the owner boundary;
- disabled accounts cannot receive access;
- expired entitlements cannot receive access.

Frontend TypeScript compilation, local Supabase migration execution, WebCrypto runtime,
remote JWKS rotation, real OAuth tokens, and hosted CI remain validation-pending.

## Security impact

```text
Security impact:
Added cryptographic access-token verification and an owner-derived paid-access read
boundary. No entitlement write or product unlock was added.

Data accessed:
JWT header and claims, public JWKS keys, the authenticated user's profile,
subscription summary, and current entitlement keys.

Data written:
In-memory JWKS cache entries only. No user, billing, token, or entitlement record is
created or modified by the endpoint.

Network communication added:
Configured SaaS server only: bounded HTTPS requests to the pinned Supabase JWKS and
REST RPC origins. Numeric-loopback HTTP is permitted only for local development.

New permissions/capabilities:
Authenticated execute permission for the parameterless public.get_my_access() RPC.
No Tauri permission or desktop capability was added.

External processes:
None.

Untrusted inputs:
Authorization header, JWT segments/JSON/claims/signature, JWKS HTTP metadata/body/key
fields, and Supabase RPC HTTP metadata/body.

Validation added:
Header/token/response limits, five-second network deadlines, no redirects, exact
issuer/audience/client ID/role/time checks, approved algorithms, one matching JWK,
WebCrypto signature verification, strict Zod response parsing, owner-derived auth.uid(),
and server-side recomputation of fullAccess.

Secrets involved:
The server-only Supabase secret key is sent only as the API gateway key to the pinned
Supabase origin. It is not returned, logged, embedded in Tauri/React, or used as the RPC
authorization identity. User access tokens are not logged or persisted by this batch.

Security tests completed:
Static implementation review and compile-oriented WebCrypto type review. pgTAP,
frontend build, real-token verification, network timeout, key rotation, cross-user
runtime, and hosted CI execution remain pending.

Remaining risks:
Native code exchange and ID-token nonce validation, refresh-token rotation/revocation,
rate limits, payment-webhook authenticity, entitlement writes, offline leases, AuthGate,
native command enforcement, reconciliation, remote Supabase configuration, and
penetration testing remain unimplemented.
```
