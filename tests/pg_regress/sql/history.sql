-- pg_regress test for walrus.history table (User Story 1: T016)
-- Tests query capabilities and acceptance scenarios from spec.md

-- Verify history table exists in walrus schema
SELECT EXISTS (
    SELECT 1 FROM pg_catalog.pg_class c
    JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
    WHERE n.nspname = 'walrus' AND c.relname = 'history'
) AS history_table_exists;

-- Verify schema has walrus namespace
SELECT nspname FROM pg_catalog.pg_namespace WHERE nspname = 'walrus';

-- Verify all expected columns exist with correct types
SELECT column_name, data_type, is_nullable
FROM information_schema.columns
WHERE table_schema = 'walrus' AND table_name = 'history'
ORDER BY ordinal_position;

-- Verify index exists
SELECT indexname, indexdef
FROM pg_indexes
WHERE schemaname = 'walrus' AND tablename = 'history';

-- Verify CHECK constraints exist
SELECT conname, pg_get_constraintdef(oid)
FROM pg_constraint
WHERE conrelid = 'walrus.history'::regclass AND contype = 'c'
ORDER BY conname;

-- Insert test data for query testing
INSERT INTO walrus.history
    (action, old_size_mb, new_size_mb, forced_checkpoints, checkpoint_timeout_sec, reason, metadata)
VALUES
    ('increase', 1024, 2048, 5, 300, 'Forced checkpoints exceeded threshold', '{"delta": 5, "multiplier": 6}'::jsonb),
    ('decrease', 2048, 1536, 0, 300, 'Sustained low activity', '{"shrink_factor": 0.75, "quiet_intervals": 5}'::jsonb),
    ('capped', 2048, 4096, 10, 300, 'Calculated size exceeded walrus.max', '{"calculated_size_mb": 22528, "walrus_max_mb": 4096}'::jsonb);

-- Verify data inserted correctly
SELECT action, old_size_mb, new_size_mb, forced_checkpoints, reason IS NOT NULL AS has_reason
FROM walrus.history
ORDER BY id;

-- Test query: most recent decisions (spec acceptance scenario)
SELECT action, old_size_mb, new_size_mb
FROM walrus.history
ORDER BY timestamp DESC
LIMIT 10;

-- Test query: filter by action type (spec acceptance scenario)
SELECT count(*) AS increase_count
FROM walrus.history
WHERE action = 'increase';

-- Test query: summary by action type (spec acceptance scenario)
SELECT action, count(*) AS count, avg(new_size_mb - old_size_mb)::int AS avg_change
FROM walrus.history
GROUP BY action
ORDER BY action;

-- Test JSONB metadata access
SELECT action, metadata->>'delta' AS delta, metadata->>'shrink_factor' AS shrink_factor
FROM walrus.history
ORDER BY id;

-- Verify GUC is accessible
SHOW walrus.history_retention_days;

-- Verify GUC range via pg_settings
SELECT name, setting, min_val, max_val, vartype, context
FROM pg_settings
WHERE name = 'walrus.history_retention_days';

-- Clean up test data
DELETE FROM walrus.history;
