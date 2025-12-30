# Quickstart: Rate Limiting

**Feature**: 006-rate-limiting
**Date**: 2025-12-30

## Overview

Rate limiting prevents pg_walrus from making rapid successive adjustments to `max_wal_size` during unstable workloads. It enforces two controls:

1. **Cooldown Period**: Minimum time between adjustments (default: 300 seconds)
2. **Hourly Limit**: Maximum adjustments per rolling hour (default: 4)

## Configuration

### GUC Parameters

```sql
-- View current settings
SHOW walrus.cooldown_sec;          -- Default: 300 (5 minutes)
SHOW walrus.max_changes_per_hour;  -- Default: 4

-- Modify at runtime (requires pg_reload_conf())
ALTER SYSTEM SET walrus.cooldown_sec = 600;        -- 10 minute cooldown
ALTER SYSTEM SET walrus.max_changes_per_hour = 2;  -- Max 2 changes per hour
SELECT pg_reload_conf();

-- Disable cooldown (only hourly limit applies)
ALTER SYSTEM SET walrus.cooldown_sec = 0;
SELECT pg_reload_conf();

-- Disable all automatic adjustments (emergency stop)
ALTER SYSTEM SET walrus.max_changes_per_hour = 0;
SELECT pg_reload_conf();
```

## Monitoring

### Check Rate Limiting Status

```sql
SELECT walrus.status();
```

**Rate limiting fields in output**:
```json
{
  "cooldown_sec": 300,
  "max_changes_per_hour": 4,
  "cooldown_active": true,
  "cooldown_remaining_sec": 180,
  "changes_this_hour": 2,
  "hourly_window_start": "2025-12-30T10:00:00.000000+00:00",
  "hourly_limit_reached": false,
  ...
}
```

### View Skipped Adjustments

```sql
-- Show all skipped adjustments
SELECT * FROM walrus.history()
WHERE action = 'skipped'
ORDER BY timestamp DESC;

-- Show recent rate limit events with details
SELECT
  timestamp,
  action,
  reason,
  metadata->>'blocked_by' AS blocked_by,
  metadata->>'cooldown_remaining_sec' AS cooldown_remaining
FROM walrus.history
WHERE action = 'skipped'
ORDER BY timestamp DESC
LIMIT 10;
```

## Manual Override

When rate limiting blocks automatic adjustments, operators can still intervene:

```sql
-- Manual adjustment bypasses rate limiting (superuser only)
SELECT walrus.analyze(apply := true);
```

**Note**: Manual adjustments via `walrus.analyze(apply := true)` do NOT count against rate limits and are not blocked by cooldown or hourly limits.

## Common Scenarios

### Scenario 1: High Volatility Workload

For workloads with frequent checkpoint spikes, increase limits:

```sql
ALTER SYSTEM SET walrus.cooldown_sec = 60;         -- 1 minute cooldown
ALTER SYSTEM SET walrus.max_changes_per_hour = 12; -- Up to 12 changes/hour
SELECT pg_reload_conf();
```

### Scenario 2: Stable Production Environment

For stable environments, enforce stricter limits:

```sql
ALTER SYSTEM SET walrus.cooldown_sec = 900;        -- 15 minute cooldown
ALTER SYSTEM SET walrus.max_changes_per_hour = 2;  -- Max 2 changes/hour
SELECT pg_reload_conf();
```

### Scenario 3: Emergency: Stop All Adjustments

```sql
-- Immediately stop all automatic adjustments
ALTER SYSTEM SET walrus.max_changes_per_hour = 0;
SELECT pg_reload_conf();

-- Or disable the extension entirely
ALTER SYSTEM SET walrus.enable = false;
SELECT pg_reload_conf();
```

### Scenario 4: Reset After Incident

After resolving an incident, reset rate limiting state:

```sql
-- Clear all counters including rate limiting state (superuser only)
SELECT walrus.reset();
```

## Behavior Summary

| Condition | Automatic Adjustment | Manual Adjustment |
|-----------|---------------------|-------------------|
| Within cooldown | BLOCKED | ALLOWED |
| Hourly limit reached | BLOCKED | ALLOWED |
| cooldown_sec = 0 | Uses hourly limit only | ALLOWED |
| max_changes_per_hour = 0 | BLOCKED | ALLOWED |
| PostgreSQL restart | Counters reset | ALLOWED |

## Logging

Rate limiting events are logged at LOG level:

```
LOG:  pg_walrus: adjustment blocked - cooldown active (180 seconds remaining)
LOG:  pg_walrus: adjustment blocked - hourly limit reached (4 of 4)
```

These messages appear in the PostgreSQL log when automatic adjustments are blocked.
