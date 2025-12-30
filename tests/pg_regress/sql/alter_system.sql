-- Test ALTER SYSTEM functionality for max_wal_size
-- This verifies that pg_walrus's internal ALTER SYSTEM mechanism works correctly
-- (pg_test cannot test this because ALTER SYSTEM cannot run inside a transaction block)

-- Cleanup any stale walrus settings from previous test runs
-- (ALTER SYSTEM persists in postgresql.auto.conf across database drops)
ALTER SYSTEM RESET walrus.enable;
ALTER SYSTEM RESET walrus.max;
ALTER SYSTEM RESET walrus.threshold;
ALTER SYSTEM RESET walrus.shrink_enable;
ALTER SYSTEM RESET walrus.shrink_factor;
ALTER SYSTEM RESET walrus.shrink_intervals;
ALTER SYSTEM RESET walrus.min_size;
ALTER SYSTEM RESET walrus.history_retention_days;
ALTER SYSTEM RESET walrus.dry_run;
ALTER SYSTEM RESET walrus.cooldown_sec;
ALTER SYSTEM RESET walrus.max_changes_per_hour;

-- Record current max_wal_size
SELECT setting AS original_max_wal_size FROM pg_settings WHERE name = 'max_wal_size' \gset

-- Test that ALTER SYSTEM works for max_wal_size
ALTER SYSTEM SET max_wal_size = '2GB';

-- Verify the change was written (check postgresql.auto.conf via pg_file_settings)
SELECT name, setting, sourcefile LIKE '%postgresql.auto.conf'
FROM pg_file_settings
WHERE name = 'max_wal_size'
ORDER BY seqno DESC
LIMIT 1;

-- Reset max_wal_size to avoid affecting other tests
ALTER SYSTEM RESET max_wal_size;

-- Verify reset worked
SELECT COUNT(*) = 0 AS reset_successful
FROM pg_file_settings
WHERE name = 'max_wal_size' AND sourcefile LIKE '%postgresql.auto.conf';

-- Test that walrus GUCs can be changed via ALTER SYSTEM
ALTER SYSTEM SET walrus.enable = false;
ALTER SYSTEM SET walrus.max = '8GB';
ALTER SYSTEM SET walrus.threshold = 5;

-- Verify changes were written
SELECT name, setting
FROM pg_file_settings
WHERE name LIKE 'walrus.%'
ORDER BY name;

-- Reset all walrus settings
ALTER SYSTEM RESET walrus.enable;
ALTER SYSTEM RESET walrus.max;
ALTER SYSTEM RESET walrus.threshold;

-- Verify all resets worked
SELECT COUNT(*) = 0 AS all_walrus_reset
FROM pg_file_settings
WHERE name LIKE 'walrus.%';
