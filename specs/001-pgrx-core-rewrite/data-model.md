# Data Model: pg_walrus Core Extension

**Date**: 2025-12-29
**Feature**: 001-pgrx-core-rewrite
**Scope**: Internal extension state and configuration

## Overview

pg_walrus is a PostgreSQL extension with no persistent storage. All state is held in-memory during the background worker's lifecycle. This document describes the runtime data structures and GUC configuration.

## Entities

### E1: WorkerState

**Purpose**: Maintains background worker runtime state across checkpoint monitoring cycles.

**Lifecycle**: Created on worker startup, destroyed on worker shutdown.

| Field | Type | Description | Default |
|-------|------|-------------|---------|
| `first_iteration` | `bool` | Skip first cycle to establish baseline | `true` |
| `prev_requested` | `i64` | Previous checkpoint count (for delta calculation) | `0` |

**State Transitions**:

```text
[Start] → first_iteration=true, prev_requested=0
    ↓
[First Latch Wake] → Store current checkpoint count, first_iteration=false
    ↓
[Subsequent Wakes] → Calculate delta, evaluate threshold, maybe resize
    ↓
[SIGTERM] → [Shutdown]
```

**Validation Rules**:
- `prev_requested` must be non-negative
- `first_iteration` transitions from `true` to `false` exactly once

---

### E2: GUC Parameters (walrus.*)

**Purpose**: Runtime configuration controlling extension behavior.

| Parameter | Type | Default | Min | Max | Context | Flags |
|-----------|------|---------|-----|-----|---------|-------|
| `walrus.enable` | bool | `true` | - | - | SIGHUP | - |
| `walrus.max` | int | `4096` (4GB) | `2` | `i32::MAX` | SIGHUP | UNIT_MB |
| `walrus.threshold` | int | `2` | `1` | `1000` | SIGHUP | - |

**Behavior**:
- `walrus.enable = false`: Worker wakes but skips all monitoring/resize logic
- `walrus.max`: New `max_wal_size` will never exceed this value (in MB)
- `walrus.threshold`: Minimum forced checkpoints required to trigger resize

**Context**: All parameters use `SIGHUP` context, allowing runtime changes via:
- `ALTER SYSTEM SET walrus.enable = false;` followed by `SELECT pg_reload_conf();`
- Direct edit of `postgresql.conf` followed by `pg_ctl reload`

---

### E3: CheckpointStats (External/Read-Only)

**Purpose**: Represents checkpoint statistics retrieved from PostgreSQL.

**Source**: `pgstat_fetch_stat_checkpointer()` system function.

| Field | Type | PG 15-16 | PG 17+ | Description |
|-------|------|----------|--------|-------------|
| `requested` | `i64` | `requested_checkpoints` | `num_requested` | Total forced checkpoints since startup |
| `timed` | `i64` | `timed_checkpoints` | `num_timed` | Total timed checkpoints (unused) |

**Access Pattern**:
- Call `pgstat_clear_snapshot()` before fetch to get fresh data
- Calculate delta: `delta = current_requested - prev_requested`
- Store `current_requested` for next cycle

**Error Handling**:
- If `pgstat_fetch_stat_checkpointer()` returns NULL, log warning and skip cycle

---

### E4: AlterSystemContext (Transient)

**Purpose**: Holds PostgreSQL AST nodes for executing ALTER SYSTEM.

**Lifecycle**: Created when resize triggered, freed after transaction commits.

| Component | PostgreSQL Type | Purpose |
|-----------|-----------------|---------|
| `alter_stmt` | `AlterSystemStmt` | Top-level ALTER SYSTEM statement |
| `setstmt` | `VariableSetStmt` | SET clause (`max_wal_size = <value>`) |
| `useval` | `A_Const` | Integer constant node |

**Memory Management**:
- Nodes allocated via `pg_sys::makeNode()` in PostgreSQL memory context
- `setstmt->args` list must be freed via `list_free()` before reassignment
- Transaction commit handles cleanup of other nodes

---

## Relationships

```text
┌─────────────────┐
│  WorkerState    │
│  (in memory)    │
└────────┬────────┘
         │ reads
         ▼
┌─────────────────┐     ┌─────────────────┐
│ CheckpointStats │     │ GUC Parameters  │
│ (from pg_sys)   │     │ (from pgrx GUC) │
└────────┬────────┘     └────────┬────────┘
         │                       │
         │ delta exceeds         │ controls
         │ threshold?            │ behavior
         ▼                       ▼
┌─────────────────┐
│AlterSystemContext│
│   (transient)   │
└────────┬────────┘
         │ executes
         ▼
┌─────────────────┐
│postgresql.auto  │
│   .conf file    │
└─────────────────┘
```

---

## Data Flow

### Monitoring Cycle

```text
1. wait_latch(checkpoint_timeout)
2. Check SUPPRESS_NEXT_SIGHUP flag → skip if self-triggered
3. Check walrus.enable → skip if disabled
4. Check first_iteration → establish baseline on first run
5. Fetch checkpoint stats
6. Calculate delta = current - prev_requested
7. Store current as prev_requested
8. If delta >= walrus.threshold:
   a. Calculate new_size = max_wal_size_mb * (delta + 1)
   b. Cap at walrus.max
   c. Skip if already at cap
   d. Execute ALTER SYSTEM
   e. Send SIGHUP to postmaster
```

### Configuration Reload

```text
1. External SIGHUP received
2. pgrx/PostgreSQL automatically reloads GUC values
3. BackgroundWorker::sighup_received() returns true
4. No explicit action needed (GucSetting values update automatically)
```

---

## Invariants

1. **Single Writer**: Only the background worker modifies `max_wal_size` via ALTER SYSTEM
2. **Monotonic Growth**: `max_wal_size` only increases, never decreases (within extension scope)
3. **Bounded Growth**: `max_wal_size` will never exceed `walrus.max`
4. **Idempotent Skips**: If already at `walrus.max`, no ALTER SYSTEM is issued
5. **No Negative Deltas**: Checkpoint counts only increase (PostgreSQL resets only on restart)
6. **Signal Safety**: Atomic flag prevents duplicate processing of self-triggered SIGHUP
