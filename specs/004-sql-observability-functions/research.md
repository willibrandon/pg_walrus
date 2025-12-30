# Research: SQL Observability Functions

**Branch**: `004-sql-observability-functions` | **Date**: 2025-12-30

## Phase 0 Research Findings

### 1. PostgreSQL Shared Memory for Worker State

**Decision**: Use pgrx's `PgLwLock<T>` for composite state struct and `PgAtomic<T>` for simple atomics

**Rationale**:
- pgrx provides high-level wrappers for PostgreSQL shared memory: `PgLwLock<T>` and `PgAtomic<T>`
- `PgLwLock<T>` provides read/write locking for complex structures with `.share()` (read) and `.exclusive()` (write)
- `PgAtomic<T>` wraps Rust atomics for lock-free access to simple values
- Both require initialization via `pg_shmem_init!()` macro in `_PG_init()`
- Types must implement `PGRXSharedMemory` trait (primitives, atomics already do)
- Custom structs need `#[derive(Copy, Clone)]` and `unsafe impl PGRXSharedMemory`

**Alternatives Considered**:
1. **Direct pg_sys shmem calls**: Rejected - requires manual memory management and lock handling
2. **Process-local state only**: Rejected - spec requires SQL functions to access worker state
3. **Custom atomic implementation**: Rejected - pgrx already provides `PgAtomic<T>`

**Implementation Pattern** (from pgrx-examples/shmem):
```rust
// For complex struct with multiple fields
#[derive(Copy, Clone, Default)]
pub struct WalrusState {
    quiet_intervals: i32,
    total_adjustments: i64,
    prev_requested: i64,
    last_check_time: i64,      // Unix timestamp
    last_adjustment_time: i64,  // Unix timestamp
}
unsafe impl PGRXSharedMemory for WalrusState {}

static WALRUS_STATE: PgLwLock<WalrusState> = unsafe { PgLwLock::new(c"walrus_state") };

// In _PG_init():
pg_shmem_init!(WALRUS_STATE);

// Worker writes (exclusive lock):
{
    let mut state = WALRUS_STATE.exclusive();
    state.quiet_intervals = 0;
    state.last_check_time = now();
}

// SQL function reads (shared lock):
{
    let state = WALRUS_STATE.share();
    // build JSONB from state fields
}
```

### 2. JSONB Return Type for SQL Functions

**Decision**: Use `pgrx::JsonB` wrapper type with `serde_json::Value`

**Rationale**:
- pgrx provides `JsonB` type that wraps `serde_json::Value`
- Functions return `JsonB(serde_json::json!({ ... }))`
- Already used in history.rs for metadata field
- No additional dependencies needed (serde_json already in Cargo.toml)

**Pattern**:
```rust
use pgrx::JsonB;
use serde_json::json;

#[pg_extern]
fn status() -> JsonB {
    JsonB(json!({
        "enabled": true,
        "current_max_wal_size_mb": 1024,
        // ...
    }))
}
```

### 3. Set-Returning Function (TableIterator) for history()

**Decision**: Use `TableIterator` with named columns via `name!()` macro

**Rationale**:
- pgrx `TableIterator` provides set-returning function (SRF) support
- `name!()` macro specifies column names in the return type
- Pattern matches pg_regress-style expected output
- Used extensively in pgrx-examples/spi_srf

**Pattern** (from spi_srf example):
```rust
#[pg_extern]
fn history() -> Result<
    TableIterator<
        'static,
        (
            name!(timestamp, pgrx::TimestampWithTimeZone),
            name!(action, String),
            name!(old_size_mb, i32),
            name!(new_size_mb, i32),
            name!(forced_checkpoints, i64),
            name!(reason, Option<String>),
        ),
    >,
    spi::Error,
> {
    Spi::connect(|client| {
        let results = client.select("SELECT ... FROM walrus.history", None, &[])?;
        let rows: Vec<_> = results.map(|row| { ... }).collect();
        Ok(TableIterator::new(rows))
    })
}
```

### 4. Superuser Authorization Check

**Decision**: Use `pgrx::pg_sys::superuser()` function

**Rationale**:
- PostgreSQL provides `superuser()` function to check if current user is superuser
- pgrx exposes this via `pg_sys::superuser()`
- Functions requiring superuser (analyze, reset) call this and error if false

**Pattern**:
```rust
#[pg_extern]
fn reset() -> Result<bool, pgrx::spi::Error> {
    if !unsafe { pgrx::pg_sys::superuser() } {
        pgrx::error!("permission denied: walrus.reset() requires superuser");
    }
    // ... proceed with reset
}
```

### 5. Recommendation Algorithm Integration

**Decision**: Extract existing algorithm logic to shared functions callable from both worker and SQL

**Rationale**:
- Worker already has checkpoint stats fetching and calculation logic
- `recommendation()` needs same logic but without applying changes
- Extract to shared module functions:
  - `get_recommendation(current_size, prev_requested, current_requested, threshold, max_allowed, ...) -> Recommendation`
  - Worker calls this then applies
  - `walrus.recommendation()` calls this and returns JSONB

**Data Flow**:
```
SQL: walrus.recommendation()
  -> Read shared memory (prev_requested, quiet_intervals)
  -> Read GUCs (threshold, max, shrink_factor, etc.)
  -> Fetch checkpoint stats
  -> Calculate recommendation
  -> Return JSONB

Worker: process_checkpoint_stats()
  -> Same calculation logic
  -> Apply via ALTER SYSTEM if needed
  -> Update shared memory state
```

### 6. analyze() Function Implementation

**Decision**: Run analysis logic in SQL session context, optionally apply via ALTER SYSTEM

**Rationale**:
- Spec clarifies: "walrus.analyze() runs its own analysis logic in the SQL session context"
- Does NOT signal worker - runs independently
- Uses same algorithm as worker
- When `apply := true`, executes ALTER SYSTEM directly (requires superuser)
- Worker continues unaffected

**Pattern**:
```rust
#[pg_extern]
fn analyze(apply: default!(bool, false)) -> Result<JsonB, pgrx::spi::Error> {
    if apply && !unsafe { pgrx::pg_sys::superuser() } {
        pgrx::error!("permission denied: walrus.analyze(apply := true) requires superuser");
    }

    let recommendation = compute_recommendation();
    let mut applied = false;

    if apply && recommendation.action != "none" {
        execute_alter_system(recommendation.new_size)?;
        applied = true;
    }

    Ok(JsonB(json!({
        "analyzed": true,
        "recommendation": recommendation,
        "applied": applied
    })))
}
```

### 7. Timestamp Handling in Shared Memory

**Decision**: Store Unix timestamps as i64 in shmem, convert to TimestampWithTimeZone in SQL functions

**Rationale**:
- Shared memory requires `Copy` types
- pgrx `TimestampWithTimeZone` is not Copy
- Store as Unix epoch seconds (i64) in shmem
- Convert using `to_timestamp()` in SQL or pgrx conversion in Rust

**Pattern**:
```rust
// In shmem struct
last_check_time: i64,  // Unix timestamp in seconds

// Worker sets:
state.last_check_time = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_secs() as i64;

// SQL function converts:
let ts = pgrx::TimestampWithTimeZone::from_unix_epoch(state.last_check_time);
```

### 8. reset() Shared Memory Write

**Decision**: Direct write to shmem via exclusive lock

**Rationale**:
- Spec clarifies: "reset directly writes zeros to shared memory"
- Worker sees reset state on next cycle
- No signaling needed - just write zeros to shmem fields
- Also clears history table via DELETE

**Pattern**:
```rust
#[pg_extern]
fn reset() -> Result<bool, pgrx::spi::Error> {
    if !unsafe { pgrx::pg_sys::superuser() } {
        pgrx::error!("permission denied");
    }

    // Clear shared memory
    {
        let mut state = WALRUS_STATE.exclusive();
        state.quiet_intervals = 0;
        state.total_adjustments = 0;
        state.prev_requested = 0;
        state.last_check_time = 0;
        state.last_adjustment_time = 0;
    }

    // Clear history table
    Spi::run("DELETE FROM walrus.history")?;

    Ok(true)
}
```

## Summary of Technical Decisions

| Area | Decision | Key Files |
|------|----------|-----------|
| Shared Memory | `PgLwLock<WalrusState>` struct | src/shmem.rs (new) |
| JSONB Returns | `pgrx::JsonB` wrapper | src/functions.rs (new) |
| SRF (history) | `TableIterator` with `name!()` | src/functions.rs |
| Authorization | `pg_sys::superuser()` check | src/functions.rs |
| Timestamps | i64 Unix epoch in shmem | src/shmem.rs, src/functions.rs |
| Algorithm | Extract to shared functions | src/algorithm.rs (new) |
