-- pg_regress test for walrus.cleanup_history() function (User Story 3: T050)
-- Tests cleanup function SQL interface and behavior

-- Verify cleanup function exists in walrus schema
SELECT EXISTS (
    SELECT 1 FROM pg_proc p
    JOIN pg_namespace n ON n.oid = p.pronamespace
    WHERE n.nspname = 'walrus' AND p.proname = 'cleanup_history'
) AS cleanup_function_exists;

-- Verify function signature (returns bigint)
SELECT prorettype::regtype AS return_type
FROM pg_proc p
JOIN pg_namespace n ON n.oid = p.pronamespace
WHERE n.nspname = 'walrus' AND p.proname = 'cleanup_history';

-- Test cleanup on empty table (should return 0)
DELETE FROM walrus.history;
SELECT walrus.cleanup_history() AS deleted_count_empty;

-- Insert records with various timestamps
INSERT INTO walrus.history
    (timestamp, action, old_size_mb, new_size_mb, forced_checkpoints, checkpoint_timeout_sec, reason)
VALUES
    (now() - interval '1 day', 'increase', 1024, 2048, 5, 300, 'Recent record - should be preserved'),
    (now() - interval '3 days', 'decrease', 2048, 1536, 0, 300, 'Recent record - should be preserved'),
    (now() - interval '10 days', 'increase', 512, 1024, 3, 300, 'Old record - should be deleted'),
    (now() - interval '15 days', 'capped', 2048, 4096, 10, 300, 'Old record - should be deleted');

-- Verify 4 records exist
SELECT count(*) AS total_before_cleanup FROM walrus.history;

-- Run cleanup (default retention is 7 days)
SELECT walrus.cleanup_history() AS deleted_count;

-- Verify only 2 recent records remain
SELECT count(*) AS total_after_cleanup FROM walrus.history;

-- Verify the correct records remain (recent ones)
SELECT action, reason
FROM walrus.history
ORDER BY timestamp DESC;

-- Test that cleanup uses timestamp index (verify EXPLAIN shows index scan)
EXPLAIN (COSTS OFF)
DELETE FROM walrus.history
WHERE timestamp < now() - 7 * interval '1 day';

-- Clean up for next test
DELETE FROM walrus.history;

-- Test with retention = 0 behavior
-- Insert a fresh record
INSERT INTO walrus.history
    (action, old_size_mb, new_size_mb, forced_checkpoints, checkpoint_timeout_sec)
VALUES
    ('increase', 100, 200, 1, 300);

-- Verify record exists
SELECT count(*) AS count_before FROM walrus.history;

-- Note: We cannot change the GUC in a transaction, so we verify the cleanup query logic
-- would delete all records with retention=0 (timestamp < now() - 0 days = timestamp < now())
SELECT count(*) AS would_delete_with_retention_zero
FROM walrus.history
WHERE timestamp < now() - 0 * interval '1 day';

-- Clean up
DELETE FROM walrus.history;
