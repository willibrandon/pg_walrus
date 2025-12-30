# Research: pg_walrus Core Extension (pgrx Rewrite)

**Date**: 2025-12-29
**Feature**: 001-pgrx-core-rewrite
**Purpose**: Resolve technical unknowns before implementation

## R1: ALTER SYSTEM Execution from Background Worker

### Decision
Execute `ALTER SYSTEM SET max_wal_size = <value>` using raw pg_sys bindings to construct node trees, wrapped in a transaction via `BackgroundWorker::transaction()`.

### Rationale
- pgrx does not provide a high-level API for ALTER SYSTEM
- The C implementation constructs AST nodes directly (`AlterSystemStmt`, `VariableSetStmt`, `A_Const`)
- `AlterSystemSetConfigFile()` is available in pgrx-pg-sys bindings
- `BackgroundWorker::transaction()` handles `StartTransactionCommand()`/`CommitTransactionCommand()` and resource owner setup

### Implementation Pattern

```rust
use pgrx::pg_sys;
use std::ffi::CString;

unsafe fn alter_max_wal_size(new_value: i32) {
    // Allocate nodes in PostgreSQL memory context
    let alter_stmt = pg_sys::makeNode(pg_sys::NodeTag::T_AlterSystemStmt) as *mut pg_sys::AlterSystemStmt;
    let setstmt = pg_sys::makeNode(pg_sys::NodeTag::T_VariableSetStmt) as *mut pg_sys::VariableSetStmt;
    let useval = pg_sys::makeNode(pg_sys::NodeTag::T_A_Const) as *mut pg_sys::A_Const;

    // Configure VariableSetStmt
    let name = CString::new("max_wal_size").unwrap();
    (*setstmt).kind = pg_sys::VariableSetKind::VAR_SET_VALUE;
    (*setstmt).name = pg_sys::pstrdup(name.as_ptr());
    (*setstmt).is_local = false;

    // Configure A_Const with integer value
    (*useval).val.ival.type_ = pg_sys::NodeTag::T_Integer;
    (*useval).val.ival.ival = new_value;
    (*useval).isnull = false;

    // Build the list and statement
    (*setstmt).args = pg_sys::list_make1(useval as *mut std::ffi::c_void);
    (*alter_stmt).setstmt = setstmt;

    // Execute within transaction
    pg_sys::AlterSystemSetConfigFile(alter_stmt);
}
```

### Alternatives Considered

| Alternative | Reason Rejected |
|-------------|-----------------|
| SPI with `SELECT alter_system(...)` | No such function exists in PostgreSQL |
| Direct file write to postgresql.auto.conf | Bypasses PostgreSQL's config management, unsafe |
| pgrx Spi::run_query("ALTER SYSTEM...") | SPI cannot execute utility commands directly |

---

## R2: Checkpoint Statistics Access

### Decision
Call `pgstat_fetch_stat_checkpointer()` directly via pg_sys, with version-specific field access using `#[cfg(feature = "pgXX")]`.

### Rationale
- Function binding exists in pgrx-pg-sys for all supported versions
- Field name differs between versions: `num_requested` (PG17+) vs `requested_checkpoints` (PG15-16)
- Compile-time feature gates ensure correct code for each PostgreSQL version

### Implementation Pattern

```rust
#[cfg(any(feature = "pg15", feature = "pg16"))]
fn get_requested_checkpoints() -> i64 {
    unsafe {
        pg_sys::pgstat_clear_snapshot();
        let stats = pg_sys::pgstat_fetch_stat_checkpointer();
        if stats.is_null() {
            return -1;
        }
        (*stats).requested_checkpoints
    }
}

#[cfg(any(feature = "pg17", feature = "pg18"))]
fn get_requested_checkpoints() -> i64 {
    unsafe {
        pg_sys::pgstat_clear_snapshot();
        let stats = pg_sys::pgstat_fetch_stat_checkpointer();
        if stats.is_null() {
            return -1;
        }
        (*stats).num_requested
    }
}
```

### Key Constraint
- `pgstat_clear_snapshot()` MUST be called before `pgstat_fetch_stat_checkpointer()` to get fresh statistics

---

## R3: Sending SIGHUP to Postmaster

### Decision
Use `libc::kill()` to send SIGHUP to `pg_sys::PostmasterPid`, with atomic flag to suppress self-triggered signal handling.

### Rationale
- `PostmasterPid` is available as a static variable in pg_sys
- Standard POSIX signal API via libc is idiomatic for cross-platform Rust
- Atomic flag pattern from C implementation prevents redundant loop iterations

### Implementation Pattern

```rust
use std::sync::atomic::{AtomicBool, Ordering};

static SUPPRESS_NEXT_SIGHUP: AtomicBool = AtomicBool::new(false);

fn send_sighup_to_postmaster() {
    SUPPRESS_NEXT_SIGHUP.store(true, Ordering::SeqCst);
    unsafe {
        libc::kill(pg_sys::PostmasterPid, libc::SIGHUP);
    }
}

// In main loop:
fn should_process_sighup() -> bool {
    if SUPPRESS_NEXT_SIGHUP.swap(false, Ordering::SeqCst) {
        return false;  // Skip self-triggered signal
    }
    BackgroundWorker::sighup_received()
}
```

### Memory Ordering
`Ordering::SeqCst` (Sequential Consistency) matches C `volatile sig_atomic_t` semantics and ensures proper visibility across signal handler and main thread.

---

## R4: Reading max_wal_size GUC

### Decision
Read `pg_sys::max_wal_size_mb` directly from the static variable.

### Rationale
- PostgreSQL exposes `max_wal_size_mb` as a global variable in MB units
- No conversion needed since our `walrus.max` GUC also uses MB units
- Value is updated automatically when PostgreSQL processes config reload

### Implementation Pattern

```rust
fn get_current_max_wal_size() -> i32 {
    unsafe { pg_sys::max_wal_size_mb }
}
```

---

## R5: Custom GUC Registration

### Decision
Use pgrx `GucRegistry::define_*_guc()` functions in `_PG_init()` with `GucContext::Sighup` for runtime reload.

### Rationale
- pgrx provides type-safe GUC registration API
- `GucContext::Sighup` allows runtime changes without restart (matching C implementation)
- `GucFlags::UNIT_MB` available for size parameters

### Implementation Pattern

```rust
use pgrx::prelude::*;
use pgrx::guc::{GucContext, GucFlags, GucRegistry, GucSetting};

static WALRUS_ENABLE: GucSetting<bool> = GucSetting::new(true);
static WALRUS_MAX: GucSetting<i32> = GucSetting::new(4096);
static WALRUS_THRESHOLD: GucSetting<i32> = GucSetting::new(2);

#[pg_guard]
pub extern "C-unwind" fn _PG_init() {
    GucRegistry::define_bool_guc(
        "walrus.enable",
        "Enable automatic resizing of max_wal_size parameter.",
        "When enabled, pg_walrus monitors forced checkpoints and adjusts max_wal_size.",
        &WALRUS_ENABLE,
        GucContext::Sighup,
        GucFlags::default(),
    );

    GucRegistry::define_int_guc(
        "walrus.max",
        "Maximum size for max_wal_size that pg_walrus will not exceed.",
        "Set lower than available WAL device storage.",
        &WALRUS_MAX,
        2,           // min
        i32::MAX,    // max
        GucContext::Sighup,
        GucFlags::UNIT_MB,
    );

    GucRegistry::define_int_guc(
        "walrus.threshold",
        "Forced checkpoints per timeout before increasing max_wal_size.",
        "Higher values ignore occasional WAL spikes from batch jobs.",
        &WALRUS_THRESHOLD,
        1,     // min
        1000,  // max
        GucContext::Sighup,
        GucFlags::default(),
    );
}
```

---

## R6: Background Worker Lifecycle

### Decision
Use `BackgroundWorkerBuilder` with standard pgrx patterns, reading `checkpoint_timeout` dynamically for wait interval.

### Rationale
- pgrx `BackgroundWorkerBuilder` abstracts low-level bgworker registration
- `BgWorkerStart_RecoveryFinished` ensures worker only runs on primary
- `checkpoint_timeout` accessed via extern C declaration (see R8)

### Implementation Pattern

```rust
#[pg_guard]
pub extern "C-unwind" fn _PG_init() {
    if unsafe { !pg_sys::process_shared_preload_libraries_in_progress } {
        pgrx::error!("pg_walrus must be loaded via shared_preload_libraries");
    }

    // Register GUCs first
    register_gucs();

    // Register background worker
    BackgroundWorkerBuilder::new("pg_walrus")
        .set_function("walrus_worker_main")
        .set_library("pg_walrus")
        .set_start_time(BgWorkerStartTime::RecoveryFinished)
        .enable_spi_access()
        .load();
}

#[pg_guard]
#[unsafe(no_mangle)]
pub extern "C-unwind" fn walrus_worker_main(_arg: pg_sys::Datum) {
    BackgroundWorker::attach_signal_handlers(
        SignalWakeFlags::SIGHUP | SignalWakeFlags::SIGTERM
    );

    // No SPI database connection needed (we use ALTER SYSTEM, not queries)

    pgrx::log!("pg_walrus worker started");

    let mut first_iteration = true;
    let mut prev_requested: i64 = 0;

    while BackgroundWorker::wait_latch(Some(checkpoint_timeout())) {
        if should_skip_iteration() {
            continue;
        }

        if BackgroundWorker::sighup_received() {
            // Configuration reloaded automatically by pgrx
        }

        if !WALRUS_ENABLE.get() {
            continue;
        }

        // Main monitoring logic
        process_checkpoint_stats(&mut first_iteration, &mut prev_requested);
    }

    pgrx::log!("pg_walrus worker shutting down");
}
```

---

## R7: Transaction Handling for ALTER SYSTEM

### Decision
Use raw `pg_sys::StartTransactionCommand()` / `pg_sys::CommitTransactionCommand()` directly rather than `BackgroundWorker::transaction()`.

### Rationale
- `AlterSystemSetConfigFile()` is a utility function, not a query
- The C implementation uses explicit transaction management
- Resource owner setup is handled by `StartTransactionCommand()`
- `BackgroundWorker::transaction()` is designed for SPI queries, may have incompatible setup

### Implementation Pattern

```rust
fn execute_alter_system(new_value: i32) -> Result<(), &'static str> {
    unsafe {
        // Ensure resource owner exists
        if pg_sys::CurrentResourceOwner.is_null() {
            let name = CString::new("pg_walrus").unwrap();
            pg_sys::CurrentResourceOwner = pg_sys::ResourceOwnerCreate(
                std::ptr::null_mut(),
                name.as_ptr(),
            );
        }

        pg_sys::StartTransactionCommand();

        // Build and execute ALTER SYSTEM
        alter_max_wal_size(new_value);

        pg_sys::CommitTransactionCommand();
    }
    Ok(())
}
```

---

## R8: Accessing CheckPointTimeout GUC Variable

### Decision
Declare `CheckPointTimeout` directly via extern C block, since pgrx does not expose this PostgreSQL global variable.

### Root Cause
- pgrx generates bindings via bindgen from include files in `pgrx-pg-sys/include/pgXX.h`
- These headers do NOT include `postmaster/bgwriter.h` (where `CheckPointTimeout` is declared)
- However, they DO include `access/xlog_internal.h` → `access/xlog.h` (where `CheckPointSegments` is declared)
- That's why `pg_sys::CheckPointSegments` exists but `pg_sys::CheckPointTimeout` does not

### Implementation Pattern

```rust
// In src/stats.rs or src/guc.rs

use std::ffi::c_int;
use std::time::Duration;

/// Direct access to PostgreSQL's checkpoint-related GUC variables.
/// These are exported by PostgreSQL with PGDLLIMPORT but not included
/// in pgrx's default bindgen headers.
extern "C" {
    /// Checkpoint timeout in seconds (default 300, range 30-86400).
    /// Defined in src/backend/postmaster/checkpointer.c
    /// Declared in src/include/postmaster/bgwriter.h
    static CheckPointTimeout: c_int;

    /// Checkpoint warning threshold in seconds (default 30).
    static CheckPointWarning: c_int;

    /// Checkpoint completion target (0.0-1.0, default 0.9).
    static CheckPointCompletionTarget: f64;
}

/// Returns the checkpoint_timeout GUC value in seconds.
#[inline]
pub fn checkpoint_timeout_secs() -> i32 {
    // SAFETY: CheckPointTimeout is exported by PostgreSQL with PGDLLIMPORT,
    // guaranteed to exist and be initialized before any extension code runs.
    unsafe { CheckPointTimeout }
}

/// Returns the checkpoint_timeout as a Duration for use with WaitLatch.
#[inline]
pub fn checkpoint_timeout() -> Duration {
    Duration::from_secs(checkpoint_timeout_secs() as u64)
}
```

### Rationale
- **Zero overhead**: Direct memory read, no function calls or string parsing
- **Always available**: No transaction context or SPI required (works in `_PG_init`)
- **Type-safe**: Returns native `i32`, no string parsing needed
- **Stable API**: `CheckPointTimeout` is a public PostgreSQL symbol with `PGDLLIMPORT`
- **Dynamically updated**: Value changes automatically when PostgreSQL processes SIGHUP

### Alternatives Considered

| Alternative | Reason Rejected |
|-------------|-----------------|
| `pg_sys::GetConfigOption("checkpoint_timeout")` | Requires string parsing, function call overhead |
| `Spi::get_one("SELECT current_setting('checkpoint_timeout')::int")` | Requires active transaction, high overhead, not available in `_PG_init` |
| Hardcoded default (300 seconds) | Does not respect administrator's `checkpoint_timeout` setting |
| Submit PR to pgrx to add header | Upstream dependency, uncertain timeline |

### Verification

The extern declaration is safe because:

1. `CheckPointTimeout` is declared in `src/include/postmaster/bgwriter.h`:
   ```c
   extern PGDLLIMPORT int CheckPointTimeout;
   ```

2. `PGDLLIMPORT` ensures the symbol is exported from the PostgreSQL shared library

3. The variable is initialized in `src/backend/postmaster/checkpointer.c`:
   ```c
   int CheckPointTimeout = 300;
   ```

4. PostgreSQL guarantees this variable exists and is initialized before any extension `_PG_init` runs

---

## Summary

| Topic | Decision | Key API |
|-------|----------|---------|
| ALTER SYSTEM | Raw pg_sys node construction | `AlterSystemSetConfigFile()` |
| Checkpoint Stats | Direct pg_sys call with version gates | `pgstat_fetch_stat_checkpointer()` |
| SIGHUP Signal | libc::kill() with atomic flag | `PostmasterPid`, `AtomicBool` |
| max_wal_size Read | Direct static variable access | `pg_sys::max_wal_size_mb` |
| GUC Registration | pgrx GucRegistry | `GucSetting`, `GucRegistry` |
| Background Worker | pgrx BackgroundWorkerBuilder | `BackgroundWorker::wait_latch()` |
| Transactions | Raw pg_sys transaction commands | `StartTransactionCommand()` |
| CheckPointTimeout | extern C declaration | `static CheckPointTimeout: c_int` |

---

## R9: pgrx-tests Framework Support for shared_preload_libraries

### Decision
Use `postgresql_conf_options()` returning `vec!["shared_preload_libraries='pg_walrus'"]` in the `pg_test` module. The pgrx-tests framework fully supports this configuration.

### How It Works

The pgrx-tests framework (`pgrx-tests/src/framework.rs`) handles `postgresql_conf_options()` as follows:

1. **Line 257**: `initdb(postgresql_conf)?;` - initializes database with config settings
2. **Lines 519-530**: `modify_postgresql_conf()` writes settings to `postgresql.auto.conf`:
   ```rust
   for setting in postgresql_conf {
       contents.push_str(&format!("{setting}\n"));
   }
   let postgresql_auto_conf = pgdata.join("postgresql.auto.conf");
   std::fs::write(postgresql_auto_conf, contents.as_bytes())?;
   ```
3. **Line 259**: `start_pg()` - starts PostgreSQL WITH those settings already applied
4. **Line 263**: `create_extension()?;` - runs CREATE EXTENSION AFTER PostgreSQL is already running

### Execution Order

```text
initdb(postgresql_conf)          ← Settings written to postgresql.auto.conf
        ↓
start_pg()                       ← PostgreSQL starts with shared_preload_libraries
        ↓                           _PG_init() called, bgworker registered
create_extension()               ← SQL objects created (extension already loaded)
        ↓
run tests                        ← Background worker is now running
```

### Implementation Pattern

```rust
// In src/lib.rs

#[cfg(test)]
pub mod pg_test {
    pub fn setup(_options: Vec<&str>) {}

    pub fn postgresql_conf_options() -> Vec<&'static str> {
        vec!["shared_preload_libraries='pg_walrus'"]
    }
}
```

### Critical: _PG_init() Must Check Loading Context

```rust
#[pg_guard]
pub extern "C-unwind" fn _PG_init() {
    // CRITICAL: Only register bgworker when loaded via shared_preload_libraries
    if unsafe { !pg_sys::process_shared_preload_libraries_in_progress } {
        // Not loaded via shared_preload_libraries - skip bgworker registration
        // GUCs can still be registered for SQL-level access
        return;
    }

    register_gucs();

    BackgroundWorkerBuilder::new("pg_walrus")
        .set_function("walrus_worker_main")
        .set_library("pg_walrus")
        // ...
        .load();
}
```

### Why This Matters

Without checking `process_shared_preload_libraries_in_progress`:
- When extension loads via CREATE EXTENSION (SQL), `_PG_init()` is called
- Attempting to register a background worker at this point fails silently
- The bgworker never starts, tests fail

With the check:
- Extension loaded via shared_preload_libraries at postmaster startup → bgworker registered
- Extension loaded via CREATE EXTENSION → GUCs available, bgworker already running from startup

### Existing Examples in pgrx

| Example | postgresql_conf_options() | Has Tests? |
|---------|---------------------------|------------|
| bgworker | `shared_preload_libraries='bgworker'` | No #[pg_test] for worker |
| shmem | `shared_preload_libraries='shmem'` | No #[pg_test] for shmem |
| wal_decoder | `wal_level = logical` | Uses pgrx_tests::initialize() |

### Testing Background Worker Visibility

```rust
#[cfg(any(test, feature = "pg_test"))]
#[pg_schema]
mod tests {
    use pgrx::prelude::*;

    #[pg_test]
    fn test_background_worker_running() {
        let result = Spi::get_one::<bool>(
            "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE backend_type = 'pg_walrus')"
        ).expect("query failed");
        assert_eq!(result, Some(true), "pg_walrus background worker should be running");
    }
}
```

This test WILL pass when:
1. `postgresql_conf_options()` returns `shared_preload_libraries='pg_walrus'`
2. `_PG_init()` checks `process_shared_preload_libraries_in_progress` and registers bgworker only then
3. Worker function is exported with `#[unsafe(no_mangle)]` and `extern "C-unwind"`

---

## R10: pg_regress SQL Tests with pgrx

### Decision

Use `cargo pgrx regress` for SQL-based regression tests that complement `#[pg_test]` integration tests.

### Rationale

- pg_regress verifies extension behavior via SQL commands in psql
- Tests are easier to read and write for SQL-centric scenarios
- Output comparison provides clear diff-based failure diagnosis
- Complements `#[pg_test]` which runs in transaction context

### Directory Structure

```text
tests/pg_regress/
├── sql/                 # Test SQL scripts
│   ├── setup.sql        # Creates extension (runs first, special)
│   ├── guc_params.sql   # GUC parameter tests
│   └── extension_info.sql # Extension metadata tests
├── expected/            # Expected output files
│   ├── setup.out
│   ├── guc_params.out
│   └── extension_info.out
└── results/             # Generated during tests (gitignored)
```

### Execution Commands

```bash
# Run all pg_regress tests for PostgreSQL 17
cargo pgrx regress pg17

# Run specific test
cargo pgrx regress pg17 guc_params

# Auto-accept new test output
cargo pgrx regress pg17 --auto

# Reset database and re-run setup.sql
cargo pgrx regress pg17 --resetdb

# Run with custom PostgreSQL config (for background workers)
cargo pgrx regress pg17 --postgresql-conf shared_preload_libraries=pg_walrus
```

### Key Behaviors

1. **setup.sql is special**: Runs once when database is created, must include `CREATE EXTENSION`
2. **Tests run alphabetically**: Name files to control execution order if needed
3. **Persistent changes**: Unlike `#[pg_test]`, changes persist between tests (use `DROP IF EXISTS` pattern)
4. **Output comparison**: Uses diff against expected/*.out files

### When to Use pg_regress vs #[pg_test]

| Scenario | Recommended |
|----------|-------------|
| Verify SQL syntax and output format | pg_regress |
| Test requiring PostgreSQL SPI/internal APIs | #[pg_test] |
| Error message verification | pg_regress |
| Pure Rust logic (calculations, etc.) | #[test] |
| Background worker visibility | #[pg_test] |
| GUC parameter SQL interface | pg_regress |
| GUC parameter internal values | #[pg_test] |

---

## Summary

| Topic | Decision | Key API |
|-------|----------|---------|
| ALTER SYSTEM | Raw pg_sys node construction | `AlterSystemSetConfigFile()` |
| Checkpoint Stats | Direct pg_sys call with version gates | `pgstat_fetch_stat_checkpointer()` |
| SIGHUP Signal | libc::kill() with atomic flag | `PostmasterPid`, `AtomicBool` |
| max_wal_size Read | Direct static variable access | `pg_sys::max_wal_size_mb` |
| GUC Registration | pgrx GucRegistry | `GucSetting`, `GucRegistry` |
| Background Worker | pgrx BackgroundWorkerBuilder | `BackgroundWorker::wait_latch()` |
| Transactions | Raw pg_sys transaction commands | `StartTransactionCommand()` |
| CheckPointTimeout | extern C declaration | `static CheckPointTimeout: c_int` |
| pgrx-tests shared_preload | postgresql_conf_options() | `vec!["shared_preload_libraries='pg_walrus'"]` |
| pg_regress SQL tests | cargo pgrx regress | `tests/pg_regress/sql/*.sql` |

All technical unknowns resolved. Ready for Phase 1 design artifacts.
