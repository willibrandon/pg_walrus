# Research: History Table Implementation

**Feature**: 003-history-table
**Date**: 2025-12-30

## Research Topics

### 1. Schema and Table Creation via extension_sql!

**Decision**: Use `extension_sql!` macro with `bootstrap` positioning to create schema and table during extension installation.

**Rationale**:
- pgrx's `extension_sql!` macro allows arbitrary SQL to be executed during `CREATE EXTENSION`
- The `bootstrap` positioning ensures schema creation happens before any other generated SQL
- Table and index creation are atomic with extension installation

**Alternatives considered**:
- `extension_sql_file!` - Rejected: inline SQL is sufficient for this small schema
- Creating schema in `_PG_init()` via SPI - Rejected: SPI not available during extension creation; also would re-run on each server start

**Implementation pattern** (from pgrx-examples/custom_sql):
```rust
extension_sql!(
    r#"
    CREATE SCHEMA IF NOT EXISTS walrus;

    CREATE TABLE walrus.history (
        id BIGSERIAL PRIMARY KEY,
        ...
    );

    CREATE INDEX ON walrus.history (timestamp);
    "#,
    name = "create_walrus_schema",
    bootstrap,
);
```

### 2. SPI INSERT with Multiple Parameters

**Decision**: Use `Spi::run_with_args` or `Spi::get_one_with_args` for parameterized INSERT operations.

**Rationale**:
- Parameterized queries prevent SQL injection
- pgrx provides `IntoDatum` trait implementations for standard Rust types
- JSONB can be passed as `pgrx::JsonB` which wraps `serde_json::Value`

**Alternatives considered**:
- String interpolation - Rejected: SQL injection risk (Constitution VI prohibits this)
- Raw SPI via pg_sys - Rejected: pgrx SPI wrapper is safer and more ergonomic

**Implementation pattern** (from pgrx-examples/spi):
```rust
use pgrx::prelude::*;
use pgrx::JsonB;
use serde_json::json;

fn insert_history_record(
    action: &str,
    old_size: i32,
    new_size: i32,
    forced_checkpoints: i64,
    timeout_sec: i32,
    reason: Option<&str>,
    metadata: Option<serde_json::Value>,
) -> Result<(), spi::Error> {
    let jsonb_metadata = metadata.map(JsonB);

    Spi::run_with_args(
        "INSERT INTO walrus.history
         (action, old_size_mb, new_size_mb, forced_checkpoints, checkpoint_timeout_sec, reason, metadata)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
        &[
            action.into(),
            old_size.into(),
            new_size.into(),
            forced_checkpoints.into(),
            timeout_sec.into(),
            reason.into(),
            jsonb_metadata.into(),
        ],
    )
}
```

### 3. JSONB Handling in pgrx

**Decision**: Use `pgrx::JsonB` type which wraps `serde_json::Value` for metadata column.

**Rationale**:
- `JsonB` implements `IntoDatum` and `FromDatum` for seamless PostgreSQL interop
- Uses `serde_json::Value` internally for flexible JSON structure
- Can construct metadata inline with `serde_json::json!` macro

**Alternatives considered**:
- Raw TEXT column with JSON string - Rejected: loses JSONB indexing and validation benefits
- Custom struct with PostgresType derive - Rejected: overkill for flexible metadata

**Implementation pattern** (from pgrx/src/datum/json.rs):
```rust
use pgrx::JsonB;
use serde_json::json;

let metadata = JsonB(json!({
    "shrink_factor": 0.75,
    "quiet_intervals": 5,
    "growth_multiplier": 4
}));
```

### 4. Cleanup Function as pg_extern

**Decision**: Implement `walrus.cleanup_history()` as a `#[pg_extern]` function that can be called from SQL and from background worker via SPI.

**Rationale**:
- `#[pg_extern]` creates a SQL-callable function
- Using `#[pg_schema]` on a module named `walrus` places the function in the walrus schema
- Can be called directly from worker code or scheduled externally (pg_cron)

**Alternatives considered**:
- Internal-only Rust function - Rejected: spec requires SQL-callable cleanup function
- Stored procedure (PL/pgSQL) - Rejected: want all logic in Rust for consistency

**Implementation pattern**:
```rust
#[pg_schema]
mod walrus {
    use pgrx::prelude::*;
    use crate::guc::WALRUS_HISTORY_RETENTION_DAYS;

    #[pg_extern]
    fn cleanup_history() -> Result<i64, spi::Error> {
        let retention_days = WALRUS_HISTORY_RETENTION_DAYS.get();
        let deleted = Spi::get_one::<i64>(
            &format!(
                "DELETE FROM walrus.history
                 WHERE timestamp < now() - interval '{} days'
                 RETURNING count(*)",
                retention_days
            )
        )?;
        Ok(deleted.unwrap_or(0))
    }
}
```

**Note**: The above uses string formatting for the interval, but since `retention_days` is from a validated GUC (integer 0-3650), this is safe. Alternative is to use `$1 * interval '1 day'` with parameter binding.

### 5. Background Worker Transaction Scope

**Decision**: Use `BackgroundWorker::transaction()` to wrap history insert and cleanup operations for proper transaction isolation.

**Rationale**:
- Current worker already uses `wait_latch` loop but doesn't have explicit transaction blocks for SPI
- History insert should not fail the entire monitoring cycle if it errors
- Cleanup should run in its own transaction

**Alternatives considered**:
- Insert without transaction wrapper - Rejected: risks partial state on error
- Single transaction for monitoring + history - Rejected: history failure shouldn't abort resize

**Implementation pattern**:
```rust
// After successful resize
if let Err(e) = BackgroundWorker::transaction(|| {
    insert_history_record(action, old, new, delta, timeout, reason, metadata)
}) {
    pgrx::warning!("pg_walrus: failed to log history: {}", e);
}

// After monitoring cycle
if let Err(e) = BackgroundWorker::transaction(|| {
    cleanup_old_history()
}) {
    pgrx::warning!("pg_walrus: failed to cleanup history: {}", e);
}
```

### 6. GUC Registration for history_retention_days

**Decision**: Add new integer GUC following existing pattern in `guc.rs`.

**Rationale**:
- Consistent with existing `walrus.*` GUC parameters
- Uses `GucContext::Sighup` for runtime configurability
- Range validation via pgrx GUC infrastructure

**Implementation pattern** (matching existing guc.rs style):
```rust
pub static WALRUS_HISTORY_RETENTION_DAYS: GucSetting<i32> = GucSetting::<i32>::new(7);

// In register_gucs():
GucRegistry::define_int_guc(
    "walrus.history_retention_days",
    "Days to retain history records before automatic cleanup",
    "Records older than this are deleted by cleanup_history(). Range: 0-3650.",
    &WALRUS_HISTORY_RETENTION_DAYS,
    0,      // min
    3650,   // max (10 years)
    GucContext::Sighup,
    GucFlags::default(),
);
```

## Dependency Map

```
extension_sql! (bootstrap)
    └── Creates walrus schema
    └── Creates walrus.history table
    └── Creates timestamp index

guc.rs
    └── WALRUS_HISTORY_RETENTION_DAYS

history.rs (NEW)
    ├── insert_history_record() - internal Rust function
    └── cleanup_old_history() - internal Rust function

walrus schema module (in lib.rs or separate file)
    └── cleanup_history() - #[pg_extern] SQL-callable

worker.rs
    ├── Calls insert_history_record() after resize
    └── Calls cleanup_old_history() after monitoring cycle
```

## Test Strategy

| Test Type | Scope | Location |
|-----------|-------|----------|
| `#[pg_test]` | Table exists after CREATE EXTENSION | lib.rs tests |
| `#[pg_test]` | insert_history_record creates row | history.rs tests |
| `#[pg_test]` | cleanup_history deletes old rows | history.rs tests |
| `#[pg_test]` | GUC retention_days accessible | lib.rs tests |
| pg_regress | SQL function walrus.cleanup_history() works | cleanup.sql |
| pg_regress | History table queryable with filters | history.sql |
| `#[test]` | Metadata JSON structure (pure Rust) | history.rs unit tests |
