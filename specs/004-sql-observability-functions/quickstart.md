# Quickstart: SQL Observability Functions

**Branch**: `004-sql-observability-functions` | **Date**: 2025-12-30

## Prerequisites

- pg_walrus extension installed and loaded via `shared_preload_libraries`
- PostgreSQL 15, 16, 17, or 18

## Quick Examples

### Check Extension Status

```sql
-- Get current extension state
SELECT walrus.status();
```

Output:
```json
{
  "enabled": true,
  "current_max_wal_size_mb": 1024,
  "configured_maximum_mb": 4096,
  "threshold": 2,
  "worker_running": true,
  "total_adjustments": 3,
  "quiet_intervals": 0,
  "at_ceiling": false
}
```

### View Adjustment History

```sql
-- See all past sizing decisions
SELECT * FROM walrus.history() ORDER BY timestamp DESC;
```

Output:
```
        timestamp         |  action  | old_size_mb | new_size_mb | forced_checkpoints |             reason
--------------------------+----------+-------------+-------------+--------------------+-----------------------------------
 2025-12-30 10:15:30+00   | increase |        1024 |        2048 |                  5 | Forced checkpoints exceeded threshold
 2025-12-30 09:45:00+00   | decrease |        2048 |        1536 |                  0 | Sustained low checkpoint activity
```

### Preview Recommendations

```sql
-- See what the extension would recommend (without applying)
SELECT walrus.recommendation();
```

Output:
```json
{
  "current_size_mb": 1024,
  "recommended_size_mb": 2048,
  "action": "increase",
  "reason": "3 forced checkpoints detected in last interval",
  "confidence": 85
}
```

### Trigger Manual Analysis

```sql
-- Analyze current state (superuser only to apply)
SELECT walrus.analyze();

-- Analyze AND apply the recommendation
SELECT walrus.analyze(apply := true);  -- Requires superuser
```

### Reset Extension State

```sql
-- Clear history and reset counters (superuser only)
SELECT walrus.reset();
```

## Common Use Cases

### Monitoring Dashboard Query

```sql
SELECT
    (status->>'enabled')::boolean AS enabled,
    (status->>'worker_running')::boolean AS worker_running,
    (status->>'current_max_wal_size_mb')::int AS current_mb,
    (status->>'configured_maximum_mb')::int AS max_mb,
    (status->>'total_adjustments')::int AS adjustments,
    (status->>'at_ceiling')::boolean AS at_ceiling
FROM walrus.status() AS status;
```

### Recent Activity Report

```sql
SELECT
    timestamp,
    action,
    old_size_mb || ' MB -> ' || new_size_mb || ' MB' AS change,
    reason
FROM walrus.history()
WHERE timestamp > now() - interval '7 days'
ORDER BY timestamp DESC;
```

### Pre-Maintenance Check

```sql
-- Before maintenance: preview what would happen
SELECT
    walrus.recommendation()->>'action' AS recommended_action,
    walrus.recommendation()->>'confidence' AS confidence;

-- If needed, apply recommendation before taking system offline
SELECT walrus.analyze(apply := true);
```

## Authorization Summary

| Function | Any User | Superuser Only |
|----------|----------|----------------|
| walrus.status() | Read | Read |
| walrus.history() | Read | Read |
| walrus.recommendation() | Read | Read |
| walrus.analyze(false) | Read | Read |
| walrus.analyze(true) | Denied | Execute |
| walrus.reset() | Denied | Execute |
