begin;

create extension if not exists pgtap with schema extensions;

select plan(13);

select has_function(
  'public',
  'get_my_access',
  array[]::name[],
  'parameterless paid-access RPC exists'
);
select is(
  (
    select pronargs::integer
    from pg_proc
    where oid = 'public.get_my_access()'::regprocedure
  ),
  0,
  'paid-access RPC accepts no user identifier'
);
select ok(
  (
    select prosecdef
    from pg_proc
    where oid = 'public.get_my_access()'::regprocedure
  ),
  'paid-access RPC is security definer'
);
select is(
  (
    select provolatile
    from pg_proc
    where oid = 'public.get_my_access()'::regprocedure
  ),
  's'::"char",
  'paid-access RPC is stable'
);
select ok(
  has_function_privilege('authenticated', 'public.get_my_access()', 'EXECUTE'),
  'authenticated users may execute the paid-access RPC'
);
select ok(
  not has_function_privilege('anon', 'public.get_my_access()', 'EXECUTE'),
  'anonymous users cannot execute the paid-access RPC'
);
select throws_ok(
  'select * from public.get_my_access()',
  '42501',
  'Authentication is required.',
  'the RPC fails closed without an authenticated subject'
);

insert into auth.users (
  instance_id,
  id,
  aud,
  role,
  email,
  encrypted_password,
  email_confirmed_at,
  raw_app_meta_data,
  raw_user_meta_data,
  created_at,
  updated_at
)
values
  (
    '00000000-0000-0000-0000-000000000000',
    '11111111-1111-4111-8111-111111111111',
    'authenticated',
    'authenticated',
    'first@example.test',
    '',
    now(),
    '{}'::jsonb,
    '{}'::jsonb,
    now(),
    now()
  ),
  (
    '00000000-0000-0000-0000-000000000000',
    '22222222-2222-4222-8222-222222222222',
    'authenticated',
    'authenticated',
    'second@example.test',
    '',
    now(),
    '{}'::jsonb,
    '{}'::jsonb,
    now(),
    now()
  );

insert into private.entitlements (
  user_id,
  feature_key,
  active,
  source_provider,
  valid_from,
  valid_until
)
values
  (
    '11111111-1111-4111-8111-111111111111',
    'desktop_full_access',
    true,
    'admin',
    now() - interval '1 minute',
    now() + interval '1 hour'
  ),
  (
    '22222222-2222-4222-8222-222222222222',
    'other_user_feature',
    true,
    'admin',
    now() - interval '1 minute',
    now() + interval '1 hour'
  );

select set_config(
  'request.jwt.claims',
  '{"sub":"11111111-1111-4111-8111-111111111111","role":"authenticated"}',
  true
);

select is(
  (select account_status from public.get_my_access()),
  'active',
  'the RPC returns the authenticated account status'
);
select is(
  (select full_access from public.get_my_access()),
  true,
  'a current desktop entitlement grants full access'
);
select is(
  (select entitlements from public.get_my_access()),
  array['desktop_full_access']::text[],
  'the RPC returns only the authenticated user entitlements'
);
select ok(
  not ('other_user_feature' = any((select entitlements from public.get_my_access()))),
  'another user entitlement cannot cross the owner boundary'
);

update public.profiles
set account_status = 'disabled'
where user_id = '11111111-1111-4111-8111-111111111111';

select is(
  (select full_access from public.get_my_access()),
  false,
  'a disabled account cannot receive full access'
);

update public.profiles
set account_status = 'active'
where user_id = '11111111-1111-4111-8111-111111111111';
update private.entitlements
set valid_until = now() - interval '1 second'
where user_id = '11111111-1111-4111-8111-111111111111'
  and feature_key = 'desktop_full_access';

select is(
  (select full_access from public.get_my_access()),
  false,
  'an expired desktop entitlement cannot grant access'
);

select * from finish();
rollback;
