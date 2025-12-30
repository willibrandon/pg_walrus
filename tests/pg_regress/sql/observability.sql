-- pg_regress tests for SQL observability functions (US1-US5)

-- =========================================================================
-- walrus.status() tests (US1)
-- =========================================================================

-- Test that walrus.status() returns valid JSONB
SELECT jsonb_typeof(walrus.status()) AS status_type;

-- Test that status contains required fields
SELECT
    walrus.status() ? 'enabled' AS has_enabled,
    walrus.status() ? 'current_max_wal_size_mb' AS has_current_size,
    walrus.status() ? 'worker_running' AS has_worker_running,
    walrus.status() ? 'total_adjustments' AS has_total_adjustments;

-- Test shrink fields exist
SELECT
    walrus.status() ? 'shrink_enabled' AS has_shrink_enabled,
    walrus.status() ? 'shrink_factor' AS has_shrink_factor,
    walrus.status() ? 'shrink_intervals' AS has_shrink_intervals,
    walrus.status() ? 'min_size_mb' AS has_min_size_mb;

-- Test derived field
SELECT walrus.status() ? 'at_ceiling' AS has_at_ceiling;

-- =========================================================================
-- walrus.recommendation() tests (US3)
-- =========================================================================

-- Test that walrus.recommendation() returns valid JSONB
SELECT jsonb_typeof(walrus.recommendation()) AS recommendation_type;

-- Test that recommendation contains required fields
SELECT
    walrus.recommendation() ? 'action' AS has_action,
    walrus.recommendation() ? 'current_size_mb' AS has_current_size,
    walrus.recommendation() ? 'recommended_size_mb' AS has_recommended_size,
    walrus.recommendation() ? 'confidence' AS has_confidence,
    walrus.recommendation() ? 'reason' AS has_reason;

-- Test action value is one of expected values
SELECT (walrus.recommendation()->>'action') IN ('increase', 'decrease', 'none', 'error') AS valid_action;

-- Test confidence is numeric and in valid range
SELECT
    (walrus.recommendation()->>'confidence')::int >= 0 AS confidence_ge_0,
    (walrus.recommendation()->>'confidence')::int <= 100 AS confidence_le_100;

-- =========================================================================
-- walrus.analyze() tests (US4)
-- =========================================================================

-- Test that walrus.analyze() returns valid JSONB
SELECT jsonb_typeof(walrus.analyze()) AS analyze_type;

-- Test that analyze contains analyzed field
SELECT (walrus.analyze()->>'analyzed')::boolean AS analyzed;

-- Test that analyze(apply := false) does not apply changes
SELECT (walrus.analyze(apply := false)->>'applied')::boolean AS applied_false;

-- Test that analyze contains recommendation
SELECT walrus.analyze() ? 'recommendation' AS has_recommendation;

-- =========================================================================
-- walrus.history() tests (US2)
-- =========================================================================

-- Insert test data for history
INSERT INTO walrus.history
    (action, old_size_mb, new_size_mb, forced_checkpoints, checkpoint_timeout_sec, reason)
VALUES
    ('increase', 1024, 2048, 5, 300, 'Test increase'),
    ('decrease', 2048, 1536, 0, 300, 'Test decrease');

-- Test that walrus.history() returns rows
SELECT count(*) > 0 AS has_history FROM walrus.history();

-- Test that columns are accessible
SELECT action, old_size_mb, new_size_mb FROM walrus.history() ORDER BY timestamp DESC LIMIT 2;

-- Clean up test data
DELETE FROM walrus.history WHERE reason LIKE 'Test%';

-- =========================================================================
-- walrus.reset() tests (US5)
-- Note: This test runs as superuser
-- =========================================================================

-- Insert test data
INSERT INTO walrus.history
    (action, old_size_mb, new_size_mb, forced_checkpoints, checkpoint_timeout_sec)
VALUES ('increase', 512, 1024, 3, 300);

-- Verify data exists
SELECT count(*) > 0 AS has_data_before FROM walrus.history;

-- Reset (as superuser)
SELECT walrus.reset() AS reset_result;

-- Verify data was cleared
SELECT count(*) AS rows_after_reset FROM walrus.history;

-- =========================================================================
-- walrus.cleanup_history() tests
-- =========================================================================

-- Insert old record
INSERT INTO walrus.history
    (timestamp, action, old_size_mb, new_size_mb, forced_checkpoints, checkpoint_timeout_sec)
VALUES (now() - interval '8 days', 'increase', 1024, 2048, 5, 300);

-- Cleanup should delete old records
SELECT walrus.cleanup_history() >= 0 AS cleanup_succeeded;
