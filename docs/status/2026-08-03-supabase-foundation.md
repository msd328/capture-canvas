# Supabase paid-access foundation — 2026-08-03

## Scope

This batch establishes the first executable database and server-configuration boundary
for the accepted paid desktop architecture. It does not create a remote Supabase
project, contact Supabase, process a payment, validate a user token, or activate an
entitlement.

## Implemented database structure

Committed Supabase CLI layout:

```text
supabase/config.toml
supabase/migrations/20260803010000_paid_access_foundation.sql
supabase/migrations/20260803010100_harden_private_privileges.sql
supabase/tests/database/paid_access_foundation.test.sql
```

The migration creates:

```text
public.profiles
public.billing_plans
private.billing_provider_prices
private.billing_customers
private.billing_checkouts
private.billing_subscriptions
private.entitlements
private.billing_webhook_events
private.billing_audit_log
```

`auth.users.id` is the canonical ownership key for profiles, checkouts, subscriptions
and entitlements. Email is synchronized into the profile for account display/contact
purposes but is not used to prove payment ownership.

## Public and private boundaries

Direct Data API access is limited to:

- an authenticated user reading their own profile;
- an authenticated user updating only their own `display_name`; and
- anonymous/authenticated users reading active plan descriptions.

The `private` schema is not exposed in `supabase/config.toml`. Schema, table, sequence
and function privileges are revoked from `public`, `anon` and `authenticated`. Trusted
server code using the Supabase secret/service role receives the required privileges.
Private tables also have RLS enabled with no client policies.

## Payment correlation and idempotency

The schema supports the accepted correlation chain:

```text
verified auth.users.id
  ↔ private.billing_checkouts.id
  ↔ provider checkout/order ID
  ↔ provider subscription ID
  ↔ verified webhook event ID
  ↔ private.entitlements(user_id, feature_key)
```

Constraints include:

- one provider customer mapping per user/provider;
- globally unique provider customer, price, checkout and subscription identifiers;
- one entitlement row per user/feature;
- one webhook event row per provider/event ID;
- normalized provider and subscription statuses;
- bounded identifier formats and period ordering.

No production plan price, provider product identifier, subscription or entitlement is
seeded by this batch.

## Server configuration boundary

The SaaS server now recognizes an all-or-none Supabase configuration:

```text
RECORDER_SUPABASE_URL
RECORDER_SUPABASE_SECRET_KEY
RECORDER_AUTH_ISSUER
RECORDER_AUTH_AUDIENCE
```

Production URLs require HTTPS. Numeric loopback HTTP is accepted only for local
development. The issuer must use the same origin as the Supabase URL and the `/auth/v1`
path. Values with credentials, query strings, fragments, invalid audiences or malformed
secrets fail closed.

`GET /api/v1/capabilities` returns only readiness booleans:

```text
database.provider
database.configured
database.configurationValid
database.urlTrusted
database.authIssuerTrusted
database.audienceConfigured
database.serverSecretConfigured
```

It never returns the URL, issuer, audience or secret. Overall SaaS readiness now also
requires the database contract in addition to authentication and upload storage.

## Repository secret hygiene

Added `.env.example` as a name-only contract and updated `.gitignore` to exclude:

```text
.env
.env.*
supabase/.temp/
supabase/.branches/
```

`.env.example` remains tracked. Payment-provider credentials are shown only as commented
future names and are not consumed by application code.

## Validation

Added `scripts/supabase-local-check.ps1` to run:

```text
supabase start
supabase db reset
supabase test db
```

pgTAP tests verify the expected tables, RLS flags, private-schema access denial,
service-role access, profile column privileges, public policies and unique webhook,
entitlement and checkout indexes.

This environment cannot run Docker or the Supabase CLI, so migration execution and
pgTAP results remain pending on the user's Windows development machine.

## Roadmap status

```text
SAAS-18  🟡 Supabase PostgreSQL/RLS foundation implemented; local and remote validation pending.
SEC-16   🟡 Private-schema, RLS, grant and server-secret isolation implemented; negative tests pending execution.
DB-01    🟡 Local project layout and migration workflow implemented; remote environment linking remains.
DB-02–12 🟡 Schema, identity, billing, entitlement, idempotency and RLS controls implemented; validation pending.
DB-13–14 ⚪ Backup/recovery and deletion lifecycle remain planned.
```

## Security impact

```text
Security impact:
Added a least-privilege Supabase schema and server-only configuration boundary for
future authentication, billing and entitlement enforcement.

Data accessed:
At migration time, Supabase auth.users IDs, emails and created timestamps are read to
create or synchronize profile rows. At runtime, server environment configuration is
read only to produce readiness booleans or future trusted-server configuration.

Data written:
Versioned SQL migrations, local Supabase configuration, pgTAP tests, public profile and
plan tables, private billing/entitlement/webhook/audit tables, repository documentation
and environment-name examples.

Network communication added:
None. No Supabase, Stripe, PayPal, PhonePe, storage or other remote endpoint is contacted.
The local validation script may start local Docker services only when explicitly run.

New permissions/capabilities:
Database grants only. Authenticated users receive own-profile SELECT and display_name-only
UPDATE plus active-plan SELECT. Trusted service_role receives private-schema access.
No Tauri capability or desktop permission was added.

External processes:
None during normal application execution. The explicit local validation script invokes
the Supabase CLI and Docker-backed local services.

Untrusted inputs:
Server environment strings, future auth.users profile values, future provider IDs,
checkout/subscription states and webhook identifiers. No webhook body is accepted yet.

Validation added:
URL/issuer/audience/secret bounds, same-origin issuer check, production HTTPS with
numeric-loopback development exception, SQL constraints, foreign keys, RLS, column-level
grants, private-schema revocation, unique provider/event identifiers and pgTAP tests.

Secrets involved:
RECORDER_SUPABASE_SECRET_KEY is defined as a server-only environment contract. No value
is committed, logged, returned by capabilities, written to the database or exposed to
the desktop/frontend.

Security tests completed:
Static review of schema ownership, grants, policies, constraints, configuration output
and repository ignore rules. Supabase migration execution and pgTAP negative tests remain
pending because local Docker/Supabase execution was unavailable in this environment.

Remaining risks:
Remote development/staging/production projects are not created or linked. Migration and
RLS tests have not executed. Token signature/claim validation, /me/access, provider
webhook verification, transactional entitlement transitions, rate limits, reconciliation,
backup/restore, deletion lifecycle, native entitlement enforcement and payment-provider
secret management remain unimplemented.
```
