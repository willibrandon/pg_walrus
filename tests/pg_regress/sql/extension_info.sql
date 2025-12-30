-- Test extension metadata and GUC visibility
-- Note: GUCs are visible because pg_walrus is loaded via shared_preload_libraries

-- All GUC parameters should be visible in pg_settings
SELECT name, setting, unit, short_desc
FROM pg_settings
WHERE name LIKE 'walrus.%'
ORDER BY name;

-- Check GUC context is SIGHUP (allows runtime changes via ALTER SYSTEM)
SELECT name, context
FROM pg_settings
WHERE name LIKE 'walrus.%'
ORDER BY name;
