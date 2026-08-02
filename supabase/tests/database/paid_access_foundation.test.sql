begin;

create extension if not exists pgtap with schema extensions;

select plan(24);

select has_schema('private', 'private server-only schema exists');
select has_table('public', 'profiles', 'profiles table exists');
select has_table('public', 'billing_plans', 'billing plans table exists');
select has_table('private', 'billing_provider_prices', 'provider prices table exists');
select has_table('private', 'billing_customers', 'billing customers table exists');
select has_table('private', 'billing_checkouts', 'billing checkouts table exists');
select has_table('private', 'billing_subscriptions', 'billing subscriptions table exists');
select has_table('private', 'entitlements', 'entitlements table exists');
select has_table('private', 'billing_webhook_events', 'webhook event table exists');
select has_table('private', 'billing_audit_log', 'billing audit table exists');

select ok(
  (select relrowsecurity from pg_class where oid = 'public.profiles'::regclass),
  'profiles has RLS enabled'
);
select ok(
  (select relrowsecurity from pg_class where oid = 'public.billing_plans'::regclass),
  'billing plans has RLS enabled'
);
select ok(
  (select relrowsecurity from pg_class where oid = 'private.entitlements'::regclass),
  'private entitlements has RLS enabled'
);

select ok(
  not has_schema_privilege('authenticated', 'private', 'USAGE'),
  'authenticated users cannot access the private schema'
);
select ok(
  has_schema_privilege('service_role', 'private', 'USAGE'),
  'service role can access the private schema'
);
select ok(
  has_table_privilege('authenticated', 'public.profiles', 'SELECT'),
  'authenticated users can select profiles subject to RLS'
);
select ok(
  not has_table_privilege('anon', 'public.profiles', 'SELECT'),
  'anonymous users cannot read profiles'
);
select ok(
  has_column_privilege('authenticated', 'public.profiles', 'display_name', 'UPDATE'),
  'authenticated users may update display_name subject to RLS'
);
select ok(
  not has_column_privilege('authenticated', 'public.profiles', 'account_status', 'UPDATE'),
  'authenticated users cannot update account status'
);

select ok(
  exists (
    select 1
    from pg_policies
    where schemaname = 'public'
      and tablename = 'profiles'
      and policyname = 'profiles_select_own'
  ),
  'profiles own-row select policy exists'
);
select ok(
  exists (
    select 1
    from pg_policies
    where schemaname = 'public'
      and tablename = 'billing_plans'
      and policyname = 'billing_plans_read_active'
  ),
  'active-plan read policy exists'
);
select ok(
  exists (
    select 1
    from pg_indexes
    where schemaname = 'private'
      and tablename = 'billing_webhook_events'
      and indexdef like 'CREATE UNIQUE INDEX%'
      and indexdef like '%(provider, provider_event_id)%'
  ),
  'provider webhook event IDs are idempotent'
);
select ok(
  exists (
    select 1
    from pg_indexes
    where schemaname = 'private'
      and tablename = 'entitlements'
      and indexdef like 'CREATE UNIQUE INDEX%'
      and indexdef like '%(user_id, feature_key)%'
  ),
  'each user has at most one row per entitlement feature'
);
select ok(
  exists (
    select 1
    from pg_indexes
    where schemaname = 'private'
      and tablename = 'billing_checkouts'
      and indexname = 'billing_checkouts_provider_id_unique'
  ),
  'provider checkout IDs are unique when present'
);

select * from finish();
rollback;
