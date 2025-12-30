-- Setup test: create extension and verify pg_walrus is functional
-- pg_walrus must be loaded via shared_preload_libraries for the background worker
-- CREATE EXTENSION creates the walrus schema and history table

CREATE EXTENSION pg_walrus;

-- Verify GUCs are visible
SELECT COUNT(*) >= 8 AS pg_walrus_gucs_registered
FROM pg_settings
WHERE name LIKE 'walrus.%';

-- Verify walrus schema exists
SELECT nspname FROM pg_namespace WHERE nspname = 'walrus';

-- Verify history table exists
SELECT EXISTS (
    SELECT 1 FROM pg_class c
    JOIN pg_namespace n ON n.oid = c.relnamespace
    WHERE n.nspname = 'walrus' AND c.relname = 'history'
) AS history_table_exists;
