# Quickstart: Auto-Shrink Feature

**Date**: 2025-12-30
**Feature**: 002-auto-shrink

## Overview

The auto-shrink feature automatically reduces `max_wal_size` when workload decreases, preventing permanent storage growth from transient spikes.

## Prerequisites

- pg_walrus extension installed and loaded via `shared_preload_libraries`
- PostgreSQL 15, 16, 17, or 18

## Default Behavior

With default settings, auto-shrink is **enabled** and will:

1. Wait for 5 consecutive quiet checkpoint intervals (no forced checkpoints)
2. Shrink `max_wal_size` by 25% (multiply by 0.75)
3. Never shrink below 1GB (1024 MB)

## Configuration

### View Current Settings

```sql
SHOW walrus.shrink_enable;    -- 'on' (default)
SHOW walrus.shrink_factor;    -- '0.75' (default)
SHOW walrus.shrink_intervals; -- '5' (default)
SHOW walrus.min_size;         -- '1GB' (default)
```

### Modify Settings (Runtime)

```sql
-- Disable shrinking (grow still works)
ALTER SYSTEM SET walrus.shrink_enable = false;
SELECT pg_reload_conf();

-- More aggressive shrinking (50% reduction)
ALTER SYSTEM SET walrus.shrink_factor = 0.5;
SELECT pg_reload_conf();

-- Require longer quiet period (10 intervals)
ALTER SYSTEM SET walrus.shrink_intervals = 10;
SELECT pg_reload_conf();

-- Higher minimum floor (2GB)
ALTER SYSTEM SET walrus.min_size = '2GB';
SELECT pg_reload_conf();
```

### Reset to Defaults

```sql
ALTER SYSTEM RESET walrus.shrink_enable;
ALTER SYSTEM RESET walrus.shrink_factor;
ALTER SYSTEM RESET walrus.shrink_intervals;
ALTER SYSTEM RESET walrus.min_size;
SELECT pg_reload_conf();
```

## Common Configurations

### Conservative (Slow Shrink)

For production systems where stability is paramount:

```sql
ALTER SYSTEM SET walrus.shrink_factor = 0.9;      -- Only 10% reduction
ALTER SYSTEM SET walrus.shrink_intervals = 10;    -- Wait 10 quiet intervals
ALTER SYSTEM SET walrus.min_size = '2GB';         -- Higher floor
SELECT pg_reload_conf();
```

### Aggressive (Fast Shrink)

For development/test environments:

```sql
ALTER SYSTEM SET walrus.shrink_factor = 0.5;     -- 50% reduction
ALTER SYSTEM SET walrus.shrink_intervals = 2;    -- Wait only 2 intervals
ALTER SYSTEM SET walrus.min_size = '512MB';      -- Lower floor
SELECT pg_reload_conf();
```

### Grow-Only (Disable Shrink)

For DBAs who prefer manual shrinking:

```sql
ALTER SYSTEM SET walrus.shrink_enable = false;
SELECT pg_reload_conf();
```

## Monitoring

### Check Background Worker Status

```sql
SELECT pid, backend_type, application_name, state
FROM pg_stat_activity
WHERE backend_type = 'pg_walrus';
```

### View Shrink Events in Logs

Shrink events are logged at LOG level:

```
LOG:  pg_walrus: shrinking max_wal_size from 4096 MB to 3072 MB
```

### Current max_wal_size

```sql
SHOW max_wal_size;
```

## Interaction with Grow

| Scenario | Behavior |
|----------|----------|
| Forced checkpoints >= threshold | Grow evaluates; shrink skipped |
| Forced checkpoints < threshold | Quiet interval counter increments; shrink may trigger |
| Grow triggers | Quiet interval counter resets to 0 |
| Shrink triggers | Quiet interval counter resets to 0 |

## Troubleshooting

### Shrink Not Happening

1. **Check if enabled**: `SHOW walrus.shrink_enable;` should return `on`
2. **Check quiet intervals**: Default is 5, each interval = `checkpoint_timeout`
3. **Check min_size**: Current `max_wal_size` may already be at floor
4. **Check activity**: Forced checkpoints may be resetting the counter

### Shrink Too Aggressive

Increase `shrink_factor` (closer to 1.0) and/or increase `shrink_intervals`.

### Shrink Going Below Safe Level

Increase `walrus.min_size` to set a higher floor.

## Example Workflow

1. Database starts with `max_wal_size = 1GB`
2. Batch job causes forced checkpoints → pg_walrus grows to 4GB
3. Batch job completes, normal low-write workload resumes
4. 5 checkpoint intervals pass with no forced checkpoints
5. pg_walrus shrinks from 4GB to 3GB (4096 × 0.75 = 3072)
6. 5 more quiet intervals pass
7. pg_walrus shrinks from 3GB to 2.25GB (3072 × 0.75 = 2304)
8. Eventually stabilizes at `min_size` (1GB) or sustainable level
