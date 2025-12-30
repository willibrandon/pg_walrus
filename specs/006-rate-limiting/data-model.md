# Data Model: Rate Limiting

**Feature**: 006-rate-limiting
**Date**: 2025-12-30

## Entities

### 1. WalrusState (Shared Memory - Extended)

**Purpose**: Runtime state accessible by both background worker and SQL functions

**Current Fields** (from `shmem.rs`):
| Field | Type | Description |
|-------|------|-------------|
| quiet_intervals | i32 | Consecutive low-activity intervals |
| total_adjustments | i64 | Total adjustments since PostgreSQL start |
| prev_requested | i64 | Previous checkpoint count baseline |
| last_check_time | i64 | Unix timestamp of last analysis cycle |
| last_adjustment_time | i64 | Unix timestamp of last sizing adjustment |

**New Fields** (for rate limiting):
| Field | Type | Default | Description |
|-------|------|---------|-------------|
| last_change_time | i64 | 0 | Unix timestamp of last successful adjustment (for cooldown). Note: May duplicate last_adjustment_time semantically - evaluate if single field suffices. |
| changes_this_hour | i32 | 0 | Count of adjustments in current hour window |
| hour_window_start | i64 | 0 | Unix timestamp when current hour window began |

**Implementation Note**: `last_adjustment_time` already exists and tracks the same concept as `last_change_time`. Decision: Reuse `last_adjustment_time` for cooldown calculations instead of adding `last_change_time`. This reduces field duplication.

**Revised New Fields**:
| Field | Type | Default | Description |
|-------|------|---------|-------------|
| changes_this_hour | i32 | 0 | Count of adjustments in current hour window |
| hour_window_start | i64 | 0 | Unix timestamp when current hour window began |

**State Transitions**:
- On successful adjustment: `last_adjustment_time = now`, `changes_this_hour += 1`, update `hour_window_start` if expired
- On hour window expiry: `changes_this_hour = 1`, `hour_window_start = now`
- On `walrus.reset()`: All fields reset to 0

**Thread Safety**: Protected by `PgLwLock` via pgrx shared memory infrastructure

### 2. GUC Parameters (New)

**Purpose**: Runtime configuration for rate limiting behavior

| Parameter | Type | Default | Min | Max | Context | Description |
|-----------|------|---------|-----|-----|---------|-------------|
| walrus.cooldown_sec | integer | 300 | 0 | 86400 | Sighup | Minimum seconds between adjustments |
| walrus.max_changes_per_hour | integer | 4 | 0 | 1000 | Sighup | Maximum adjustments per rolling hour |

**GucFlags**: `default()` (no special flags needed)

**Validation Rules**:
- `cooldown_sec = 0`: Disables cooldown, only hourly limit applies
- `max_changes_per_hour = 0`: Blocks all automatic adjustments

### 3. History Table (Extended)

**Purpose**: Persistent audit trail of sizing decisions

**Schema Change**: Add 'skipped' to action CHECK constraint

**Current Constraint**:
```sql
CHECK (action IN ('increase', 'decrease', 'capped', 'dry_run'))
```

**New Constraint**:
```sql
CHECK (action IN ('increase', 'decrease', 'capped', 'dry_run', 'skipped'))
```

**New Record Type** (action = 'skipped'):
| Field | Value |
|-------|-------|
| action | 'skipped' |
| old_size_mb | Current max_wal_size |
| new_size_mb | Would-be target size |
| forced_checkpoints | Current checkpoint count |
| checkpoint_timeout_sec | Current timeout value |
| reason | 'cooldown active' or 'hourly limit reached' |
| metadata | `{"blocked_by": "cooldown", "cooldown_remaining_sec": N}` or `{"blocked_by": "hourly_limit", "changes_this_hour": N}` |

### 4. walrus.status() Output (Extended)

**Purpose**: JSONB status including rate limiting metrics

**New Fields**:
| Field | Type | Description |
|-------|------|-------------|
| cooldown_sec | integer | Current cooldown_sec GUC value |
| max_changes_per_hour | integer | Current max_changes_per_hour GUC value |
| cooldown_active | boolean | true if in cooldown period |
| cooldown_remaining_sec | integer | Seconds until cooldown expires (0 if not active) |
| changes_this_hour | integer | Adjustments made in current hour window |
| hourly_window_start | string (ISO 8601) | When current hour window started (null if no adjustments) |
| hourly_limit_reached | boolean | true if changes_this_hour >= max_changes_per_hour |

**Computed Field Logic**:
```
cooldown_active = (last_adjustment_time + cooldown_sec > now) AND (last_adjustment_time > 0)
cooldown_remaining_sec = max(0, last_adjustment_time + cooldown_sec - now) if cooldown_active else 0
hourly_limit_reached = (changes_this_hour >= max_changes_per_hour) AND (max_changes_per_hour > 0)
```

## Relationships

```
┌─────────────────────────────────────────────────────────────────────┐
│                        Background Worker                             │
│                                                                      │
│  ┌──────────────┐    reads     ┌──────────────┐                    │
│  │ GUC Params   │─────────────>│ Rate Limit   │                    │
│  │ cooldown_sec │              │ Check Logic  │                    │
│  │ max_changes  │              └──────┬───────┘                    │
│  └──────────────┘                     │                            │
│                                       │ if blocked                 │
│                                       v                            │
│  ┌──────────────┐    update    ┌──────────────┐                    │
│  │ WalrusState  │<─────────────│ Record Skip  │──────────────────┐ │
│  │ (shmem)      │              │ or Proceed   │                  │ │
│  │              │              └──────────────┘                  │ │
│  │ - last_adj   │                     │                          │ │
│  │ - changes/hr │                     │ if allowed               │ │
│  │ - hr_start   │                     v                          │ │
│  └──────────────┘              ┌──────────────┐    insert        │ │
│         ^                      │ Execute      │───────────>      │ │
│         │                      │ Adjustment   │                  │ │
│         │                      └──────────────┘                  │ │
│         │                                                        │ │
│         │ read                                           insert  │ │
│         │                                                        v │
└─────────┼───────────────────────────────────────────────────────┬┘
          │                                                        │
          v                                                        v
┌──────────────────┐                                  ┌──────────────────┐
│ walrus.status()  │                                  │ walrus.history   │
│ (SQL function)   │                                  │ (table)          │
└──────────────────┘                                  └──────────────────┘
```

## Validation Rules

### GUC Validation
- `cooldown_sec`: Enforced by GucRegistry `min`/`max` parameters
- `max_changes_per_hour`: Enforced by GucRegistry `min`/`max` parameters

### State Invariants
- `changes_this_hour >= 0`
- `hour_window_start = 0` implies `changes_this_hour = 0`
- `last_adjustment_time >= hour_window_start` (if both non-zero)

### History Record Validation
- Existing CHECK constraints remain
- New 'skipped' action requires non-null reason

## Migration

**SQL Migration** (to be added to extension SQL):
```sql
-- Add 'skipped' to action constraint
ALTER TABLE walrus.history
  DROP CONSTRAINT IF EXISTS walrus_history_action_check;

ALTER TABLE walrus.history
  ADD CONSTRAINT walrus_history_action_check
  CHECK (action IN ('increase', 'decrease', 'capped', 'dry_run', 'skipped'));
```

**Note**: This migration runs during `CREATE EXTENSION pg_walrus` for new installations. Existing installations may need manual migration or extension upgrade mechanism.
