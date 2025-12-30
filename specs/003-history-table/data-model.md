# Data Model: History Table

**Feature**: 003-history-table
**Date**: 2025-12-30

## Entities

### HistoryRecord

Represents a single sizing decision event recorded by the pg_walrus background worker.

| Field | Type | Nullable | Description |
|-------|------|----------|-------------|
| `id` | BIGSERIAL | NO | Auto-incrementing primary key |
| `timestamp` | TIMESTAMPTZ | NO | When the decision was made (default: now()) |
| `action` | TEXT | NO | Decision type: 'increase', 'decrease', 'capped' |
| `old_size_mb` | INTEGER | NO | max_wal_size before change (in MB) |
| `new_size_mb` | INTEGER | NO | max_wal_size after change (in MB) |
| `forced_checkpoints` | BIGINT | NO | Forced checkpoint count at decision time |
| `checkpoint_timeout_sec` | INTEGER | NO | checkpoint_timeout value at decision time |
| `reason` | TEXT | YES | Human-readable explanation of decision |
| `metadata` | JSONB | YES | Algorithm-specific details |

**Primary Key**: `id`

**Indexes**:
- `walrus_history_timestamp_idx` on `(timestamp)` - for efficient range queries and cleanup

**Constraints**:
- `action` must be one of: 'increase', 'decrease', 'capped'
- `old_size_mb` and `new_size_mb` must be positive integers
- `forced_checkpoints` must be non-negative
- `checkpoint_timeout_sec` must be positive (30-86400 per PostgreSQL)

## Action Types

| Action | When Recorded | Size Relationship |
|--------|---------------|-------------------|
| `increase` | Worker increases max_wal_size due to forced checkpoints | new > old |
| `decrease` | Worker shrinks max_wal_size after quiet intervals | new < old |
| `capped` | Calculated size exceeds walrus.max, capped to max | new = walrus.max |

**Note**: `no_change` and `dry_run` mentioned in spec are NOT recorded to avoid table bloat (FR-005).

## Metadata Schema

The `metadata` JSONB column stores algorithm-specific details. Structure varies by action type:

### For 'increase' action:
```json
{
  "delta": 4,
  "multiplier": 5,
  "calculated_size_mb": 5120
}
```

### For 'decrease' action:
```json
{
  "shrink_factor": 0.75,
  "quiet_intervals": 5,
  "calculated_size_mb": 768
}
```

### For 'capped' action:
```json
{
  "delta": 10,
  "multiplier": 11,
  "calculated_size_mb": 11264,
  "walrus_max_mb": 4096
}
```

## Schema DDL

```sql
CREATE SCHEMA IF NOT EXISTS walrus;

CREATE TABLE walrus.history (
    id BIGSERIAL PRIMARY KEY,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT now(),
    action TEXT NOT NULL CHECK (action IN ('increase', 'decrease', 'capped')),
    old_size_mb INTEGER NOT NULL CHECK (old_size_mb > 0),
    new_size_mb INTEGER NOT NULL CHECK (new_size_mb > 0),
    forced_checkpoints BIGINT NOT NULL CHECK (forced_checkpoints >= 0),
    checkpoint_timeout_sec INTEGER NOT NULL CHECK (checkpoint_timeout_sec > 0),
    reason TEXT,
    metadata JSONB
);

CREATE INDEX walrus_history_timestamp_idx ON walrus.history (timestamp);

COMMENT ON TABLE walrus.history IS 'Audit trail of pg_walrus sizing decisions';
COMMENT ON COLUMN walrus.history.action IS 'Decision type: increase, decrease, or capped';
COMMENT ON COLUMN walrus.history.metadata IS 'Algorithm-specific details in JSON format';
```

## State Transitions

```
                    ┌─────────────────────────────────────────┐
                    │                                         │
                    ▼                                         │
    ┌───────────────────────────────┐                        │
    │   Worker Monitoring Cycle     │                        │
    └───────────────────────────────┘                        │
                    │                                         │
                    ▼                                         │
    ┌───────────────────────────────┐                        │
    │   Fetch Checkpoint Stats      │                        │
    └───────────────────────────────┘                        │
                    │                                         │
                    ▼                                         │
    ┌───────────────────────────────┐                        │
    │   Calculate Delta             │                        │
    └───────────────────────────────┘                        │
                    │                                         │
        ┌───────────┴───────────┐                            │
        ▼                       ▼                            │
┌──────────────────┐  ┌──────────────────┐                   │
│ Delta >= Thresh  │  │ Delta < Thresh   │                   │
│ (GROW PATH)      │  │ (SHRINK PATH)    │                   │
└──────────────────┘  └──────────────────┘                   │
        │                       │                            │
        ▼                       ▼                            │
┌──────────────────┐  ┌──────────────────┐                   │
│ Calculate Size   │  │ Incr Quiet Ints  │                   │
└──────────────────┘  └──────────────────┘                   │
        │                       │                            │
        ▼                       │                            │
┌──────────────────┐            │                            │
│ Size > Max?      │            │                            │
│ (YES=capped)     │            │                            │
└──────────────────┘            │                            │
        │                       ▼                            │
        ▼             ┌──────────────────┐                   │
┌──────────────────┐  │ Quiet >= Thresh? │                   │
│ ALTER SYSTEM     │  │ Size > Min?      │                   │
│ + SIGHUP         │  │ Shrink Enabled?  │                   │
└──────────────────┘  └──────────────────┘                   │
        │                       │                            │
        ▼                       ▼ (all YES)                  │
┌──────────────────┐  ┌──────────────────┐                   │
│ INSERT history   │  │ Shrink + SIGHUP  │                   │
│ (action=increase │  └──────────────────┘                   │
│  or capped)      │            │                            │
└──────────────────┘            ▼                            │
        │             ┌──────────────────┐                   │
        │             │ INSERT history   │                   │
        │             │ (action=decrease)│                   │
        │             └──────────────────┘                   │
        │                       │                            │
        └───────────┬───────────┘                            │
                    │                                         │
                    ▼                                         │
    ┌───────────────────────────────┐                        │
    │   Call cleanup_history()      │                        │
    └───────────────────────────────┘                        │
                    │                                         │
                    └─────────────────────────────────────────┘
```

## Retention Policy

| GUC Parameter | Default | Range | Description |
|---------------|---------|-------|-------------|
| `walrus.history_retention_days` | 7 | 0-3650 | Days before cleanup deletes records |

**Cleanup Logic**:
```sql
DELETE FROM walrus.history
WHERE timestamp < now() - interval 'N days'
```

Where N = `walrus.history_retention_days`.

**Special case**: If `retention_days = 0`, all records are deleted on each cleanup call.

## Volume Estimates

| Scenario | Records/Day | 7-Day Retention | 30-Day Retention |
|----------|-------------|-----------------|------------------|
| Stable workload | ~4 | 28 | 120 |
| Bursty workload | ~50 | 350 | 1,500 |
| High churn | ~200 | 1,400 | 6,000 |

**Assumptions**:
- checkpoint_timeout = 5 minutes (default)
- Stable: 1-2 resize events/day
- Bursty: resize every 2-3 intervals during peaks
- High churn: resize every interval during active periods

**Storage per record**: ~200-500 bytes (depending on metadata size)

**Maximum expected table size**: < 10 MB at 7-day retention even with high churn.
