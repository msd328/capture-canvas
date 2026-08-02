-- Defense in depth for objects created before the default-privilege rules in the
-- foundation migration. The private schema is not exposed through the Data API and
-- untrusted roles must not retain direct object or function privileges.

revoke all on all tables in schema private from public, anon, authenticated;
revoke all on all sequences in schema private from public, anon, authenticated;
revoke execute on all functions in schema private from public, anon, authenticated;

grant all on all tables in schema private to service_role;
grant all on all sequences in schema private to service_role;
grant execute on all functions in schema private to service_role;
