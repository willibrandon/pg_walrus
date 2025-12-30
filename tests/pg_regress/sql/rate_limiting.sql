-- pg_regress tests for rate limiting feature (Feature 006)
-- Tests GUC parameters and status function output

-- Test 1: walrus.cooldown_sec GUC is visible with correct default (300)
SELECT name, setting, context, vartype
FROM pg_settings
WHERE name = 'walrus.cooldown_sec';

-- Test 2: walrus.max_changes_per_hour GUC is visible with correct default (4)
SELECT name, setting, context, vartype
FROM pg_settings
WHERE name = 'walrus.max_changes_per_hour';

-- Test 3: Both GUCs have sighup context
SELECT COUNT(*) = 2 AS both_sighup
FROM pg_settings
WHERE name IN ('walrus.cooldown_sec', 'walrus.max_changes_per_hour')
  AND context = 'sighup';

-- Test 4: walrus.cooldown_sec has valid range (0 to 86400)
SELECT name, min_val::int, max_val::int
FROM pg_settings
WHERE name = 'walrus.cooldown_sec';

-- Test 5: walrus.max_changes_per_hour has valid range (0 to 1000)
SELECT name, min_val::int, max_val::int
FROM pg_settings
WHERE name = 'walrus.max_changes_per_hour';

-- Test 6: walrus.status() includes rate limiting fields
SELECT
    (status->>'cooldown_sec')::int AS cooldown_sec,
    (status->>'max_changes_per_hour')::int AS max_changes_per_hour,
    (status->>'cooldown_active')::boolean AS cooldown_active,
    (status->>'cooldown_remaining_sec')::int AS cooldown_remaining_sec,
    (status->>'changes_this_hour')::int AS changes_this_hour,
    (status->>'hourly_limit_reached')::boolean AS hourly_limit_reached
FROM walrus.status() AS status;

-- Test 7: walrus.history allows 'skipped' action
-- First verify the check constraint includes 'skipped'
SELECT check_clause LIKE '%skipped%' AS has_skipped_action
FROM information_schema.check_constraints
WHERE check_clause LIKE '%action%ANY%';

-- Test 8: Insert a skipped record to verify schema
INSERT INTO walrus.history
    (action, old_size_mb, new_size_mb, forced_checkpoints, checkpoint_timeout_sec, reason, metadata)
VALUES
    ('skipped', 1024, 2048, 5, 300, 'cooldown active',
     '{"blocked_by": "cooldown", "cooldown_remaining_sec": 180}'::jsonb);

-- Verify the insert succeeded
SELECT action, old_size_mb, new_size_mb, reason,
       metadata->>'blocked_by' AS blocked_by
FROM walrus.history
WHERE action = 'skipped' AND reason = 'cooldown active'
ORDER BY id DESC LIMIT 1;

-- Cleanup test record
DELETE FROM walrus.history WHERE action = 'skipped' AND reason = 'cooldown active';

-- Test 9: Count of walrus GUCs with sighup context should be 11
SELECT COUNT(*) AS sighup_guc_count
FROM pg_settings
WHERE name LIKE 'walrus.%' AND context = 'sighup';
