-- Owner-derived paid-access snapshot for the authenticated user.
--
-- The caller cannot supply a user ID. auth.uid() is the only account selector, and
-- the private billing schema remains inaccessible to anon/authenticated roles.

create or replace function public.get_my_access()
returns table (
  account_status text,
  subscription_provider text,
  subscription_status text,
  current_period_end timestamptz,
  entitlements text[],
  full_access boolean
)
language plpgsql
security definer
stable
set search_path = ''
as $$
declare
  current_user_id uuid := auth.uid();
begin
  if current_user_id is null then
    raise exception 'Authentication is required.' using errcode = '42501';
  end if;

  return query
  with profile as (
    select p.account_status
    from public.profiles as p
    where p.user_id = current_user_id
  ),
  active_entitlements as (
    select e.feature_key
    from private.entitlements as e
    where e.user_id = current_user_id
      and e.active
      and e.valid_from <= now()
      and (e.valid_until is null or e.valid_until > now())
  ),
  latest_subscription as (
    select
      s.provider,
      s.status,
      s.current_period_end
    from private.billing_subscriptions as s
    where s.user_id = current_user_id
    order by s.updated_at desc, s.created_at desc, s.id desc
    limit 1
  ),
  access_snapshot as (
    select
      coalesce((select p.account_status from profile as p), 'disabled') as account_status,
      (select s.provider from latest_subscription as s) as subscription_provider,
      (select s.status from latest_subscription as s) as subscription_status,
      (select s.current_period_end from latest_subscription as s) as current_period_end,
      coalesce(
        (
          select array_agg(e.feature_key order by e.feature_key)
          from active_entitlements as e
        ),
        array[]::text[]
      ) as entitlements
  )
  select
    a.account_status,
    a.subscription_provider,
    a.subscription_status,
    a.current_period_end,
    a.entitlements,
    a.account_status = 'active'
      and 'desktop_full_access' = any(a.entitlements) as full_access
  from access_snapshot as a;
end;
$$;

revoke all on function public.get_my_access() from public;
revoke all on function public.get_my_access() from anon;
grant execute on function public.get_my_access() to authenticated;
grant execute on function public.get_my_access() to service_role;
