-- Capture Canvas paid-access database foundation.
--
-- Public API exposure is intentionally narrow:
--   * authenticated users may read their own profile and update display_name only;
--   * anyone may read active product-plan descriptions;
--   * all billing, checkout, subscription, entitlement, webhook and audit tables live
--     in the unexposed private schema and are writable only by trusted server code.
--
-- Payment ownership is keyed by auth.users.id. Email is contact/profile data and is
-- never used to prove that a payment belongs to an account.

create schema if not exists private;

revoke all on schema private from public;
revoke all on schema private from anon;
revoke all on schema private from authenticated;
grant usage on schema private to service_role;

create or replace function private.set_updated_at()
returns trigger
language plpgsql
set search_path = ''
as $$
begin
  new.updated_at = now();
  return new;
end;
$$;

create table if not exists public.profiles (
  user_id uuid primary key references auth.users(id) on delete cascade,
  email text,
  display_name text,
  account_status text not null default 'active',
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  constraint profiles_display_name_length check (
    display_name is null
    or char_length(btrim(display_name)) between 1 and 120
  ),
  constraint profiles_account_status check (
    account_status in ('active', 'disabled', 'deletion_pending')
  )
);

create table if not exists public.billing_plans (
  id uuid primary key default gen_random_uuid(),
  plan_key text not null unique,
  name text not null,
  billing_interval text not null,
  active boolean not null default true,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  constraint billing_plans_key_format check (
    plan_key ~ '^[a-z][a-z0-9_]{2,63}$'
  ),
  constraint billing_plans_name_length check (
    char_length(btrim(name)) between 1 and 120
  ),
  constraint billing_plans_interval check (
    billing_interval in ('month', 'year')
  )
);

create table if not exists private.billing_provider_prices (
  id uuid primary key default gen_random_uuid(),
  plan_id uuid not null references public.billing_plans(id) on delete restrict,
  provider text not null,
  provider_product_id text,
  provider_price_id text not null,
  currency text not null,
  amount_minor bigint not null,
  active boolean not null default true,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  constraint billing_provider_prices_provider check (
    provider in ('stripe', 'paypal', 'phonepe')
  ),
  constraint billing_provider_prices_currency check (
    currency ~ '^[A-Z]{3}$'
  ),
  constraint billing_provider_prices_amount check (amount_minor > 0),
  constraint billing_provider_prices_external_id_length check (
    char_length(provider_price_id) between 1 and 255
    and (
      provider_product_id is null
      or char_length(provider_product_id) between 1 and 255
    )
  ),
  unique (provider, provider_price_id),
  unique (plan_id, provider, currency)
);

create table if not exists private.billing_customers (
  id uuid primary key default gen_random_uuid(),
  user_id uuid not null references auth.users(id) on delete cascade,
  provider text not null,
  provider_customer_id text not null,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  constraint billing_customers_provider check (
    provider in ('stripe', 'paypal', 'phonepe')
  ),
  constraint billing_customers_external_id_length check (
    char_length(provider_customer_id) between 1 and 255
  ),
  unique (user_id, provider),
  unique (provider, provider_customer_id)
);

create table if not exists private.billing_checkouts (
  id uuid primary key default gen_random_uuid(),
  user_id uuid not null references auth.users(id) on delete cascade,
  plan_id uuid not null references public.billing_plans(id) on delete restrict,
  provider text not null,
  provider_checkout_id text,
  status text not null default 'created',
  currency text not null,
  amount_minor bigint not null,
  expires_at timestamptz,
  completed_at timestamptz,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  constraint billing_checkouts_provider check (
    provider in ('stripe', 'paypal', 'phonepe')
  ),
  constraint billing_checkouts_status check (
    status in ('created', 'pending', 'completed', 'expired', 'cancelled', 'failed')
  ),
  constraint billing_checkouts_currency check (currency ~ '^[A-Z]{3}$'),
  constraint billing_checkouts_amount check (amount_minor > 0),
  constraint billing_checkouts_external_id_length check (
    provider_checkout_id is null
    or char_length(provider_checkout_id) between 1 and 255
  )
);

create unique index if not exists billing_checkouts_provider_id_unique
  on private.billing_checkouts (provider, provider_checkout_id)
  where provider_checkout_id is not null;

create index if not exists billing_checkouts_user_created_idx
  on private.billing_checkouts (user_id, created_at desc);

create table if not exists private.billing_subscriptions (
  id uuid primary key default gen_random_uuid(),
  user_id uuid not null references auth.users(id) on delete cascade,
  plan_id uuid not null references public.billing_plans(id) on delete restrict,
  billing_customer_id uuid references private.billing_customers(id) on delete restrict,
  source_checkout_id uuid references private.billing_checkouts(id) on delete set null,
  provider text not null,
  provider_subscription_id text not null,
  status text not null,
  current_period_start timestamptz,
  current_period_end timestamptz,
  cancel_at_period_end boolean not null default false,
  cancelled_at timestamptz,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  constraint billing_subscriptions_provider check (
    provider in ('stripe', 'paypal', 'phonepe')
  ),
  constraint billing_subscriptions_status check (
    status in (
      'pending',
      'active',
      'grace_period',
      'past_due',
      'suspended',
      'cancelled',
      'expired'
    )
  ),
  constraint billing_subscriptions_external_id_length check (
    char_length(provider_subscription_id) between 1 and 255
  ),
  constraint billing_subscriptions_period_order check (
    current_period_start is null
    or current_period_end is null
    or current_period_end >= current_period_start
  ),
  unique (provider, provider_subscription_id)
);

create index if not exists billing_subscriptions_user_status_idx
  on private.billing_subscriptions (user_id, status, current_period_end desc);

create table if not exists private.entitlements (
  id uuid primary key default gen_random_uuid(),
  user_id uuid not null references auth.users(id) on delete cascade,
  feature_key text not null,
  active boolean not null default false,
  source_provider text,
  source_subscription_id uuid references private.billing_subscriptions(id) on delete set null,
  valid_from timestamptz not null default now(),
  valid_until timestamptz,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  constraint entitlements_feature_key_format check (
    feature_key ~ '^[a-z][a-z0-9_]{2,63}$'
  ),
  constraint entitlements_source_provider check (
    source_provider is null
    or source_provider in ('stripe', 'paypal', 'phonepe', 'admin', 'migration')
  ),
  constraint entitlements_validity_order check (
    valid_until is null or valid_until >= valid_from
  ),
  unique (user_id, feature_key)
);

create index if not exists entitlements_user_active_idx
  on private.entitlements (user_id, active, valid_until desc);

create table if not exists private.billing_webhook_events (
  id uuid primary key default gen_random_uuid(),
  provider text not null,
  provider_event_id text not null,
  event_type text not null,
  payload_sha256 text,
  processing_status text not null default 'received',
  last_error_code text,
  received_at timestamptz not null default now(),
  processed_at timestamptz,
  constraint billing_webhook_events_provider check (
    provider in ('stripe', 'paypal', 'phonepe')
  ),
  constraint billing_webhook_events_status check (
    processing_status in ('received', 'processed', 'ignored', 'failed')
  ),
  constraint billing_webhook_events_external_id_length check (
    char_length(provider_event_id) between 1 and 255
    and char_length(event_type) between 1 and 160
  ),
  constraint billing_webhook_events_payload_hash check (
    payload_sha256 is null or payload_sha256 ~ '^[a-f0-9]{64}$'
  ),
  constraint billing_webhook_events_error_code_length check (
    last_error_code is null or char_length(last_error_code) between 1 and 80
  ),
  unique (provider, provider_event_id)
);

create index if not exists billing_webhook_events_status_received_idx
  on private.billing_webhook_events (processing_status, received_at);

create table if not exists private.billing_audit_log (
  id bigint generated always as identity primary key,
  user_id uuid references auth.users(id) on delete set null,
  provider text,
  entity_type text not null,
  entity_id text not null,
  action text not null,
  previous_status text,
  new_status text,
  request_id uuid,
  created_at timestamptz not null default now(),
  constraint billing_audit_log_provider check (
    provider is null or provider in ('stripe', 'paypal', 'phonepe', 'internal')
  ),
  constraint billing_audit_log_field_lengths check (
    char_length(entity_type) between 1 and 80
    and char_length(entity_id) between 1 and 255
    and char_length(action) between 1 and 120
  )
);

create index if not exists billing_audit_log_user_created_idx
  on private.billing_audit_log (user_id, created_at desc);

create or replace function private.sync_auth_user_profile()
returns trigger
language plpgsql
security definer
set search_path = ''
as $$
begin
  insert into public.profiles (user_id, email, created_at, updated_at)
  values (new.id, new.email, coalesce(new.created_at, now()), now())
  on conflict (user_id) do update
    set email = excluded.email,
        updated_at = now();
  return new;
end;
$$;

drop trigger if exists auth_user_profile_insert on auth.users;
create trigger auth_user_profile_insert
after insert on auth.users
for each row execute function private.sync_auth_user_profile();

drop trigger if exists auth_user_profile_email_update on auth.users;
create trigger auth_user_profile_email_update
after update of email on auth.users
for each row execute function private.sync_auth_user_profile();

insert into public.profiles (user_id, email, created_at, updated_at)
select id, email, created_at, now()
from auth.users
on conflict (user_id) do update
  set email = excluded.email,
      updated_at = now();

drop trigger if exists profiles_set_updated_at on public.profiles;
create trigger profiles_set_updated_at
before update on public.profiles
for each row execute function private.set_updated_at();

drop trigger if exists billing_plans_set_updated_at on public.billing_plans;
create trigger billing_plans_set_updated_at
before update on public.billing_plans
for each row execute function private.set_updated_at();

drop trigger if exists billing_provider_prices_set_updated_at on private.billing_provider_prices;
create trigger billing_provider_prices_set_updated_at
before update on private.billing_provider_prices
for each row execute function private.set_updated_at();

drop trigger if exists billing_customers_set_updated_at on private.billing_customers;
create trigger billing_customers_set_updated_at
before update on private.billing_customers
for each row execute function private.set_updated_at();

drop trigger if exists billing_checkouts_set_updated_at on private.billing_checkouts;
create trigger billing_checkouts_set_updated_at
before update on private.billing_checkouts
for each row execute function private.set_updated_at();

drop trigger if exists billing_subscriptions_set_updated_at on private.billing_subscriptions;
create trigger billing_subscriptions_set_updated_at
before update on private.billing_subscriptions
for each row execute function private.set_updated_at();

drop trigger if exists entitlements_set_updated_at on private.entitlements;
create trigger entitlements_set_updated_at
before update on private.entitlements
for each row execute function private.set_updated_at();

alter table public.profiles enable row level security;
alter table public.billing_plans enable row level security;
alter table private.billing_provider_prices enable row level security;
alter table private.billing_customers enable row level security;
alter table private.billing_checkouts enable row level security;
alter table private.billing_subscriptions enable row level security;
alter table private.entitlements enable row level security;
alter table private.billing_webhook_events enable row level security;
alter table private.billing_audit_log enable row level security;

drop policy if exists profiles_select_own on public.profiles;
create policy profiles_select_own
on public.profiles
for select
to authenticated
using ((select auth.uid()) = user_id);

drop policy if exists profiles_update_own on public.profiles;
create policy profiles_update_own
on public.profiles
for update
to authenticated
using ((select auth.uid()) = user_id)
with check ((select auth.uid()) = user_id);

drop policy if exists billing_plans_read_active on public.billing_plans;
create policy billing_plans_read_active
on public.billing_plans
for select
to anon, authenticated
using (active);

revoke all on public.profiles from anon, authenticated;
grant select on public.profiles to authenticated;
grant update (display_name) on public.profiles to authenticated;
grant all on public.profiles to service_role;

revoke all on public.billing_plans from anon, authenticated;
grant select on public.billing_plans to anon, authenticated;
grant all on public.billing_plans to service_role;

grant all on all tables in schema private to service_role;
grant all on all sequences in schema private to service_role;
grant execute on all functions in schema private to service_role;

alter default privileges for role postgres in schema private
  revoke all on tables from public, anon, authenticated;
alter default privileges for role postgres in schema private
  revoke all on sequences from public, anon, authenticated;
alter default privileges for role postgres in schema private
  revoke all on functions from public, anon, authenticated;
alter default privileges for role postgres in schema private
  grant all on tables to service_role;
alter default privileges for role postgres in schema private
  grant all on sequences to service_role;
alter default privileges for role postgres in schema private
  grant execute on functions to service_role;
