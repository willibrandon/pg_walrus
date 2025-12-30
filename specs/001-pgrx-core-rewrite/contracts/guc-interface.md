# GUC Interface Contract: pg_walrus

**Date**: 2025-12-29
**Feature**: 001-pgrx-core-rewrite

## Overview

pg_walrus exposes three GUC (Grand Unified Configuration) parameters for runtime control. These parameters follow PostgreSQL's standard configuration interface.

## Parameters

### walrus.enable

| Property | Value |
|----------|-------|
| Type | boolean |
| Default | `true` |
| Context | SIGHUP |
| Description | Enable automatic resizing of max_wal_size parameter |

**Behavior**:
- `true`: Background worker monitors checkpoints and adjusts `max_wal_size`
- `false`: Background worker runs but takes no action

**Example Usage**:
```sql
-- Disable pg_walrus temporarily
ALTER SYSTEM SET walrus.enable = false;
SELECT pg_reload_conf();

-- Re-enable
ALTER SYSTEM SET walrus.enable = true;
SELECT pg_reload_conf();

-- Check current value
SHOW walrus.enable;
```

---

### walrus.max

| Property | Value |
|----------|-------|
| Type | integer |
| Unit | MB |
| Default | `4096` (4GB) |
| Minimum | `2` |
| Maximum | `2147483647` (i32::MAX) |
| Context | SIGHUP |
| Description | Maximum size for max_wal_size that pg_walrus will not exceed |

**Behavior**:
- Calculated `max_wal_size` is capped at this value
- If current `max_wal_size` already equals `walrus.max`, no resize occurs
- Warning logged when cap is reached

**Example Usage**:
```sql
-- Set maximum to 16GB
ALTER SYSTEM SET walrus.max = '16GB';
SELECT pg_reload_conf();

-- Can also use MB directly
ALTER SYSTEM SET walrus.max = 16384;
SELECT pg_reload_conf();

-- Check current value (returns MB)
SHOW walrus.max;
```

---

### walrus.threshold

| Property | Value |
|----------|-------|
| Type | integer |
| Default | `2` |
| Minimum | `1` |
| Maximum | `1000` |
| Context | SIGHUP |
| Description | Forced checkpoints per checkpoint_timeout interval before increasing max_wal_size |

**Behavior**:
- Resize triggered only when `forced_checkpoint_delta >= walrus.threshold`
- Higher values tolerate occasional WAL spikes (e.g., from batch jobs)
- Lower values respond more aggressively to forced checkpoints

**Example Usage**:
```sql
-- Only resize after 5 forced checkpoints
ALTER SYSTEM SET walrus.threshold = 5;
SELECT pg_reload_conf();

-- More aggressive (resize after any forced checkpoint)
ALTER SYSTEM SET walrus.threshold = 1;
SELECT pg_reload_conf();

-- Check current value
SHOW walrus.threshold;
```

---

## Configuration File Examples

### postgresql.conf
```ini
# pg_walrus configuration
walrus.enable = true
walrus.max = 8GB
walrus.threshold = 3
```

### ALTER SYSTEM (postgresql.auto.conf)
```sql
ALTER SYSTEM SET walrus.enable = true;
ALTER SYSTEM SET walrus.max = '8GB';
ALTER SYSTEM SET walrus.threshold = 3;
SELECT pg_reload_conf();
```

---

## Querying Configuration

```sql
-- Show all pg_walrus parameters
SELECT name, setting, unit, short_desc
FROM pg_settings
WHERE name LIKE 'walrus.%';

-- Expected output:
--       name        | setting | unit |                   short_desc
-- ------------------+---------+------+------------------------------------------------
--  walrus.enable    | on      |      | Enable automatic resizing of max_wal_size parameter
--  walrus.max       | 4096    | MB   | Maximum size for max_wal_size that pg_walrus will not exceed
--  walrus.threshold | 2       |      | Forced checkpoints per timeout before increasing max_wal_size
```

---

## Interaction with PostgreSQL Parameters

| PostgreSQL Parameter | pg_walrus Interaction |
|---------------------|----------------------|
| `max_wal_size` | Modified by pg_walrus via ALTER SYSTEM |
| `checkpoint_timeout` | Used as wake interval for background worker |
| `shared_preload_libraries` | Must include `pg_walrus` for extension to load |

**Constraints**:
- `walrus.max` should be less than available WAL storage
- `max_wal_size` modified by pg_walrus persists in `postgresql.auto.conf`
- Manual changes to `max_wal_size` are respected; pg_walrus only increases, never decreases

---

## Error Conditions

| Condition | Behavior |
|-----------|----------|
| `walrus.max < current max_wal_size` | Resize skipped (no decrease) |
| Invalid value for parameter | Standard PostgreSQL error at SET/ALTER SYSTEM time |
| Extension not in shared_preload_libraries | ERROR at startup |
| ALTER SYSTEM permission denied | Warning logged, operation skipped |
