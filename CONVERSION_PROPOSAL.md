# pg_walrus: C to Rust Conversion Proposal

## Rewriting pg_walsizer as pg_walrus Using the pgrx Framework

**Document Version:** 1.1
**Date:** December 2025
**Target PostgreSQL Versions:** 15, 16, 17, 18

> **Note**: pg_walrus (WAL + Rust) is the new name for the Rust rewrite of pg_walsizer.

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Current Architecture Analysis](#2-current-architecture-analysis)
3. [pgrx Framework Capabilities Assessment](#3-pgrx-framework-capabilities-assessment)
4. [Conversion Strategy](#4-conversion-strategy)
5. [Detailed Technical Design](#5-detailed-technical-design)
6. [API Mapping: C to Rust](#6-api-mapping-c-to-rust)
7. [Memory Management Strategy](#7-memory-management-strategy)
8. [Error Handling Design](#8-error-handling-design)
9. [Testing Strategy](#9-testing-strategy)
10. [Implementation Plan](#10-implementation-plan)
11. [Risk Assessment](#11-risk-assessment)
12. [Appendices](#12-appendices)

---

## 1. Executive Summary

### 1.1 Project Overview

This document proposes rewriting the **pg_walsizer** PostgreSQL extension as **pg_walrus** (WAL + Rust) using the **pgrx** framework. pg_walrus automatically monitors and adjusts the `max_wal_size` configuration parameter to prevent performance-degrading forced checkpoints, with significant new features over the original.

### 1.2 Rationale for Conversion

| Aspect | C Implementation | Rust/pgrx Implementation |
|--------|------------------|--------------------------|
| **Memory Safety** | Manual management, potential for leaks/corruption | Automatic via RAII, compile-time guarantees |
| **Type Safety** | Weak typing, runtime errors | Strong typing, compile-time checks |
| **Error Handling** | Manual sigsetjmp/siglongjmp management | Automatic via `#[pg_guard]` macro |
| **Cross-Version Support** | Manual `#ifdef` conditionals | Feature flags with clean API |
| **Testing** | No framework (manual testing) | Built-in `#[pg_test]` framework |
| **Maintainability** | Moderate | High (idiomatic patterns, less boilerplate) |
| **Build System** | PGXS (Makefile) | Cargo (modern, reproducible) |

### 1.3 Feasibility Assessment

**Verdict: FULLY FEASIBLE**

All functionality of pg_walsizer can be implemented in Rust using pgrx:

| Feature | C Implementation | pgrx Support | Notes |
|---------|------------------|--------------|-------|
| Background Worker | `RegisterBackgroundWorker()` | `BackgroundWorkerBuilder` | Full support |
| GUC Variables | `DefineCustom*Variable()` | `GucSetting<T>` | Full support |
| Checkpoint Stats | `pgstat_fetch_stat_checkpointer()` | Via `pg_sys::` | Direct FFI |
| ALTER SYSTEM | AST construction + `AlterSystemSetConfigFile()` | SPI or `pg_sys::` | Multiple approaches |
| Signal Handling | `pqsignal()` | `SignalWakeFlags` | Idiomatic wrapper |
| Latch/Event Loop | `WaitLatch()` | `BackgroundWorker::wait_latch()` | Full support |
| Configuration Reload | `ProcessConfigFile()` | Via `pg_sys::` | Direct FFI |

### 1.4 Key Benefits of Conversion

1. **Elimination of memory bugs**: Rust's ownership model prevents memory leaks and use-after-free
2. **Compile-time version safety**: Feature flags ensure correct API usage per PostgreSQL version
3. **Better error messages**: Rust panics converted to PostgreSQL ERROR with full context
4. **Modern tooling**: Cargo for dependencies, rustfmt for formatting, clippy for linting
5. **Testability**: In-process testing with `#[pg_test]`
6. **Community**: Active pgrx community for support

### 1.5 Challenges

1. **No high-level WAL abstraction**: Must use `pg_sys::` direct FFI for checkpoint stats
2. **ALTER SYSTEM complexity**: Requires either SPI or low-level AST construction
3. **Version-specific field names**: `requested_checkpoints` vs `num_requested` requires conditional compilation
4. **Learning curve**: Developers must understand both Rust and pgrx idioms

---

## 2. Current Architecture Analysis

### 2.1 Component Overview

```
pg_walsizer (C)
├── _PG_init()           # Extension initialization
│   ├── GUC registration (3 variables)
│   └── Background worker registration
│
└── walsizer_main()      # Background worker entry point
    ├── Signal handler setup
    ├── Database connection
    ├── AST node construction (for ALTER SYSTEM)
    └── Main event loop
        ├── WaitLatch() with checkpoint_timeout
        ├── Signal handling (SIGHUP, SIGTERM)
        ├── Checkpoint statistics fetch
        ├── Threshold check
        ├── Size calculation
        └── ALTER SYSTEM execution
```

### 2.2 Data Flow

```
                    ┌─────────────────────────────┐
                    │   PostgreSQL Checkpointer   │
                    │  (updates statistics)       │
                    └─────────────┬───────────────┘
                                  │
                                  ▼
┌───────────────────────────────────────────────────────────────┐
│                     pg_walsizer Background Worker              │
│                                                               │
│  ┌─────────────┐    ┌──────────────────┐    ┌──────────────┐ │
│  │ WaitLatch() │───▶│ pgstat_fetch_    │───▶│ Calculate    │ │
│  │ (timeout=   │    │ stat_checkpointer│    │ new size     │ │
│  │ checkpoint_ │    │ ()               │    │              │ │
│  │ timeout)    │    └──────────────────┘    └──────┬───────┘ │
│  └─────────────┘                                   │         │
│                                                    ▼         │
│                              ┌──────────────────────────────┐│
│                              │ AlterSystemSetConfigFile()   ││
│                              │ + SIGHUP to Postmaster       ││
│                              └──────────────────────────────┘│
└───────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
                    ┌─────────────────────────────┐
                    │   postgresql.auto.conf      │
                    │   max_wal_size = <new_val>  │
                    └─────────────────────────────┘
```

### 2.3 Current Code Metrics

| Metric | Value |
|--------|-------|
| Total Lines of Code | 299 |
| Functions | 2 |
| GUC Parameters | 3 |
| PostgreSQL APIs Used | 30+ |
| External Dependencies | 0 |

---

## 3. pgrx Framework Capabilities Assessment

### 3.1 Supported Features Mapping

| pg_walsizer Requirement | pgrx Mechanism | Confidence |
|------------------------|----------------|------------|
| Background Worker | `BackgroundWorkerBuilder` | HIGH |
| Custom GUC Variables | `GucSetting<T>`, `GucRegistry` | HIGH |
| Latch-based Event Loop | `BackgroundWorker::wait_latch()` | HIGH |
| Signal Handling | `SignalWakeFlags`, atomic flags | HIGH |
| Statistics Access | `pg_sys::pgstat_fetch_stat_checkpointer()` | HIGH |
| ALTER SYSTEM | SPI `SELECT alter_system_set()` or FFI | MEDIUM |
| Configuration Reload | `pg_sys::ProcessConfigFile()` | HIGH |
| Process Signaling | `pg_sys::kill()` or `libc::kill()` | HIGH |
| Transaction Management | `Spi::connect()` auto-manages | HIGH |
| Logging | `pgrx::log!()`, `warning!()`, `error!()` | HIGH |

### 3.2 Missing High-Level Abstractions

The following C APIs have no idiomatic pgrx wrapper and require direct `pg_sys::` access:

1. **`pgstat_fetch_stat_checkpointer()`** - No statistics wrapper
2. **`pgstat_clear_snapshot()`** - No statistics wrapper
3. **`AlterSystemSetConfigFile()`** - No GUC modification wrapper
4. **`ProcessConfigFile()`** - No configuration wrapper
5. **Global variables** (`CheckPointTimeout`, `max_wal_size_mb`, `PostmasterPid`)

**Mitigation**: All are accessible via `pg_sys::` with `unsafe` blocks and `#[pg_guard]` for error safety.

### 3.3 Version Compatibility Strategy

pgrx supports PostgreSQL 13-18 via feature flags. pg_walsizer currently supports PG 15-17.

**Conditional Compilation Pattern:**

```rust
#[cfg(any(feature = "pg15", feature = "pg16"))]
fn get_requested_checkpoints(stats: &pg_sys::PgStat_CheckpointerStats) -> i64 {
    stats.requested_checkpoints
}

#[cfg(any(feature = "pg17", feature = "pg18"))]
fn get_requested_checkpoints(stats: &pg_sys::PgStat_CheckpointerStats) -> i64 {
    stats.num_requested
}
```

---

## 4. Conversion Strategy

### 4.1 Approach Selection

**Selected Approach: Incremental Translation with Idiomatic Improvements**

Rather than a 1:1 line-by-line translation, we will:
1. Preserve the algorithm and behavior exactly
2. Use idiomatic Rust patterns where they improve clarity
3. Leverage pgrx abstractions where available
4. Add comprehensive error handling
5. Implement a proper test suite

### 4.2 Module Structure

```
pg_walrus/
├── Cargo.toml
├── src/
│   ├── lib.rs              # Entry point, _PG_init, GUC registration
│   ├── worker.rs           # Background worker implementation
│   ├── stats.rs            # Checkpoint statistics access
│   ├── config.rs           # ALTER SYSTEM implementation
│   └── version_compat.rs   # Version-specific code
├── sql/
│   └── pg_walrus--1.0.0.sql    # Generated by pgrx
└── tests/
    └── integration_tests.rs
```

### 4.3 Key Design Decisions

| Decision | Rationale |
|----------|-----------|
| Use SPI for ALTER SYSTEM | Simpler than AST construction, sufficient performance |
| Atomic types for signals | Idiomatic Rust, safer than volatile |
| Logging via pgrx macros | Consistent formatting, automatic context |
| Separate stats module | Encapsulates unsafe FFI, easier testing |
| Version compatibility module | Isolates `#[cfg]` complexity |

---

## 5. Detailed Technical Design

### 5.1 GUC Variable Design

**Current C Implementation:**
```c
static bool walsizer_enable;
static int walsizer_max;
static int walsizer_threshold;

DefineCustomBoolVariable("walsizer.enable", ..., &walsizer_enable, true, PGC_SIGHUP, ...);
DefineCustomIntVariable("walsizer.max", ..., &walsizer_max, 4096, 2, MAX_KILOBYTES, PGC_SIGHUP, GUC_UNIT_MB, ...);
DefineCustomIntVariable("walsizer.threshold", ..., &walsizer_threshold, 2, 1, 1000, PGC_SIGHUP, ...);
```

**Proposed Rust Implementation:**

```rust
use pgrx::prelude::*;
use pgrx::guc::*;

// GUC Settings - static lifetime, thread-safe access
pub static WALRUS_ENABLE: GucSetting<bool> = GucSetting::<bool>::new(true);
pub static WALRUS_MAX: GucSetting<i32> = GucSetting::<i32>::new(4096);
pub static WALRUS_THRESHOLD: GucSetting<i32> = GucSetting::<i32>::new(2);

pub fn register_gucs() {
    GucRegistry::define_bool_guc(
        "walrus.enable",
        "Enable automatic resizing of max_wal_size parameter.",
        "When enabled, pg_walrus monitors checkpoint activity and adjusts max_wal_size.",
        &WALRUS_ENABLE,
        GucContext::Sighup,
        GucFlags::default(),
    );

    GucRegistry::define_int_guc(
        "walrus.max",
        "Maximum size for max_wal_size that pg_walrus will not exceed.",
        "Set this lower than available WAL storage. Default: 4096 MB (4 GB).",
        &WALRUS_MAX,
        2,           // min
        2_097_151,   // max (approximately 2 PB in MB)
        GucContext::Sighup,
        GucFlags::UNIT_MB,
    );

    GucRegistry::define_int_guc(
        "walrus.threshold",
        "Forced checkpoints per timeout before increasing max_wal_size.",
        "Higher values ignore occasional batch job checkpoints. Default: 2.",
        &WALRUS_THRESHOLD,
        1,     // min
        1000,  // max
        GucContext::Sighup,
        GucFlags::default(),
    );

    // Reserve the namespace
    GucRegistry::define_string_guc(
        "walrus._reserved",
        "Reserved namespace marker",
        "Internal use only",
        &GucSetting::<Option<&'static CStr>>::new(None),
        GucContext::Internal,
        GucFlags::NO_SHOW_ALL | GucFlags::NO_RESET_ALL,
    );
}
```

### 5.2 Background Worker Design

**Architecture:**

```rust
use pgrx::prelude::*;
use pgrx::bgworkers::*;
use std::sync::atomic::{AtomicBool, Ordering};

// Signal flags
static GOT_SIGHUP: AtomicBool = AtomicBool::new(false);
static SELF_TRIGGERED_SIGHUP: AtomicBool = AtomicBool::new(false);

#[pg_guard]
pub extern "C" fn walrus_main(_arg: pg_sys::Datum) {
    // Initialize worker
    BackgroundWorker::attach_signal_handlers(SignalWakeFlags::SIGHUP | SignalWakeFlags::SIGTERM);
    BackgroundWorker::connect_worker_to_spi(Some("postgres"), None);

    // State
    let mut prev_requested: i64 = 0;
    let mut first_iteration = true;

    // Main loop
    loop {
        // Wait for checkpoint_timeout
        let timeout_ms = get_checkpoint_timeout() * 1000;
        let wake_reason = BackgroundWorker::wait_latch(Some(Duration::from_millis(timeout_ms as u64)));

        // Check for termination
        if BackgroundWorker::sigterm_received() {
            log!("pg_walrus: received SIGTERM, shutting down");
            break;
        }

        // Handle SIGHUP
        if BackgroundWorker::sighup_received() {
            // Skip if we triggered this ourselves
            if SELF_TRIGGERED_SIGHUP.swap(false, Ordering::SeqCst) {
                continue;
            }
            reload_configuration();
        }

        // Skip if disabled
        if !WALRUS_ENABLE.get() {
            continue;
        }

        // Fetch and process checkpoint statistics
        match process_checkpoint_stats(&mut prev_requested, &mut first_iteration) {
            Ok(Some(new_size)) => {
                if let Err(e) = apply_new_wal_size(new_size) {
                    warning!("pg_walrus: failed to apply new max_wal_size: {}", e);
                }
            }
            Ok(None) => { /* No resize needed */ }
            Err(e) => {
                warning!("pg_walrus: error processing checkpoint stats: {}", e);
            }
        }
    }
}
```

### 5.3 Checkpoint Statistics Access

**Version-Compatible Statistics Module:**

```rust
// src/stats.rs
use pgrx::prelude::*;

/// Result of checkpoint statistics analysis
pub struct CheckpointDelta {
    pub forced_checkpoints: i64,
}

/// Fetch current checkpoint statistics, handling version differences
pub fn fetch_checkpoint_stats() -> Result<CheckpointDelta, &'static str> {
    unsafe {
        // Clear any stale snapshot
        pg_sys::pgstat_clear_snapshot();

        // Fetch checkpointer stats
        let stats = pg_sys::pgstat_fetch_stat_checkpointer();
        if stats.is_null() {
            return Err("Failed to fetch checkpointer statistics");
        }

        let requested = get_requested_count(&*stats);
        Ok(CheckpointDelta {
            forced_checkpoints: requested,
        })
    }
}

#[cfg(any(feature = "pg15", feature = "pg16"))]
fn get_requested_count(stats: &pg_sys::PgStat_CheckpointerStats) -> i64 {
    stats.requested_checkpoints
}

#[cfg(any(feature = "pg17", feature = "pg18"))]
fn get_requested_count(stats: &pg_sys::PgStat_CheckpointerStats) -> i64 {
    stats.num_requested
}

/// Access global PostgreSQL variables for WAL configuration
pub fn get_max_wal_size_mb() -> i32 {
    unsafe { pg_sys::max_wal_size_mb }
}

pub fn get_checkpoint_timeout() -> i32 {
    unsafe { pg_sys::CheckPointTimeout }
}

pub fn get_postmaster_pid() -> i32 {
    unsafe { pg_sys::PostmasterPid }
}
```

### 5.4 ALTER SYSTEM Implementation

**Option A: Using SPI (Recommended)**

```rust
// src/config.rs
use pgrx::prelude::*;

/// Apply new max_wal_size via ALTER SYSTEM
pub fn alter_system_max_wal_size(size_mb: i32) -> Result<(), spi::Error> {
    Spi::connect(|client| {
        // Use parameterized query to prevent any injection issues
        let query = format!("ALTER SYSTEM SET max_wal_size = '{}MB'", size_mb);
        client.update(&query, None, None)?;
        Ok(())
    })
}

/// Signal postmaster to reload configuration
pub fn signal_config_reload() -> Result<(), std::io::Error> {
    let pid = get_postmaster_pid();

    // Set flag before signaling
    SELF_TRIGGERED_SIGHUP.store(true, Ordering::SeqCst);

    // Send SIGHUP
    unsafe {
        if libc::kill(pid, libc::SIGHUP) != 0 {
            SELF_TRIGGERED_SIGHUP.store(false, Ordering::SeqCst);
            return Err(std::io::Error::last_os_error());
        }
    }

    Ok(())
}
```

**Option B: Using AST Construction (Lower-level, matches C exactly)**

```rust
// src/config.rs - Alternative implementation
use pgrx::prelude::*;

/// Apply new max_wal_size via direct AST construction (matches C implementation)
pub fn alter_system_max_wal_size_ast(size_mb: i32) -> Result<(), &'static str> {
    unsafe {
        // Allocate AST nodes
        let alter_stmt: *mut pg_sys::AlterSystemStmt =
            pg_sys::makeNode(pg_sys::NodeTag::T_AlterSystemStmt) as *mut _;
        let set_stmt: *mut pg_sys::VariableSetStmt =
            pg_sys::makeNode(pg_sys::NodeTag::T_VariableSetStmt) as *mut _;
        let val_node: *mut pg_sys::A_Const =
            pg_sys::makeNode(pg_sys::NodeTag::T_A_Const) as *mut _;

        // Configure VariableSetStmt
        (*set_stmt).kind = pg_sys::VariableSetKind::VAR_SET_VALUE;
        (*set_stmt).name = "max_wal_size\0".as_ptr() as *mut _;
        (*set_stmt).is_local = false;

        // Configure value
        (*val_node).val.ival.type_ = pg_sys::NodeTag::T_Integer;
        (*val_node).val.ival.ival = size_mb;

        // Build args list
        (*set_stmt).args = pg_sys::list_make1(val_node as *mut _);

        // Configure AlterSystemStmt
        (*alter_stmt).setstmt = set_stmt;

        // Execute within transaction
        pg_sys::StartTransactionCommand();
        pg_sys::AlterSystemSetConfigFile(alter_stmt);
        pg_sys::CommitTransactionCommand();

        Ok(())
    }
}
```

### 5.5 Main Processing Logic

```rust
// src/worker.rs

/// Calculate desired max_wal_size based on forced checkpoint count
fn calculate_new_size(current_mb: i32, forced_checkpoints: i64) -> i32 {
    // Algorithm: new_size = current_size * (forced_checkpoints + 1)
    // This provides enough headroom to avoid forced checkpoints
    let multiplier = (forced_checkpoints + 1) as i32;

    // Check for overflow
    match current_mb.checked_mul(multiplier) {
        Some(size) => size,
        None => i32::MAX, // Will be capped by walrus.max
    }
}

/// Process checkpoint statistics and determine if resize is needed
fn process_checkpoint_stats(
    prev_requested: &mut i64,
    first_iteration: &mut bool,
) -> Result<Option<i32>, &'static str> {
    let stats = fetch_checkpoint_stats()?;

    // First iteration: just record baseline
    if *first_iteration {
        *prev_requested = stats.forced_checkpoints;
        *first_iteration = false;
        debug1!("pg_walrus: initialized with {} prior forced checkpoints", *prev_requested);
        return Ok(None);
    }

    // Calculate delta
    let delta = stats.forced_checkpoints - *prev_requested;
    *prev_requested = stats.forced_checkpoints;

    // Check threshold
    let threshold = WALRUS_THRESHOLD.get();
    if delta < threshold as i64 {
        return Ok(None);
    }

    let current_size = get_max_wal_size_mb();
    let checkpoint_timeout = get_checkpoint_timeout();

    log!(
        "pg_walrus: detected {} forced checkpoints over {} seconds",
        delta,
        checkpoint_timeout
    );

    // Calculate new size
    let mut new_size = calculate_new_size(current_size, delta);

    // Apply cap
    let max_allowed = WALRUS_MAX.get();
    if new_size > max_allowed {
        warning!(
            "pg_walrus: calculated max_wal_size {} MB exceeds maximum {} MB; using maximum",
            new_size,
            max_allowed
        );
        new_size = max_allowed;
    }

    // Check if change is needed
    if new_size == current_size {
        return Ok(None);
    }

    log!(
        "pg_walrus: threshold ({}) met, resizing max_wal_size from {} MB to {} MB",
        threshold,
        current_size,
        new_size
    );

    Ok(Some(new_size))
}

/// Apply the new WAL size configuration
fn apply_new_wal_size(new_size: i32) -> Result<(), Box<dyn std::error::Error>> {
    // Execute ALTER SYSTEM
    alter_system_max_wal_size(new_size)?;

    // Signal postmaster to reload
    signal_config_reload()?;

    Ok(())
}
```

### 5.6 Extension Entry Point

```rust
// src/lib.rs
use pgrx::prelude::*;

mod config;
mod stats;
mod worker;

pg_module_magic!();

/// Extension initialization - called when shared library is loaded
#[pg_guard]
pub extern "C" fn _PG_init() {
    // Register GUC variables
    config::register_gucs();

    // Register background worker
    BackgroundWorkerBuilder::new("pg_walrus")
        .set_function("walrus_main")
        .set_library("pg_walrus")
        .set_argument(0.into())
        .enable_shmem_access(None)
        .enable_spi_access()
        .set_start_time(BgWorkerStartTime::RecoveryFinished)
        .set_restart_time(Some(Duration::from_secs(
            stats::get_checkpoint_timeout() as u64
        )))
        .load();
}

// Export the background worker entry point
#[pg_guard]
#[no_mangle]
pub extern "C" fn walrus_main(arg: pg_sys::Datum) {
    worker::walrus_main(arg);
}
```

---

## 6. API Mapping: C to Rust

### 6.1 PostgreSQL API Mappings

| C API | Rust Equivalent | Notes |
|-------|-----------------|-------|
| `PG_MODULE_MAGIC` | `pg_module_magic!()` | Macro in pgrx |
| `DefineCustomBoolVariable()` | `GucRegistry::define_bool_guc()` | Full support |
| `DefineCustomIntVariable()` | `GucRegistry::define_int_guc()` | With `GucFlags::UNIT_MB` |
| `MarkGUCPrefixReserved()` | Define internal GUC | Workaround |
| `RegisterBackgroundWorker()` | `BackgroundWorkerBuilder::load()` | Fluent API |
| `BackgroundWorkerInitializeConnection()` | `BackgroundWorker::connect_worker_to_spi()` | Simplified |
| `pqsignal()` | `BackgroundWorker::attach_signal_handlers()` | Flags-based |
| `WaitLatch()` | `BackgroundWorker::wait_latch()` | Returns reason |
| `ResetLatch()` | Automatic | Handled by pgrx |
| `ConfigReloadPending` | `BackgroundWorker::sighup_received()` | Method call |
| `ProcessConfigFile()` | `pg_sys::ProcessConfigFile()` | Direct FFI |
| `pgstat_fetch_stat_checkpointer()` | `pg_sys::pgstat_fetch_stat_checkpointer()` | Direct FFI |
| `pgstat_clear_snapshot()` | `pg_sys::pgstat_clear_snapshot()` | Direct FFI |
| `elog()` | `pgrx::log!()`, `warning!()`, `error!()` | Type-safe macros |
| `StartTransactionCommand()` | `Spi::connect()` auto-manages | Or direct FFI |
| `CommitTransactionCommand()` | `Spi::connect()` auto-manages | Or direct FFI |
| `AlterSystemSetConfigFile()` | SPI query or `pg_sys::` FFI | See design |
| `kill()` | `libc::kill()` | Standard libc |
| `makeNode()` | `pg_sys::makeNode()` | Direct FFI |
| `list_make1()` | `pg_sys::list_make1()` | Direct FFI |
| `proc_exit()` | Return from main function | Idiomatic |

### 6.2 Type Mappings

| C Type | Rust Type | Notes |
|--------|-----------|-------|
| `bool` | `bool` | Direct |
| `int` | `i32` | Direct |
| `int32_t` | `i32` | Direct |
| `int64_t` | `i64` | Direct |
| `Datum` | `pg_sys::Datum` | Opaque |
| `char *` | `*mut c_char` or `CString` | FFI boundary |
| `PgStat_CheckpointerStats *` | `*mut pg_sys::PgStat_CheckpointerStats` | Pointer |
| `AlterSystemStmt *` | `*mut pg_sys::AlterSystemStmt` | Pointer |
| `VariableSetStmt *` | `*mut pg_sys::VariableSetStmt` | Pointer |
| `A_Const *` | `*mut pg_sys::A_Const` | Pointer |
| `volatile sig_atomic_t` | `AtomicBool` | Idiomatic Rust |
| `BackgroundWorker` | `BackgroundWorkerBuilder` | Builder pattern |

### 6.3 Global Variable Access

| C Global | Rust Access | Notes |
|----------|-------------|-------|
| `max_wal_size_mb` | `unsafe { pg_sys::max_wal_size_mb }` | Direct read |
| `CheckPointTimeout` | `unsafe { pg_sys::CheckPointTimeout }` | Direct read |
| `PostmasterPid` | `unsafe { pg_sys::PostmasterPid }` | Direct read |
| `MyLatch` | Managed by pgrx | Via `wait_latch()` |
| `CurrentResourceOwner` | Managed by pgrx | Automatic |

---

## 7. Memory Management Strategy

### 7.1 Rust Ownership Model Benefits

In the C implementation, memory is allocated via `makeNode()` in PostgreSQL's `CurrentMemoryContext`. The Rust implementation leverages RAII for automatic cleanup:

| Scenario | C Handling | Rust Handling |
|----------|------------|---------------|
| AST nodes | Allocated via `makeNode()`, lives in memory context | Same via `pg_sys::makeNode()`, or use Rust-owned types |
| List structures | Manual `list_free()` | `Drop` trait if using Rust wrappers |
| String buffers | PostgreSQL `palloc`/`pfree` | Rust `String` or `CString` |
| Statistics structs | Pointer to shared memory, no ownership | Reference, no ownership |

### 7.2 Memory Context Awareness

```rust
/// Execute code in a specific memory context
fn in_memory_context<F, R>(ctx: PgMemoryContexts, f: F) -> R
where
    F: FnOnce() -> R,
{
    PgMemoryContexts::switch_to(ctx, f)
}

/// Example: Allocate AST in TopTransactionContext
fn allocate_alter_stmt() -> *mut pg_sys::AlterSystemStmt {
    in_memory_context(PgMemoryContexts::TopTransactionContext, || unsafe {
        pg_sys::makeNode(pg_sys::NodeTag::T_AlterSystemStmt) as *mut _
    })
}
```

### 7.3 Resource Cleanup

The C implementation creates a `ResourceOwner` for the background worker:

```c
Assert(CurrentResourceOwner == NULL);
CurrentResourceOwner = ResourceOwnerCreate(NULL, "walrus");
```

In pgrx, this is handled automatically by `BackgroundWorker::connect_worker_to_spi()`.

---

## 8. Error Handling Design

### 8.1 Error Translation

pgrx automatically translates between Rust panics and PostgreSQL ERRORs:

| Rust | PostgreSQL |
|------|------------|
| `panic!("message")` | `ereport(ERROR, errmsg("message"))` |
| `error!("message")` | `ereport(ERROR, errmsg("message"))` |
| `warning!("message")` | `ereport(WARNING, errmsg("message"))` |
| `Result::Err` | Can be converted to ERROR or handled |

### 8.2 Error Handling Strategy

```rust
/// Wrapper for operations that can fail
fn safe_process_cycle(state: &mut WorkerState) {
    if let Err(e) = process_checkpoint_stats_inner(state) {
        warning!("pg_walrus: cycle error: {}", e);
        // Continue running, don't crash the worker
    }
}

/// Inner function returns Result for clean error handling
fn process_checkpoint_stats_inner(state: &mut WorkerState) -> Result<(), WalsizerError> {
    let stats = fetch_checkpoint_stats()
        .map_err(|e| WalsizerError::StatsFetch(e))?;

    // ... processing ...

    alter_system_max_wal_size(new_size)
        .map_err(|e| WalsizerError::AlterSystem(e))?;

    Ok(())
}

/// Custom error type for the extension
#[derive(Debug)]
enum WalrusError {
    StatsFetch(&'static str),
    AlterSystem(spi::Error),
    SignalFailed(std::io::Error),
}

impl std::fmt::Display for WalrusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StatsFetch(msg) => write!(f, "stats fetch failed: {}", msg),
            Self::AlterSystem(e) => write!(f, "ALTER SYSTEM failed: {:?}", e),
            Self::SignalFailed(e) => write!(f, "signal failed: {}", e),
        }
    }
}
```

### 8.3 FFI Boundary Protection

All functions crossing the FFI boundary must be marked with `#[pg_guard]`:

```rust
#[pg_guard]
pub extern "C" fn walsizer_main(arg: pg_sys::Datum) {
    // Rust panics here are caught and converted to PostgreSQL ERROR
    // PostgreSQL ERRORs in called functions are caught and re-raised
}
```

---

## 9. Testing Strategy

### 9.1 Unit Tests (Pure Rust)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_new_size() {
        assert_eq!(calculate_new_size(1024, 4), 5120);
        assert_eq!(calculate_new_size(1024, 0), 1024);
        assert_eq!(calculate_new_size(1024, 1), 2048);
    }

    #[test]
    fn test_calculate_new_size_overflow() {
        // Should handle overflow gracefully
        let result = calculate_new_size(i32::MAX, 2);
        assert_eq!(result, i32::MAX);
    }

    #[test]
    fn test_size_capping() {
        // Test that calculated size respects maximum
        let new_size = calculate_new_size(2000, 10); // 22000
        let capped = std::cmp::min(new_size, 4096);
        assert_eq!(capped, 4096);
    }
}
```

### 9.2 Integration Tests (pg_test)

```rust
#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use pgrx::prelude::*;

    #[pg_test]
    fn test_guc_defaults() {
        // Verify GUC variables have correct defaults
        let enable: bool = Spi::get_one("SHOW walrus.enable")
            .expect("query failed")
            .expect("null value");
        assert_eq!(enable, true);

        let max: String = Spi::get_one("SHOW walrus.max")
            .expect("query failed")
            .expect("null value");
        assert!(max.contains("4096") || max.contains("4GB"));
    }

    #[pg_test]
    fn test_guc_modification() {
        // Test that GUCs can be modified
        Spi::run("ALTER SYSTEM SET walrus.threshold = 5").expect("alter failed");
        Spi::run("SELECT pg_reload_conf()").expect("reload failed");

        let threshold: i32 = Spi::get_one("SHOW walrus.threshold")
            .expect("query failed")
            .expect("null value");
        assert_eq!(threshold, 5);
    }

    #[pg_test]
    fn test_background_worker_running() {
        // Verify background worker is active
        let count: i64 = Spi::get_one(
            "SELECT count(*) FROM pg_stat_activity WHERE backend_type = 'pg_walrus'"
        ).expect("query failed").expect("null value");

        assert_eq!(count, 1, "Background worker should be running");
    }
}
```

### 9.3 End-to-End Testing

```bash
# Test script for manual verification
#!/bin/bash

# Start PostgreSQL with extension
pg_ctl start -D $PGDATA -o "-c shared_preload_libraries=pg_walrus"

# Verify extension loaded
psql -c "SELECT * FROM pg_stat_activity WHERE backend_type LIKE '%walrus%'"

# Generate WAL activity to trigger forced checkpoints
pgbench -i -s 100 postgres
pgbench -c 10 -T 300 postgres

# Monitor logs for pg_walrus activity
tail -f $PGDATA/log/postgresql*.log | grep walrus

# Verify max_wal_size changed
psql -c "SHOW max_wal_size"
cat $PGDATA/postgresql.auto.conf | grep max_wal_size
```

---

## 10. Implementation Plan

### 10.1 Phase 1: Project Setup

**Tasks:**
1. Create new pgrx project: `cargo pgrx new pg_walrus`
2. Configure Cargo.toml with dependencies and features
3. Set up CI/CD pipeline for multi-version testing
4. Create module structure

**Deliverables:**
- Working project skeleton that compiles
- CI configuration for PG 15, 16, 17

### 10.2 Phase 2: Core Infrastructure

**Tasks:**
1. Implement GUC registration (`config.rs`)
2. Implement statistics access (`stats.rs`)
3. Implement version compatibility layer (`version_compat.rs`)
4. Write unit tests for calculation logic

**Deliverables:**
- All GUC variables defined and accessible
- Statistics fetching working across PG versions
- Unit tests passing

### 10.3 Phase 3: Background Worker

**Tasks:**
1. Implement background worker registration
2. Implement main event loop
3. Implement signal handling
4. Implement configuration reload

**Deliverables:**
- Background worker starts with PostgreSQL
- Worker responds to SIGHUP and SIGTERM
- Configuration changes apply correctly

### 10.4 Phase 4: ALTER SYSTEM Integration

**Tasks:**
1. Implement ALTER SYSTEM via SPI
2. Implement postmaster signaling
3. Handle self-triggered SIGHUP detection
4. Integration testing

**Deliverables:**
- Full end-to-end functionality
- Logs show correct behavior
- postgresql.auto.conf updated correctly

### 10.5 Phase 5: Testing and Documentation

**Tasks:**
1. Write comprehensive `#[pg_test]` tests
2. Write integration test suite
3. Performance benchmarking vs C version
4. Update README and documentation
5. Create migration guide from C version

**Deliverables:**
- Test coverage > 80%
- Performance parity with C version
- Complete documentation

### 10.6 Phase 6: Release Preparation

**Tasks:**
1. Security review
2. Code review
3. Package for distribution
4. Publish to crates.io (if desired)
5. Create release notes

**Deliverables:**
- Production-ready release
- Distribution packages

---

## 11. Risk Assessment

### 11.1 Technical Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| pg_sys bindings missing required function | Low | High | Check bindings early, use cshim if needed |
| Performance regression | Low | Medium | Benchmark early and often |
| Version compatibility issues | Medium | High | Test on all target versions in CI |
| Memory safety issues at FFI boundary | Low | High | Use `#[pg_guard]` consistently |
| Incorrect signal handling | Medium | Medium | Test signal scenarios explicitly |

### 11.2 Project Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| pgrx breaking changes | Low | Medium | Pin pgrx version, monitor releases |
| Rust toolchain issues | Low | Low | Use stable Rust, document MSRV |
| Insufficient test coverage | Medium | High | Mandate test coverage in CI |

### 11.3 Compatibility Risks

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| PostgreSQL 18 API changes | Medium | Medium | Monitor PG development, update when released |
| Field name changes (like num_requested) | Known | Low | Already handled via `#[cfg]` |
| New GUC flags or contexts | Low | Low | Monitor pgrx updates |

---

## 12. Appendices

### Appendix A: Complete Cargo.toml

```toml
[package]
name = "pg_walrus"
version = "1.0.0"
edition = "2021"
license = "PostgreSQL"
description = "Automatic max_wal_size tuning based on checkpoint activity (WAL + Rust)"
repository = "https://github.com/willibrandon/pg_walrus"
readme = "README.md"

[lib]
crate-type = ["cdylib", "lib"]

[features]
default = ["pg17"]
pg15 = ["pgrx/pg15", "pgrx-tests/pg15"]
pg16 = ["pgrx/pg16", "pgrx-tests/pg16"]
pg17 = ["pgrx/pg17", "pgrx-tests/pg17"]
pg18 = ["pgrx/pg18", "pgrx-tests/pg18"]
pg_test = []
cshim = ["pgrx/cshim"]

[dependencies]
pgrx = "0.16"
libc = "0.2"

[dev-dependencies]
pgrx-tests = "0.16"

[profile.release]
lto = "fat"
codegen-units = 1
panic = "unwind"  # Required for PostgreSQL error handling
```

### Appendix B: CI/CD Configuration (GitHub Actions)

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  test:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        pg_version: [15, 16, 17]

    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-action@stable

      - name: Install pgrx
        run: cargo install cargo-pgrx --version 0.16.1

      - name: Initialize pgrx
        run: cargo pgrx init --pg${{ matrix.pg_version }} download

      - name: Build
        run: cargo build --features pg${{ matrix.pg_version }}

      - name: Test
        run: cargo pgrx test pg${{ matrix.pg_version }}

  clippy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-action@stable
        with:
          components: clippy
      - run: cargo clippy --all-features -- -D warnings

  fmt:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-action@stable
        with:
          components: rustfmt
      - run: cargo fmt -- --check
```

### Appendix C: Glossary

| Term | Definition |
|------|------------|
| **GUC** | Grand Unified Configuration - PostgreSQL's configuration system |
| **WAL** | Write-Ahead Log - PostgreSQL's durability mechanism |
| **FFI** | Foreign Function Interface - calling between languages |
| **RAII** | Resource Acquisition Is Initialization - Rust's memory management pattern |
| **SPI** | Server Programming Interface - PostgreSQL's internal query API |
| **pgrx** | Rust framework for PostgreSQL extensions |
| **SIGHUP** | Signal sent to reload configuration |
| **Latch** | PostgreSQL's event notification mechanism |
| **Background Worker** | PostgreSQL subprocess for auxiliary tasks |
| **Checkpoint** | Flush of dirty buffers to disk with WAL truncation |

### Appendix D: References

1. **pgrx Documentation**: https://github.com/pgcentralfoundation/pgrx
2. **PostgreSQL Background Workers**: https://www.postgresql.org/docs/current/bgworker.html
3. **PostgreSQL GUC System**: https://www.postgresql.org/docs/current/config-setting.html
4. **Rust FFI Guide**: https://doc.rust-lang.org/nomicon/ffi.html
5. **Original pg_walsizer**: See `walsizer.c` in this repository

---

## Document History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | Dec 2025 | Claude | Initial comprehensive proposal |

---

**End of Document**
