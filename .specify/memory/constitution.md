<!--
SYNC IMPACT REPORT
==================
Version change: 1.0.0 → 1.1.0
Bump rationale: MINOR - Significant expansion of pgrx best practices principles

Modified principles:
- II. PostgreSQL Extension Safety → II. FFI Boundary Safety (expanded with #[pg_guard] details)
- III. pgrx Idioms → split into multiple specialized sections

Added sections:
- III. Memory Management (new - PgBox, AllocatedBy semantics, memory contexts)
- IV. Background Worker Patterns (expanded from pgrx Idioms)
- V. GUC Configuration (expanded from pgrx Idioms)
- VI. SPI & Database Access (new)
- VII. Version Compatibility (expanded from pgrx Idioms)
- VIII. Test Discipline (expanded with pgrx-specific patterns)
- IX. Anti-Patterns (new - critical mistakes to avoid)
- X. Observability (renumbered)

Removed sections: None

Templates status:
- ✅ plan-template.md - Constitution Check section compatible
- ✅ spec-template.md - Compatible structure
- ✅ tasks-template.md - Compatible structure

Follow-up: None
-->

# pg_walrus Constitution

## Core Principles

### I. No Task Deferral (NON-NEGOTIABLE)

Every task assigned MUST be completed in full. No exceptions.

**PROHIBITED in code:**
- Code markers: `TODO`, `FIXME`, `PLACEHOLDER`, `HACK`, `XXX`, `STUB`, `TBD`, `PENDING`
- Incomplete implementations or functions that panic with "not implemented"
- Missing error handling, edge cases, or validation
- Partial implementations that require follow-up work

**PROHIBITED in communication:**
- Hedging: "you might want to...", "consider adding...", "it would be good to..."
- Future promises: "we can optimize later", "phase 2 work", "future enhancement"
- Responsibility shifting: "you'll need to add...", "don't forget to...", "make sure to..."
- Scope deferral: "out of scope", "beyond scope", "not in scope"
- Minimizing: "basic implementation", "simplified version", "for now"

**REQUIRED behavior:**
- Complete all assigned work in full before marking tasks complete
- Implement all edge cases and error handling immediately
- If genuinely blocked, state `BLOCKER: [specific issue]` and request a decision
- Each task MUST be fully implemented before moving to the next

**Analysis Mode Enforcement:**

When performing specification analysis (e.g., `/speckit.analyze`):

1. **Coverage gaps trigger mandatory task creation** - If analysis identifies requirements, edge cases, or acceptance criteria with zero task coverage, tasks MUST be created. Presenting options to defer or remove is PROHIBITED.

2. **Edge cases in specifications are requirements** - If a spec document lists edge cases, they have the same status as functional requirements. They MUST have task coverage. Asking "should we include edge cases?" is a constitutional violation.

3. **"User decision required" is reserved for true blockers** - This phrase may ONLY be used when:
   - Two requirements directly contradict each other
   - External information is genuinely unavailable
   - The spec contains a logical impossibility

   It MUST NOT be used for coverage gaps, edge cases, or items that "seem optional."

4. **Analysis outputs MUST NOT offer deferral as an option** - Prohibited patterns:
   - "Options: (a) Add now (b) Mark as post-MVP (c) Remove from spec"
   - "User may proceed without changes"
   - "If proceeding without changes..."
   - "Edge case handling will need to be added in future iterations"
   - "Add complexity justification to waive requirement"

5. **Gap detection = task creation** - The analysis workflow is: find gap → create task. NOT: find gap → ask user → maybe create task.

6. **Constitution requirements are non-negotiable** - If the constitution mandates something (e.g., tests), analysis MUST add tasks to satisfy it. Offering "complexity justification" as an escape hatch is PROHIBITED.

**Rationale**: Deferred work creates technical debt, misleads progress tracking, and shifts burden to users. Complete work or explicitly escalate blockers.

### II. FFI Boundary Safety

pgrx operates at the boundary between Rust and PostgreSQL's C code. Understanding and respecting this boundary is critical for extension safety.

**The Two FFI Boundaries:**

1. **Postgres → Rust (external function calls)**
   - `#[pg_guard]` wraps calls to Postgres internal functions
   - Automatically generated for `extern "C" {}` blocks in `pg_sys`
   - Traps Postgres `ERROR` via `sigsetjmp`/`siglongjmp` and converts to Rust `panic!()`
   - Allows Rust destructors to run before error propagates

2. **Rust → Postgres (callback functions)**
   - `#[pg_guard]` wraps `extern "C" fn` functions that Postgres calls
   - Uses `std::panic::catch_unwind()` to trap Rust panics
   - Converts panic to Postgres `ERROR` via `ereport()`
   - Applied automatically by `#[pg_extern]`, `#[pg_operator]`, `#[pg_trigger]`

**REQUIRED:**
- Every `extern "C-unwind"` callback function MUST have `#[pg_guard]`
- Use `pgrx::error!()` for PostgreSQL-compatible error reporting
- Use `pgrx::warning!()` for non-fatal issues
- Let panics propagate in `#[pg_extern]` functions (automatically converted to ERROR)

**PROHIBITED:**
- Calling pgrx/Postgres functions from threads other than the main thread
- Using `std::mem::forget()` on types with destructors (defeats cleanup)
- Skipping `#[pg_guard]` on manually created callback functions
- Unwinding through C stack frames without `#[pg_guard]` protection

**Rationale**: PostgreSQL extensions run in the same process as the database. Crashes or memory corruption can bring down the entire database cluster. `#[pg_guard]` ensures safe error propagation.

### III. Memory Management

pgrx coexists with PostgreSQL's `MemoryContext` system. Correct memory handling prevents crashes and leaks.

**Two Allocation Systems:**

1. **Rust Allocation (Preferred)**
   - Use `Box<T>`, `Vec<T>`, standard Rust types for extension-local state
   - Follows Rust's compile-time lifetime guarantees
   - Freed automatically when out of scope

2. **PostgreSQL Allocation (When Required)**
   - `palloc()` allocates in `CurrentMemoryContext`
   - Lifetime tied to `MemoryContext`, not Rust scopes
   - Freed en masse when context deletes (usually transaction end)

**PgBox Semantics:**
- `PgBox<T, AllocatedByPostgres>`: On drop, pointer NOT freed (Postgres owns it)
- `PgBox<T, AllocatedByRust>`: On drop, pointer IS freed via `pfree()`
- Use `into_pg()` to transfer ownership back to Postgres

**REQUIRED:**
- Track who allocated: `AllocatedByRust` vs `AllocatedByPostgres`
- Use `PgBox::from_pg()` for pointers received from PostgreSQL
- Use `PgBox::alloc()` for Rust-allocated PostgreSQL-managed memory
- Handle NULL pointers (PgBox raises ERROR on dereference if NULL)

**PROHIBITED:**
- Calling `pfree()` on Rust-allocated memory
- Calling Rust drop on PostgreSQL-allocated memory
- Holding Rust references across PostgreSQL callbacks
- Mixing allocation contexts without explicit ownership tracking

**Rationale**: Memory mismanagement in extensions causes database crashes. Clear ownership semantics prevent use-after-free and double-free bugs.

### IV. Background Worker Patterns

Background workers run as separate PostgreSQL processes. Correct implementation ensures reliability and proper lifecycle management.

**Initialization Pattern:**
```rust
#[pg_guard]
pub extern "C-unwind" fn _PG_init() {
    // Verify loaded via shared_preload_libraries
    if unsafe { !pg_sys::process_shared_preload_libraries_in_progress } {
        error!("must be loaded via shared_preload_libraries");
    }

    // Register GUCs first
    // Register background worker
    BackgroundWorkerBuilder::new("worker_name")
        .set_function("worker_main")
        .set_library("extension_name")
        .enable_spi_access()
        .load();
}
```

**Worker Main Loop:**
```rust
#[pg_guard]
#[unsafe(no_mangle)]
pub extern "C-unwind" fn worker_main(_arg: pg_sys::Datum) {
    BackgroundWorker::attach_signal_handlers(
        SignalWakeFlags::SIGHUP | SignalWakeFlags::SIGTERM
    );
    BackgroundWorker::connect_worker_to_spi(Some("postgres"), None);

    while BackgroundWorker::wait_latch(Some(Duration::from_secs(interval))) {
        if BackgroundWorker::sighup_received() {
            // Reload configuration
        }
        // Do work in transactions
        BackgroundWorker::transaction(|| { /* ... */ });
    }
}
```

**REQUIRED:**
- Check `process_shared_preload_libraries_in_progress` in `_PG_init`
- Use `BackgroundWorkerBuilder` for worker registration
- Attach signal handlers for `SIGHUP` and `SIGTERM`
- Use `wait_latch()` for sleep with proper signal handling
- Wrap database access in `BackgroundWorker::transaction()`

**PROHIBITED:**
- Panicking without `#[pg_guard]` in worker functions
- Holding transaction-scoped resources across `WaitLatch` calls
- Ignoring `SIGTERM` (prevents clean shutdown)
- Blocking indefinitely without latch timeout

**Rationale**: Background workers that crash take down connections. Proper signal handling ensures graceful shutdown and configuration reload.

### V. GUC Configuration

GUC (Grand Unified Configuration) parameters provide runtime configuration for extensions.

**Declaration Pattern:**
```rust
static ENABLE: GucSetting<bool> = GucSetting::new(true);
static MAX_SIZE: GucSetting<i32> = GucSetting::new(4096);

#[pg_guard]
pub extern "C-unwind" fn _PG_init() {
    GucRegistry::define_bool_guc(
        "walrus.enable",
        "Enable automatic WAL sizing",
        "Detailed description here",
        &ENABLE,
        GucContext::Sighup,
        GucFlags::default(),
    );
}
```

**GUC Contexts (use appropriate level):**
- `Postmaster`: Requires server restart
- `Sighup`: Requires config reload (HUP signal) - use for most extension settings
- `Suset`: Superuser can change anytime
- `Userset`: Any user can change anytime

**REQUIRED:**
- Register all GUCs in `_PG_init()` before worker registration
- Use `GucContext::Sighup` for runtime-changeable parameters
- Provide descriptive help text for each GUC
- Use appropriate `GucFlags` (e.g., `UNIT_MB` for size parameters)

**PROHIBITED:**
- Registering GUCs after `_PG_init()` completes
- Using `Postmaster` context when `Sighup` would suffice
- Missing help text or descriptions
- Accessing GUC values before registration

**Rationale**: Well-designed GUCs allow operators to tune extension behavior without restarts. Clear descriptions reduce support burden.

### VI. SPI & Database Access

SPI (Server Programming Interface) enables extensions to execute SQL queries.

**Basic Usage:**
```rust
// Simple query
let result: Option<i64> = Spi::get_one("SELECT count(*) FROM pg_class")?;

// Query with parameters
let result = Spi::get_one_with_args::<String>(
    "SELECT name FROM users WHERE id = $1",
    &[user_id.into()],
)?;

// Connection-scoped operations
Spi::connect(|client| {
    client.update("INSERT INTO table (col) VALUES ($1)", None, &[val.into()])?;
    Ok(())
})?;
```

**Background Worker SPI:**
```rust
BackgroundWorker::transaction(|| {
    Spi::connect(|client| {
        // Execute SQL in worker context
        client.select("SELECT ...", None, &[])?
    })
})?;
```

**REQUIRED:**
- Use `Spi::connect()` for operations needing transaction scope
- Use parameterized queries (`$1`, `$2`) for dynamic values
- Wrap background worker SPI calls in `BackgroundWorker::transaction()`
- Handle `spi::Error` results properly

**PROHIBITED:**
- String interpolation for SQL queries (SQL injection risk)
- SPI calls outside of transaction context in background workers
- Ignoring SPI error results

**Rationale**: SPI provides safe database access from extensions. Proper transaction handling prevents data corruption.

### VII. Version Compatibility

pg_walrus supports PostgreSQL 15, 16, 17, and 18. Version-specific code MUST use compile-time feature gates.

**Feature-Gated Code:**
```rust
#[cfg(any(feature = "pg15", feature = "pg16"))]
fn get_checkpoint_count(stats: &PgStat_CheckpointerStats) -> i64 {
    stats.requested_checkpoints
}

#[cfg(any(feature = "pg17", feature = "pg18"))]
fn get_checkpoint_count(stats: &PgStat_CheckpointerStats) -> i64 {
    stats.num_requested
}
```

**REQUIRED:**
- Use `#[cfg(feature = "pgXX")]` for version-specific code paths
- Test against all supported PostgreSQL versions (15, 16, 17, 18)
- Document version-specific behavior differences in code comments
- Provide unified API wrappers that hide version differences

**PROHIBITED:**
- Runtime version detection when compile-time detection suffices
- Untested version-specific code paths
- Breaking API compatibility across PostgreSQL versions without wrapper

**Rationale**: PostgreSQL internals change between versions. Compile-time feature gates ensure correct code for each version and catch incompatibilities at build time.

### VIII. Test Discipline

Tests MUST be written for all functionality. pgrx provides specialized testing infrastructure.

**Test Types:**

1. **`#[pg_test]` - In-Database Tests**
   ```rust
   #[cfg(any(test, feature = "pg_test"))]
   #[pg_schema]
   mod tests {
       use pgrx::prelude::*;

       #[pg_test]
       fn test_function() {
           // Runs inside PostgreSQL transaction
           let result = my_function();
           assert_eq!(result, expected);
       }

       #[pg_test(error = "expected error message")]
       fn test_error_case() {
           my_function_that_errors();
       }
   }
   ```

2. **`#[test]` - Pure Rust Tests**
   ```rust
   #[test]
   fn test_helper_logic() {
       // No database access
       assert_eq!(compute_size(100), 200);
   }
   ```

**Background Worker Testing:**
```rust
#[cfg(test)]
pub mod pg_test {
    pub fn postgresql_conf_options() -> Vec<&'static str> {
        vec!["shared_preload_libraries='pg_walrus'"]
    }
}
```

**REQUIRED:**
- `#[pg_test]` for all functions that interact with PostgreSQL
- `#[test]` for pure Rust logic without database dependencies
- Test each GUC parameter behavior
- Test background worker lifecycle (start/stop)
- Test error conditions and edge cases
- Configure `postgresql_conf_options()` for background worker tests

**Test Execution:**
```bash
cargo pgrx test pg15  # Test against PG 15
cargo pgrx test pg16  # Test against PG 16
cargo pgrx test pg17  # Test against PG 17
cargo pgrx test pg18  # Test against PG 18
```

**PROHIBITED:**
- Skipping tests for any supported PostgreSQL version
- `#[pg_test]` for logic that doesn't need database access
- Missing tests for error paths
- Tests that pass without implementation (false positives)

**Rationale**: PostgreSQL extensions are difficult to debug in production. Comprehensive testing catches issues before deployment.

### IX. Anti-Patterns (PROHIBITED)

Critical mistakes that MUST be avoided in pgrx development:

**Threading Violations:**
```rust
// PROHIBITED: pgrx/Postgres calls from threads
std::thread::spawn(|| {
    pg_sys::relation_open(...); // CRASH
});
```

**Missing Guard:**
```rust
// PROHIBITED: callback without #[pg_guard]
unsafe extern "C" fn callback() {
    panic!("Unwinds into C!"); // SEGFAULT
}

// REQUIRED: always use #[pg_guard]
#[pg_guard]
unsafe extern "C-unwind" fn callback() {
    panic!("Caught and converted to ERROR");
}
```

**Memory Mismanagement:**
```rust
// PROHIBITED: wrong allocator
let rust_vec = vec![1, 2, 3];
pg_sys::pfree(rust_vec.as_ptr() as *mut _); // CRASH

// PROHIBITED: forget defeats cleanup
let file = File::open("/tmp/data").unwrap();
std::mem::forget(file); // LEAK
```

**Rationale**: These patterns cause crashes, memory corruption, or undefined behavior. They must never appear in pg_walrus code.

### X. Observability

All significant operations MUST be observable for debugging and monitoring.

**Logging Levels:**
- `pgrx::error!()` - Fatal errors (aborts transaction)
- `pgrx::warning!()` - Non-fatal issues
- `pgrx::notice!()` - Important information for users
- `pgrx::info!()` - Informational messages
- `pgrx::log!()` - Server log messages
- `pgrx::debug1!()` through `debug5!()` - Debug levels

**REQUIRED:**
- Log configuration changes (SIGHUP handling)
- Log sizing decisions with before/after values
- Include sufficient context for debugging (checkpoint counts, sizes, thresholds)
- Use appropriate log levels (not everything is ERROR or WARNING)

**Statistics Exposure:**
- Expose runtime statistics via SQL functions
- Track sizing history in a queryable format
- Support standard monitoring integrations

**Rationale**: DBAs need visibility into extension behavior for troubleshooting and capacity planning.

## Additional Constraints

### Technology Stack

| Component | Requirement |
|-----------|-------------|
| Language | Rust (latest stable) |
| Framework | pgrx 0.16+ |
| PostgreSQL | 15, 16, 17, 18 |
| Build | cargo-pgrx |
| Testing | cargo pgrx test |

### Performance Requirements

- Background worker MUST NOT block PostgreSQL operations
- Configuration reload MUST complete within 1 second
- Memory overhead MUST be minimal (< 1MB per worker)

### Compatibility Requirements

- Extension MUST load via `shared_preload_libraries`
- GUC changes MUST take effect on SIGHUP (no restart required)
- MUST work alongside common extensions (pg_stat_statements, etc.)

## Development Workflow

### Code Review Requirements

- All PRs MUST pass CI (cargo check, clippy, fmt, test)
- Constitution compliance MUST be verified
- No prohibited code markers or deferral language
- All `unsafe` blocks MUST have safety comments

### Quality Gates

1. **Pre-commit**: `cargo fmt --check && cargo clippy -- -D warnings`
2. **Pre-merge**: All tests pass on all supported PostgreSQL versions
3. **Pre-release**: Manual testing on production-like workload

### Documentation Requirements

- Public functions MUST have rustdoc comments
- GUC parameters MUST be documented with examples
- README MUST reflect current functionality
- `unsafe` blocks MUST have `// SAFETY:` comments

## Governance

This constitution supersedes all other practices. Amendments require:

1. Written proposal with rationale
2. Impact assessment on existing code
3. Migration plan if breaking changes
4. Update to this document with version increment

**Version Policy:**
- MAJOR: Principle removal or redefinition
- MINOR: New principle or section added
- PATCH: Clarifications or typo fixes

**Compliance Review:**
- All PRs MUST verify constitution compliance
- Violations require explicit justification in Complexity Tracking
- Unjustified violations block merge

**Guidance File**: See `CLAUDE.md` for runtime development guidance.

**Version**: 1.1.0 | **Ratified**: 2025-12-29 | **Last Amended**: 2025-12-29
