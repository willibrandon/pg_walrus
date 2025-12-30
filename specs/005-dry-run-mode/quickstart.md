# Quickstart: Dry-Run Mode

**Feature**: 005-dry-run-mode

## Overview

Dry-run mode allows you to observe what pg_walrus WOULD do without actually modifying `max_wal_size`. Use this to validate extension behavior before enabling automatic sizing in production.

## Enable Dry-Run Mode

```sql
-- Enable dry-run mode (logs decisions without applying)
ALTER SYSTEM SET walrus.dry_run = true;
SELECT pg_reload_conf();

-- Verify setting
SHOW walrus.dry_run;
-- Returns: on
```

## What Happens in Dry-Run Mode

When a sizing decision would occur:

1. **Log Message**: PostgreSQL logs what WOULD happen
   ```
   LOG: pg_walrus [DRY-RUN]: would change max_wal_size from 1024 MB to 2048 MB (threshold exceeded)
   ```

2. **History Record**: Decision recorded with `action = 'dry_run'`
   ```sql
   SELECT timestamp, action, old_size_mb, new_size_mb,
          metadata->>'would_apply' AS would_apply
   FROM walrus.history
   WHERE action = 'dry_run'
   ORDER BY timestamp DESC;
   ```

3. **No Configuration Change**: `max_wal_size` remains unchanged

## View Dry-Run Decisions

```sql
-- All dry-run decisions
SELECT timestamp, old_size_mb, new_size_mb,
       metadata->>'would_apply' AS would_apply,
       reason
FROM walrus.history
WHERE action = 'dry_run'
ORDER BY timestamp DESC
LIMIT 10;

-- Count by decision type
SELECT metadata->>'would_apply' AS decision_type, count(*)
FROM walrus.history
WHERE action = 'dry_run'
GROUP BY metadata->>'would_apply';
```

## Transition to Active Mode

After validating behavior, disable dry-run:

```sql
-- Disable dry-run (enables actual sizing)
ALTER SYSTEM SET walrus.dry_run = false;
SELECT pg_reload_conf();

-- Verify
SHOW walrus.dry_run;
-- Returns: off
```

## Common Use Cases

### 1. Pre-Production Validation

```sql
-- Enable dry-run before production deployment
ALTER SYSTEM SET walrus.enable = true;
ALTER SYSTEM SET walrus.dry_run = true;
SELECT pg_reload_conf();

-- Run for several days, then review
SELECT count(*), metadata->>'would_apply'
FROM walrus.history
WHERE action = 'dry_run'
GROUP BY metadata->>'would_apply';

-- If behavior looks correct, enable active mode
ALTER SYSTEM SET walrus.dry_run = false;
SELECT pg_reload_conf();
```

### 2. Parameter Tuning

```sql
-- Experiment with threshold changes
ALTER SYSTEM SET walrus.threshold = 5;
ALTER SYSTEM SET walrus.dry_run = true;
SELECT pg_reload_conf();

-- Observe how new threshold affects decisions
-- Adjust as needed without affecting production
```

### 3. Audit Trail

```sql
-- Get full audit of simulated decisions
SELECT timestamp,
       old_size_mb,
       new_size_mb,
       metadata->>'would_apply' AS would_apply,
       metadata->>'delta' AS checkpoint_delta,
       reason
FROM walrus.history
WHERE action = 'dry_run'
  AND timestamp > now() - interval '7 days'
ORDER BY timestamp;
```

## Verify Dry-Run is Active

```sql
-- Check current setting
SHOW walrus.dry_run;

-- Check in pg_settings
SELECT name, setting, context
FROM pg_settings
WHERE name = 'walrus.dry_run';
```

## Notes

- Dry-run mode requires `walrus.enable = true` to process decisions
- Algorithm state (quiet_intervals, checkpoint counts) updates normally in dry-run mode
- Transitioning between modes is seamless; no restart required
- Dry-run records are subject to normal `walrus.history_retention_days` cleanup
