-- pg_regress test for compliance export (User Story 4: T052-T054)
-- Tests that history table can be exported for audit purposes

-- Insert test data for export testing
INSERT INTO walrus.history
    (timestamp, action, old_size_mb, new_size_mb, forced_checkpoints, checkpoint_timeout_sec, reason, metadata)
VALUES
    ('2025-01-15 10:00:00+00', 'increase', 1024, 2048, 5, 300, 'Forced checkpoints exceeded threshold', '{"delta": 5, "multiplier": 6}'::jsonb),
    ('2025-01-15 11:00:00+00', 'decrease', 2048, 1536, 0, 300, 'Sustained low activity', '{"shrink_factor": 0.75}'::jsonb),
    ('2025-01-15 12:00:00+00', 'capped', 2048, 4096, 10, 300, 'Calculated size exceeded walrus.max', '{"walrus_max_mb": 4096}'::jsonb);

-- Verify data exists
SELECT count(*) AS records_for_export FROM walrus.history;

-- Test export query format (spec acceptance scenario)
-- Exclude id column as it's non-deterministic (auto-increment sequence)
SELECT timestamp, action, old_size_mb, new_size_mb, forced_checkpoints, checkpoint_timeout_sec, reason, metadata
FROM walrus.history
WHERE timestamp >= '2025-01-01'
ORDER BY timestamp;

-- Test that JSONB metadata is preserved in queries (T054)
-- Verify we can access specific metadata fields
SELECT
    action,
    metadata->>'delta' AS delta,
    metadata->>'shrink_factor' AS shrink_factor,
    metadata->>'walrus_max_mb' AS walrus_max_mb
FROM walrus.history
ORDER BY timestamp;

-- Test COPY TO format (cannot actually write file in pg_regress, but verify query works)
-- The actual COPY command would be:
-- COPY (SELECT * FROM walrus.history WHERE timestamp >= '2025-01-01' ORDER BY timestamp) TO '/tmp/audit.csv' WITH CSV HEADER;
-- We test the SELECT portion (excluding id for deterministic output):
SELECT
    timestamp AT TIME ZONE 'UTC' AS timestamp_utc,
    action,
    old_size_mb,
    new_size_mb,
    forced_checkpoints,
    checkpoint_timeout_sec,
    reason,
    metadata::text AS metadata_json
FROM walrus.history
ORDER BY timestamp;

-- Verify all column types are export-friendly
SELECT
    column_name,
    data_type,
    CASE
        WHEN data_type IN ('bigint', 'integer', 'text', 'timestamp with time zone', 'jsonb') THEN 'CSV compatible'
        ELSE 'Check compatibility'
    END AS export_status
FROM information_schema.columns
WHERE table_schema = 'walrus' AND table_name = 'history'
ORDER BY ordinal_position;

-- Clean up
DELETE FROM walrus.history;
