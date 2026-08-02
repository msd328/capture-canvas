# Capture Canvas Supabase foundation

This directory contains the versioned database definition for the paid desktop and
SaaS product. Database changes must be committed as migrations; do not make untracked
production schema changes through the remote Dashboard.

## Local prerequisites

- Supabase CLI on `PATH`.
- Docker Desktop or another Docker-compatible runtime.
- PowerShell for the repository validation helper.

## Local validation

From the repository root:

```powershell
.\scripts\supabase-local-check.ps1
```

The script:

1. starts the local Supabase stack;
2. resets the local database from committed migrations; and
3. runs the pgTAP tests in `supabase/tests/database`.

A successful run ends with:

```text
[SupabaseCheck] Local database validation passed
```

Stop the local services when they are no longer needed:

```powershell
supabase stop
```

## Environment separation

Use separate Supabase projects for development, staging, and production. Link a local
checkout only to the intended remote project and never commit `supabase/.temp`, project
references, database passwords, secret keys, provider credentials, or populated `.env`
files.

Recommended deployment flow:

```text
feature branch migration
  → local db reset and pgTAP tests
  → code review
  → staging db push and integration tests
  → approved production db push
```

Only one coordinated deployment actor should push migrations to a shared environment at
a time.

## Identity boundary

`auth.users.id` is the canonical account identifier. The migration creates
`public.profiles.user_id` and all private billing ownership references against that UUID.
Email remains contact/profile data and is never used to match a payment to a user.

## Exposure boundary

The generated Data API exposes the configured public schemas. The `private` schema is
not listed in `supabase/config.toml` and explicitly revokes access from `anon` and
`authenticated`.

Direct client access is limited to:

- reading the signed-in user's own profile;
- updating that user's `display_name` only; and
- reading active public plan descriptions.

Provider price IDs, customer mappings, checkouts, subscriptions, entitlements, webhook
events and audit records are server-only.

## Server secrets

Use `.env.example` as the name contract. `RECORDER_SUPABASE_SECRET_KEY` is accepted only
by the SaaS server configuration module and must be supplied by a secret manager or
uncommitted local environment file. It must never be embedded in the Tauri executable,
React bundle, installer, logs, screenshots, or public CI output.

## Current scope

Implemented:

- local CLI configuration;
- paid-access migration;
- profile synchronization from `auth.users`;
- plan, provider-price, customer, checkout, subscription, entitlement, webhook-event and
  audit tables;
- RLS/grants/private-schema isolation;
- migration structure and privilege tests;
- server readiness booleans without secret disclosure.

Not implemented:

- remote Supabase project creation/linking;
- provider prices or production plan seeding;
- token exchange/JWT verification;
- `/api/v1/me/access`;
- Stripe, PayPal or PhonePe calls/webhooks;
- entitlement activation;
- backup/restore validation;
- account deletion orchestration.
