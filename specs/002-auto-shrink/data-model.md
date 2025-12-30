# Data Model: Auto-Shrink Feature

**Date**: 2025-12-30
**Feature**: 002-auto-shrink

## Entities

### 1. GUC Parameters (Persistent Configuration)

Four new GUC parameters extending the existing three (`walrus.enable`, `walrus.max`, `walrus.threshold`):

| Parameter | Type | Default | Range | Unit | Context | Description |
|-----------|------|---------|-------|------|---------|-------------|
| `walrus.shrink_enable` | bool | true | - | - | SIGHUP | Enable/disable automatic shrinking |
| `walrus.shrink_factor` | f64 | 0.75 | (0.01, 0.99) | - | SIGHUP | Multiply factor when shrinking |
| `walrus.shrink_intervals` | i32 | 5 | [1, 1000] | - | SIGHUP | Quiet intervals before shrink |
| `walrus.min_size` | i32 | 1024 | [2, i32::MAX] | MB | SIGHUP | Minimum max_wal_size floor |

**Storage**: PostgreSQL's GUC system (postgresql.conf / postgresql.auto.conf)

**Relationships**:
- `walrus.min_size` constrains shrink target (floor)
- `walrus.shrink_enable` independent of `walrus.enable` (can disable shrink while grow active)
- `walrus.threshold` shared by both grow and shrink logic (defines "quiet" vs "active")

### 2. Quiet Interval Counter (Runtime State)

| Field | Type | Lifetime | Description |
|-------|------|----------|-------------|
| `quiet_intervals` | i32 | In-memory, worker process | Consecutive intervals with delta < threshold |

**Lifecycle**:
```
Initial: 0
├── On delta < threshold: increment by 1
├── On delta >= threshold (grow triggers): reset to 0
├── On shrink executed: reset to 0
└── On PostgreSQL restart: reset to 0 (ephemeral state)
```

**Relationships**:
- Compared against `walrus.shrink_intervals` to trigger shrink
- Reset on any resize event (grow or shrink)

### 3. Shrink Decision State (Derived)

Computed each cycle (not persisted):

```
should_shrink = shrink_enable
             AND quiet_intervals >= shrink_intervals
             AND current_size > min_size
             AND grow did not trigger this cycle
```

| Input | Source | Type |
|-------|--------|------|
| `shrink_enable` | GUC `walrus.shrink_enable` | bool |
| `quiet_intervals` | Runtime counter | i32 |
| `shrink_intervals` | GUC `walrus.shrink_intervals` | i32 |
| `current_size` | PostgreSQL `max_wal_size` | i32 (MB) |
| `min_size` | GUC `walrus.min_size` | i32 (MB) |
| `grow_triggered` | Result of grow evaluation | bool |

## State Transitions

### Worker Cycle State Machine

```
                          ┌─────────────────────────┐
                          │    Wait on Latch        │
                          │  (checkpoint_timeout)   │
                          └───────────┬─────────────┘
                                      │
                         ┌────────────▼────────────┐
                         │   Check walrus.enable   │
                         └────────────┬────────────┘
                                      │
                    ┌─────────────────┴─────────────────┐
                    │ enabled                     disabled
                    ▼                                   │
        ┌───────────────────────┐                       │
        │ Fetch checkpoint stats │                      │
        │ Calculate delta       │                       │
        └───────────┬───────────┘                       │
                    │                                   │
        ┌───────────┴───────────┐                       │
        │ delta >= threshold    │ delta < threshold     │
        ▼                       ▼                       │
┌───────────────────┐  ┌────────────────────┐          │
│    GROW PATH      │  │   SHRINK PATH      │          │
│ ─────────────────  │  │ ────────────────── │          │
│ quiet_intervals=0 │  │ quiet_intervals++  │          │
│ Calculate new size│  │                    │          │
│ Cap at walrus.max │  │ Check shrink cond: │          │
│ Execute ALTER SYS │  │ enable && count>=N │          │
│ Send SIGHUP       │  │ && size > min      │          │
│                   │  │ ─────────────────  │          │
│                   │  │ IF true:           │          │
│                   │  │   Calculate shrink │          │
│                   │  │   Clamp at min     │          │
│                   │  │   Execute ALTER SYS│          │
│                   │  │   Send SIGHUP      │          │
│                   │  │   quiet_intervals=0│          │
└─────────┬─────────┘  └─────────┬──────────┘          │
          │                      │                      │
          └──────────┬───────────┘                      │
                     │                                  │
                     └──────────────┬───────────────────┘
                                    │
                                    ▼
                          ┌─────────────────────┐
                          │   Continue Loop     │
                          └─────────────────────┘
```

## Data Validation Rules

### GUC Constraints

| Parameter | Validation | Error Message |
|-----------|------------|---------------|
| `walrus.shrink_factor` | 0.01 ≤ value ≤ 0.99 | "invalid value for parameter" |
| `walrus.shrink_intervals` | 1 ≤ value ≤ 1000 | "invalid value for parameter" |
| `walrus.min_size` | 2 ≤ value ≤ i32::MAX | "invalid value for parameter" |

### Runtime Constraints

| Condition | Behavior |
|-----------|----------|
| `current_size <= min_size` | Skip shrink (already at/below floor) |
| `calculated_shrink < min_size` | Clamp to `min_size` |
| `calculated_shrink >= current_size` | Skip shrink (would not reduce) |

## Formulas

### Shrink Size Calculation

```
raw_size = current_size × shrink_factor
shrink_target = ceil(raw_size)
final_size = max(shrink_target, min_size)
```

**Example**:
- current_size = 4096 MB
- shrink_factor = 0.75
- min_size = 1024 MB
- raw_size = 4096 × 0.75 = 3072.0
- shrink_target = ceil(3072.0) = 3072
- final_size = max(3072, 1024) = 3072 MB

### Edge Case: Rounding Up

```
raw_size = 1000 × 0.75 = 750.0
shrink_target = ceil(750.0) = 750
final_size = max(750, 1024) = 1024 MB  # Clamped to min
```

### Edge Case: Near-Minimum

```
raw_size = 1200 × 0.75 = 900.0
shrink_target = ceil(900.0) = 900
final_size = max(900, 1024) = 1024 MB  # Clamped to min
```
