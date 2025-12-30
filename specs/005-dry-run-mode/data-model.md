# Data Model: Dry-Run Mode

**Feature**: 005-dry-run-mode
**Date**: 2025-12-30

## Entities

### 1. GUC Parameter: `walrus.dry_run`

**Type**: Boolean
**Default**: `false`
**Context**: SIGHUP (requires config reload)

| Attribute | Value |
|-----------|-------|
| Name | `walrus.dry_run` |
| Type | `bool` |
| Default | `false` |
| Min | N/A (boolean) |
| Max | N/A (boolean) |
| Context | `GucContext::Sighup` |
| Flags | `GucFlags::default()` |
| Short desc | "Enable dry-run mode (log decisions without applying)" |
| Long desc | "When enabled, pg_walrus logs sizing decisions but does not execute ALTER SYSTEM." |

**Rust Declaration**:
```rust
pub static WALRUS_DRY_RUN: GucSetting<bool> = GucSetting::<bool>::new(false);
```

### 2. History Record: Dry-Run Action

Extends existing `walrus.history` table (no schema changes).

**New Action Value**: `'dry_run'`

| Column | Type | Dry-Run Behavior |
|--------|------|------------------|
| `id` | BIGSERIAL | Auto-generated |
| `timestamp` | TIMESTAMPTZ | Current time (DEFAULT now()) |
| `action` | TEXT | `'dry_run'` (new value) |
| `old_size_mb` | INTEGER | Current max_wal_size |
| `new_size_mb` | INTEGER | Calculated target size |
| `forced_checkpoints` | BIGINT | Current checkpoint count |
| `checkpoint_timeout_sec` | INTEGER | Current timeout setting |
| `reason` | TEXT | Decision reason |
| `metadata` | JSONB | Extended with dry-run fields |

### 3. Metadata Schema for Dry-Run Records

**Required Fields** (added to existing algorithm metadata):

| Field | Type | Description |
|-------|------|-------------|
| `dry_run` | boolean | Always `true` for dry-run records |
| `would_apply` | string | One of: `'increase'`, `'decrease'`, `'capped'` |

**Grow Decision Metadata**:
```json
{
  "dry_run": true,
  "would_apply": "increase",
  "delta": 5,
  "multiplier": 6,
  "calculated_size_mb": 6144
}
```

**Shrink Decision Metadata**:
```json
{
  "dry_run": true,
  "would_apply": "decrease",
  "shrink_factor": 0.75,
  "quiet_intervals": 5,
  "calculated_size_mb": 3072
}
```

**Capped Decision Metadata**:
```json
{
  "dry_run": true,
  "would_apply": "capped",
  "delta": 10,
  "multiplier": 11,
  "calculated_size_mb": 22528,
  "walrus_max_mb": 4096
}
```

### 4. Log Entry Format

**Entity Type**: PostgreSQL LOG message

| Component | Format |
|-----------|--------|
| Level | LOG |
| Prefix | `pg_walrus [DRY-RUN]:` |
| Action | `would change max_wal_size` |
| Values | `from X MB to Y MB` |
| Reason | `(threshold exceeded)` / `(sustained low activity)` / `(capped at walrus.max)` |

**Full Format**:
```
LOG: pg_walrus [DRY-RUN]: would change max_wal_size from <old> MB to <new> MB (<reason>)
```

**Examples**:
```
LOG: pg_walrus [DRY-RUN]: would change max_wal_size from 1024 MB to 2048 MB (threshold exceeded)
LOG: pg_walrus [DRY-RUN]: would change max_wal_size from 4096 MB to 3072 MB (sustained low activity)
LOG: pg_walrus [DRY-RUN]: would change max_wal_size from 2048 MB to 4096 MB (capped at walrus.max)
```

## Relationships

```
┌─────────────────────────────────────────────────────────────────┐
│                        pg_settings                              │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ walrus.dry_run = true                                    │   │
│  └─────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼ (controls behavior)
┌─────────────────────────────────────────────────────────────────┐
│                     Background Worker                            │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ process_checkpoint_stats()                               │   │
│  │   if dry_run:                                            │   │
│  │     - Log [DRY-RUN] message                              │   │
│  │     - Insert history with action='dry_run'               │   │
│  │     - Skip ALTER SYSTEM                                  │   │
│  │     - Skip SIGHUP                                        │   │
│  │   else:                                                  │   │
│  │     - Execute normal path                                │   │
│  └─────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼ (writes to)
┌─────────────────────────────────────────────────────────────────┐
│                      walrus.history                              │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ action = 'dry_run'                                       │   │
│  │ metadata = {"dry_run": true, "would_apply": "..."}      │   │
│  └─────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼ (visible to)
┌─────────────────────────────────────────────────────────────────┐
│                     PostgreSQL Log                               │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ LOG: pg_walrus [DRY-RUN]: would change max_wal_size...  │   │
│  └─────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

## State Transitions

### GUC State Machine

```
┌──────────────┐     ALTER SYSTEM / SET     ┌──────────────┐
│              │ ─────────────────────────► │              │
│  dry_run =   │                            │  dry_run =   │
│    false     │ ◄───────────────────────── │    true      │
│              │     ALTER SYSTEM / SET     │              │
└──────────────┘                            └──────────────┘
       │                                           │
       ▼                                           ▼
  Normal sizing                              Dry-run mode
  - ALTER SYSTEM                             - Log only
  - SIGHUP                                   - History record
  - History (increase/                       - History (dry_run)
    decrease/capped)                         - No ALTER SYSTEM
                                             - No SIGHUP
```

### Decision Flow (Dry-Run Enabled)

```
                    ┌─────────────────┐
                    │ Checkpoint      │
                    │ stats fetch     │
                    └────────┬────────┘
                             │
                             ▼
                    ┌─────────────────┐
                    │ Calculate delta │
                    └────────┬────────┘
                             │
                    ┌────────┴────────┐
                    │                 │
                    ▼                 ▼
           delta >= threshold   delta < threshold
                    │                 │
                    ▼                 ▼
           ┌───────────────┐   ┌───────────────┐
           │ Calculate new │   │ Increment     │
           │ size (grow)   │   │ quiet_intervals│
           └───────┬───────┘   └───────┬───────┘
                   │                   │
                   │         ┌─────────┴─────────┐
                   │         │                   │
                   │  shrink conditions met?     │
                   │         │                   │
                   │    ┌────┴────┐         ┌────┴────┐
                   │    │   YES   │         │   NO    │
                   │    │ shrink  │         │ (done)  │
                   │    └────┬────┘         └─────────┘
                   │         │
                   ▼         ▼
           ┌─────────────────────────────────┐
           │          DRY-RUN CHECK          │
           │  ┌───────────────────────────┐  │
           │  │ if WALRUS_DRY_RUN.get():  │  │
           │  │   log!("[DRY-RUN]: ...")  │  │
           │  │   insert_history(dry_run) │  │
           │  │   return (no ALTER SYSTEM)│  │
           │  └───────────────────────────┘  │
           └─────────────────────────────────┘
```

## Validation Rules

1. **GUC Value**: Must be valid boolean (`on`/`off`, `true`/`false`, `1`/`0`)
2. **History Metadata**: When `action = 'dry_run'`, metadata MUST contain `dry_run: true` and `would_apply`
3. **Log Format**: `[DRY-RUN]` prefix MUST appear in all dry-run log messages
4. **State Consistency**: Algorithm state (`quiet_intervals`, `prev_requested`) MUST update identically regardless of dry-run setting
