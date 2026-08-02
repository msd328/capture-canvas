# SaaS paid-access architecture — 2026-08-03

## Scope

This record defines the target architecture for a paid-only Recorder desktop product.
Anyone may register, download and install Recorder, but registration alone does not
unlock the recording application. Full desktop access is granted only after the SaaS
backend confirms an active `desktop_full_access` entitlement created from a verified
payment-provider event.

The selected target stack is:

- Supabase Auth for user registration and OIDC/OAuth 2.1 login;
- Supabase PostgreSQL for application, billing and entitlement data;
- a trusted Recorder SaaS API for authentication, billing, entitlement and upload
  operations;
- Stripe, PayPal and PhonePe provider adapters;
- provider-hosted checkout opened in the system browser; and
- a Tauri desktop access gate backed by server-confirmed entitlements.

## Canonical identity rule

The canonical account identifier is the immutable Supabase `auth.users.id` UUID,
derived from the verified access-token `sub` claim. Email is account/profile data only.
It may be used for login, receipts, display and support, but it must never be used to
prove that a user paid.

A payment belongs to a user through the following chain:

```text
verified Supabase JWT sub
        ↓
internal billing_checkouts.id
        ↓
provider checkout/order/subscription ID
        ↓
verified provider webhook event
        ↓
billing_subscriptions.user_id
        ↓
entitlements(user_id, desktop_full_access)
```

The desktop or browser must not submit a trusted `user_id` or email for billing. The
backend derives the user UUID from the verified access token.

## User workflow

```text
Download and install Recorder
        ↓
No secure session → Login / Register
        ↓
Supabase Auth system-browser OIDC + PKCE
        ↓
Backend verifies identity and loads /api/v1/me/access
        ↓
No desktop_full_access → payment-method selection
        ↓
Stripe / PayPal / PhonePe hosted checkout
        ↓
Provider webhook verified and processed idempotently
        ↓
Supabase subscription and entitlement updated
        ↓
Desktop refreshes /api/v1/me/access
        ↓
Full Recorder UI and native recording commands unlocked
```

A browser success redirect may tell the desktop to refresh status, but it must never
activate access by itself.

## Desktop access states

| State | Allowed experience |
|---|---|
| No session | Login and registration only |
| Authenticated, no entitlement | Account, payment selection, support and sign out |
| Checkout pending | Payment-pending and entitlement-refresh screen |
| Active `desktop_full_access` | Full Recorder, Library, cloud upload and sharing |
| Past due or grace period | Product-policy-dependent bounded access |
| Cancelled at period end | Access until the verified entitlement expiry |
| Expired, suspended or disabled | Paywall and account management only |

## Desktop routes

Target routes:

```text
/auth             Login and registration
/access-check     Session and entitlement resolution
/subscribe        Stripe / PayPal / PhonePe selection
/payment-pending  Bounded webhook-confirmation polling
/                 Paid Recorder route
/library          Paid local/cloud library route
/settings         Account plus paid recorder settings
/billing          Subscription and provider-management route
```

React guards are user-experience controls only. Native Rust recording commands must
also require a valid entitlement lease, and every cloud operation must be authorized
again by the SaaS API.

## Supabase data architecture

Use Supabase-managed `auth.users` for authentication identities and application-owned
schemas for product data.

### Public/authenticated profile data

```text
profiles
- user_id uuid primary key references auth.users(id)
- email text
- display_name text
- account_status text
- created_at timestamptz
- updated_at timestamptz
```

### Private billing data

The following tables must not be directly writable by the desktop client:

```text
billing_plans
billing_provider_prices
billing_customers
billing_checkouts
billing_subscriptions
entitlements
billing_webhook_events
billing_audit_events
```

Minimum ownership fields:

```text
billing_customers
- user_id
- provider
- provider_customer_id

billing_checkouts
- id (internal checkout UUID)
- user_id
- provider
- plan_key
- provider_checkout_id
- status
- expires_at

billing_subscriptions
- user_id
- provider
- provider_subscription_id
- provider_customer_id
- plan_key
- normalized_status
- current_period_start
- current_period_end
- cancel_at_period_end

entitlements
- user_id
- feature_key
- active
- source_provider
- source_subscription_id
- valid_until

billing_webhook_events
- provider
- provider_event_id
- event_type
- processing_status
- payload_hash
- received_at
- processed_at
```

`(provider, provider_event_id)` must be unique so webhook retries cannot apply the same
state transition twice.

## Supabase security boundary

- Enable RLS on every exposed application table.
- The desktop uses the anonymous/public Supabase client configuration only where
  explicitly allowed.
- Supabase service-role credentials remain server-only and must never be compiled into
  Tauri, React, installers or public environment variables.
- Billing, subscription and entitlement writes occur only through trusted backend
  code using verified user identity or verified provider webhooks.
- User-facing reads are owner-scoped by `auth.uid()` or served through the SaaS API.
- Database migrations, backups, point-in-time recovery and environment separation must
  be tracked before production.

## Billing provider architecture

Use one internal interface with three adapters:

```text
BillingProvider
- create_checkout
- verify_webhook
- normalize_event
- get_subscription
- cancel_subscription
- create_management_session (when supported)
```

Normalized provider values:

```text
provider: stripe | paypal | phonepe
subscription_status:
  pending | active | grace_period | past_due | suspended | cancelled | expired
```

All providers grant the same internal feature:

```text
desktop_full_access
```

### Stripe

- Stripe Checkout with subscription mode.
- Internal checkout ID carried through `client_reference_id` and metadata.
- Stripe Customer and Subscription IDs stored against the Supabase user UUID.
- Signature-verified webhooks update normalized subscription state.
- Customer Portal added for payment-method changes, invoices and cancellation.

### PayPal

- PayPal product and subscription plan.
- Internal checkout ID carried through the supported custom/external reference.
- Approval URL opened in the system browser.
- Verified PayPal webhook events update the normalized subscription and entitlement.

### PhonePe

- PhonePe hosted checkout for supported country/currency combinations.
- Internal checkout UUID used as the merchant order/reference ID.
- Provider order/transaction/subscription identifiers stored before redirect.
- Server-to-server webhook verification controls entitlement changes.
- Recurring UPI AutoPay support must be confirmed for the production merchant account;
  standard one-time checkout alone does not satisfy subscription renewal.

## Payment selection flow

The desktop obtains provider availability from the backend rather than hard-coding it:

```text
GET /api/v1/billing/providers
GET /api/v1/billing/plans
```

Availability can depend on country, currency, plan and provider onboarding. The user
selects one provider and the desktop sends:

```text
POST /api/v1/billing/checkout
Authorization: Bearer <verified Supabase access token>
{
  provider,
  planKey
}
```

The backend creates the internal checkout before contacting the provider and returns
only a short-lived hosted-checkout URL.

## Required API surface

```text
POST /api/v1/auth/exchange
POST /api/v1/auth/refresh
POST /api/v1/auth/logout

GET  /api/v1/me
GET  /api/v1/me/access

GET  /api/v1/billing/providers
GET  /api/v1/billing/plans
GET  /api/v1/billing/subscription
POST /api/v1/billing/checkout
POST /api/v1/billing/cancel
POST /api/v1/billing/manage

POST /api/v1/webhooks/stripe
POST /api/v1/webhooks/paypal
POST /api/v1/webhooks/phonepe
```

`GET /api/v1/me/access` is the source of truth for the desktop gate. It derives the
user from the verified token and returns normalized subscription and entitlement data.

## Offline access lease

For useful desktop behavior without creating unlimited offline access:

- issue a signed native-verifiable access lease after online entitlement validation;
- include the verified internal user ID, entitlement keys, issue time and expiry;
- keep the first production lease short, for example 24 hours;
- refresh periodically while online;
- block protected native commands after lease expiry until online revalidation; and
- support immediate server-side denial for cloud operations even while a local lease
  remains valid.

The exact signing key, algorithm, clock-skew policy and revocation strategy remain to
be selected and reviewed.

## Non-negotiable security rules

1. Never unlock from an email-address match.
2. Never trust a user ID supplied in a checkout request body.
3. Derive ownership from a verified Supabase JWT `sub` claim.
4. Create an internal checkout before provider redirect.
5. Store provider IDs against that checkout and user.
6. Verify Stripe, PayPal and PhonePe webhook authenticity.
7. Process webhooks idempotently and transactionally.
8. Never unlock from a browser success redirect.
9. Only trusted backend code may write subscription or entitlement state.
10. Require `desktop_full_access` in React, native Rust and SaaS API boundaries.
11. Keep provider secrets and the Supabase service-role key server-only.
12. Log identifiers and state transitions safely; never log tokens, card data or full
    webhook bodies by default.

## Implementation sequence

1. Complete Supabase OIDC code exchange and strict token validation.
2. Create Supabase development/staging/production projects and migration workflow.
3. Add profiles, plans, provider prices, customer mappings, checkout, subscription,
   entitlement and webhook-event tables.
4. Add RLS and server-only role policies.
5. Implement `/me/access` and the normalized entitlement model.
6. Add desktop AuthGate, subscribe and payment-pending routes.
7. Add a billing-provider interface and Stripe first.
8. Add Stripe signature-verified idempotent webhook processing.
9. Add native Rust entitlement enforcement and signed offline lease.
10. Add PayPal subscriptions and verified webhooks.
11. Add PhonePe hosted checkout/AutoPay after merchant capability confirmation.
12. Add cancellation, grace-period, refunds, chargebacks and reconciliation jobs.
13. Enable authenticated upload and sharing only after ownership and entitlement checks.

## Status

```text
Architecture decision  🟡  Target architecture and trust boundaries documented;
                             implementation and provider onboarding remain pending.
Database               🔵  Supabase schema, RLS and migration foundation are next.
Paid access            🔵  AuthGate, /me/access, paywall and native guard are next.
Stripe                 🔵  First payment-provider implementation.
PayPal                 🔵  Required second provider after common billing core.
PhonePe                🔵  Required India provider; recurring capability confirmation
                             remains a prerequisite.
```

## Security impact

```text
Security impact:
Documented the target authentication, database, payment, entitlement and desktop
access-control architecture. No runtime enforcement was added by this documentation
batch.

Data accessed:
Repository roadmap and existing architecture decisions only.

Data written:
Repository documentation only.

Network communication added:
None.

New permissions/capabilities:
None.

New dependencies:
None.

External processes:
None.

Untrusted inputs:
Future Supabase tokens, checkout requests, payment-provider redirects and webhooks are
documented as trust boundaries but are not processed by this batch.

Secrets involved:
No secret added. The design requires Supabase service-role and payment-provider secrets
to remain server-only.

Validation added:
Architecture review checklist, canonical identity/payment correlation rule, provider
webhook and entitlement invariants, RLS boundary and implementation sequence.

Security tests completed:
Documentation review only.

Remaining risks:
All described Supabase, billing, webhook, entitlement, offline-lease and native access
enforcement components remain unimplemented until their roadmap items are completed
and validated.
```
