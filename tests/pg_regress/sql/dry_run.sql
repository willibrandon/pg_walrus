-- pg_regress tests for dry-run mode (Feature 005)
-- Tests GUC visibility and configuration

-- Test 1: walrus.dry_run GUC is visible
SELECT name, setting, context, vartype
FROM pg_settings
WHERE name = 'walrus.dry_run';

-- Test 2: Default value is 'off'
SHOW walrus.dry_run;

-- Test 3: GUC has correct context (sighup)
SELECT context FROM pg_settings WHERE name = 'walrus.dry_run';

-- Test 4: GUC has correct type (bool)
SELECT vartype FROM pg_settings WHERE name = 'walrus.dry_run';

-- Test 5: walrus.history table accepts 'dry_run' action
-- First check the CHECK constraint allows dry_run, skipped, and other actions
-- (Use LIKE pattern to avoid version-specific formatting differences in check_clause)
SELECT constraint_name,
       check_clause LIKE '%dry_run%' AS has_dry_run,
       check_clause LIKE '%skipped%' AS has_skipped,
       check_clause LIKE '%increase%' AS has_increase
FROM information_schema.check_constraints
WHERE check_clause LIKE '%action%ANY%';

-- Test 6: Insert a dry_run record manually to verify schema
INSERT INTO walrus.history
    (action, old_size_mb, new_size_mb, forced_checkpoints, checkpoint_timeout_sec, reason, metadata)
VALUES
    ('dry_run', 1024, 2048, 5, 300, 'threshold exceeded',
     '{"dry_run": true, "would_apply": "increase", "delta": 5, "multiplier": 6}'::jsonb);

-- Verify the insert succeeded
SELECT action, old_size_mb, new_size_mb, reason,
       metadata->>'dry_run' AS dry_run_flag,
       metadata->>'would_apply' AS would_apply
FROM walrus.history
WHERE action = 'dry_run' AND reason = 'threshold exceeded'
ORDER BY id DESC LIMIT 1;

-- Test 7: Insert a dry_run shrink record
INSERT INTO walrus.history
    (action, old_size_mb, new_size_mb, forced_checkpoints, checkpoint_timeout_sec, reason, metadata)
VALUES
    ('dry_run', 4096, 3072, 0, 300, 'sustained low activity',
     '{"dry_run": true, "would_apply": "decrease", "shrink_factor": 0.75, "quiet_intervals": 5}'::jsonb);

-- Verify shrink record
SELECT action, old_size_mb, new_size_mb, reason,
       metadata->>'would_apply' AS would_apply,
       metadata->>'shrink_factor' AS shrink_factor
FROM walrus.history
WHERE action = 'dry_run' AND reason = 'sustained low activity'
ORDER BY id DESC LIMIT 1;

-- Test 8: Insert a dry_run capped record
INSERT INTO walrus.history
    (action, old_size_mb, new_size_mb, forced_checkpoints, checkpoint_timeout_sec, reason, metadata)
VALUES
    ('dry_run', 2048, 4096, 10, 300, 'capped at walrus.max',
     '{"dry_run": true, "would_apply": "capped", "delta": 10, "calculated_size_mb": 22528, "walrus_max_mb": 4096}'::jsonb);

-- Verify capped record
SELECT action, old_size_mb, new_size_mb, reason,
       metadata->>'would_apply' AS would_apply,
       metadata->>'walrus_max_mb' AS walrus_max_mb
FROM walrus.history
WHERE action = 'dry_run' AND reason = 'capped at walrus.max'
ORDER BY id DESC LIMIT 1;

-- Cleanup test records
DELETE FROM walrus.history WHERE action = 'dry_run';

-- Test 9: Count of walrus GUCs with sighup context should be 11
-- (enable, max, threshold, shrink_enable, shrink_factor, shrink_intervals, min_size,
--  history_retention_days, dry_run, cooldown_sec, max_changes_per_hour)
SELECT COUNT(*) AS sighup_guc_count
FROM pg_settings
WHERE name LIKE 'walrus.%' AND context = 'sighup';
