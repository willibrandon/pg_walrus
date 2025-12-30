# Data Model: SQL Observability Functions

**Branch**: `004-sql-observability-functions` | **Date**: 2025-12-30

## Entities

### 1. WalrusState (Shared Memory)

Ephemeral worker state exposed via PostgreSQL shared memory for real-time SQL function access.

```rust
#[derive(Copy, Clone, Default)]
pub struct WalrusState {
    /// Consecutive intervals with low checkpoint activity (delta < threshold)
    quiet_intervals: i32,

    /// Total number of sizing adjustments made since PostgreSQL start
    total_adjustments: i64,

    /// Previous checkpoint count baseline (for delta calculation)
    prev_requested: i64,

    /// Unix timestamp of last analysis cycle (seconds since epoch)
    last_check_time: i64,

    /// Unix timestamp of last sizing adjustment (seconds since epoch)
    last_adjustment_time: i64,
}
```

**Storage**: PostgreSQL shared memory via `PgLwLock<WalrusState>`
**Lifecycle**: Created on extension load, persists until PostgreSQL restart
**Reset Behavior**: `walrus.reset()` writes zeros to all fields

### 2. History Record (Existing Table)

Persistent audit trail of sizing decisions. Already exists from feature 003.

```sql
TABLE walrus.history (
    id BIGSERIAL PRIMARY KEY,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT now(),
    action TEXT NOT NULL CHECK (action IN ('increase', 'decrease', 'capped')),
    old_size_mb INTEGER NOT NULL,
    new_size_mb INTEGER NOT NULL,
    forced_checkpoints BIGINT NOT NULL,
    checkpoint_timeout_sec INTEGER NOT NULL,
    reason TEXT,
    metadata JSONB
)
```

**Relationship**: One-to-many with sizing decisions
**Retention**: Controlled by `walrus.history_retention_days` GUC

### 3. Recommendation (Computed)

Transient calculation result returned by `walrus.recommendation()` and embedded in `walrus.analyze()`.

```rust
pub struct Recommendation {
    /// Current max_wal_size in MB
    current_size_mb: i32,

    /// Recommended max_wal_size in MB (may equal current if no change)
    recommended_size_mb: i32,

    /// Action type: "increase", "decrease", "none", or "error"
    action: String,

    /// Human-readable explanation
    reason: String,

    /// Confidence level 0-100 based on data quality
    confidence: i32,
}
```

**Storage**: None (computed on demand)
**Confidence Calculation**:
- Base: 50 (default with valid stats)
- +20 if checkpoint count > 10 (sufficient samples)
- +15 if quiet_intervals > 0 (stable observation period)
- +15 if prev_requested > 0 (established baseline)
- -50 if stats unavailable (error case)

### 4. Status (Computed)

Transient snapshot of extension state returned by `walrus.status()`.

```rust
pub struct Status {
    // Configuration (from GUCs)
    enabled: bool,
    current_max_wal_size_mb: i32,
    configured_maximum_mb: i32,
    threshold: i32,
    checkpoint_timeout_sec: i32,

    // Shrink configuration
    shrink_enabled: bool,
    shrink_factor: f64,
    shrink_intervals: i32,
    min_size_mb: i32,

    // Worker state (from shmem)
    worker_running: bool,
    last_check_time: Option<TimestampWithTimeZone>,
    last_adjustment_time: Option<TimestampWithTimeZone>,

    // Counters (from shmem)
    total_adjustments: i64,
    quiet_intervals: i32,

    // Derived
    at_ceiling: bool,  // current_max_wal_size_mb >= configured_maximum_mb
}
```

**Storage**: None (computed on demand)

## Entity Relationships

```
┌─────────────────┐
│   GUC Values    │ (persistent postgresql.auto.conf)
│ walrus.enable   │
│ walrus.max      │
│ walrus.threshold│
│ ...             │
└────────┬────────┘
         │ read by
         ▼
┌─────────────────┐      ┌─────────────────┐
│  WalrusState    │◄────▶│  Background     │
│  (shmem)        │ r/w  │  Worker         │
└────────┬────────┘      └────────┬────────┘
         │                        │
         │ read by               │ writes
         ▼                        ▼
┌─────────────────┐      ┌─────────────────┐
│  SQL Functions  │      │ walrus.history  │
│  status()       │      │ (table)         │
│  recommendation()│      └────────┬────────┘
│  analyze()      │               │
│  reset()  ─────────────────────▶│ truncates
│  history() ────────────────────▶│ selects
└─────────────────┘
```

## State Transitions

### quiet_intervals Counter

```
┌───────────────────────────────────────────────────────────┐
│                    PostgreSQL Start                        │
│                           │                                │
│                           ▼                                │
│                   quiet_intervals = 0                      │
│                           │                                │
│              ┌────────────┴────────────┐                   │
│              ▼                         ▼                   │
│     delta >= threshold          delta < threshold          │
│              │                         │                   │
│              ▼                         ▼                   │
│     quiet_intervals = 0        quiet_intervals++           │
│     (may increase size)               │                    │
│              │                         │                    │
│              │         ┌───────────────┴───────────────┐   │
│              │         ▼                               ▼   │
│              │  quiet < shrink_intervals    quiet >= shrink│
│              │         │                               │   │
│              │         │                               ▼   │
│              │         │                        (shrink)   │
│              │         │                    quiet_intervals│
│              │         │                         = 0       │
│              └─────────┴───────────────────────────────┘   │
│                           │                                │
│                           ▼                                │
│                    walrus.reset()                          │
│                           │                                │
│                           ▼                                │
│                   quiet_intervals = 0                      │
│                   (all counters reset)                     │
└───────────────────────────────────────────────────────────┘
```

## Validation Rules

### WalrusState
- `quiet_intervals`: >= 0 (reset to 0 after grow or shrink)
- `total_adjustments`: >= 0 (monotonically increasing except on reset)
- `prev_requested`: >= 0 (checkpoint count, can be 0 on first iteration)
- `last_check_time`: >= 0 (0 means never checked)
- `last_adjustment_time`: >= 0 (0 means never adjusted)

### Recommendation
- `action`: Must be one of: "increase", "decrease", "none", "error"
- `confidence`: 0-100 inclusive
- `recommended_size_mb`: >= min_size_mb, <= max (walrus.max)

### History Record
- All existing constraints from feature 003 apply
- `action`: Must be one of: "increase", "decrease", "capped"
