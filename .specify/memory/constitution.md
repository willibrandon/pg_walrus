<!--
SYNC IMPACT REPORT
==================
Version change: 1.7.0 → 1.8.0
Bump rationale: MINOR - Added XVI. No False Impossibility Claims principle

Modified principles: None

Added sections:
- XVI. No False Impossibility Claims - Prohibits claiming tests/code are impossible when source code is available

Removed sections: None

Templates status:
- ✅ plan-template.md - Compatible (no impossibility specifics)
- ✅ spec-template.md - Compatible (no impossibility specifics)
- ✅ tasks-template.md - Compatible (no impossibility specifics)

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

Tests MUST be written for all functionality. pg_walrus uses three complementary testing approaches.

**Three-Tier Testing Strategy:**

| Tier | Framework | Use Case | Command |
|------|-----------|----------|---------|
| `#[pg_test]` | pgrx-tests | PostgreSQL integration (SPI, GUCs, worker visibility) | `cargo pgrx test pgXX` |
| `#[test]` | Rust standard | Pure Rust logic (calculations, overflow) | `cargo test --lib` |
| pg_regress | PostgreSQL | SQL-based verification (GUC syntax, extension loading) | `cargo pgrx regress pgXX` |

---

**Tier 1: `#[pg_test]` - PostgreSQL Integration Tests**

Tests that run inside PostgreSQL with full access to SPI, GUCs, and system catalogs. Each test runs in a transaction that automatically rolls back (isolation).

```rust
#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use pgrx::prelude::*;

    #[pg_test]
    fn test_guc_default() {
        let result = Spi::get_one::<&str>("SHOW walrus.enable").unwrap();
        assert_eq!(result, Some("on"));
    }

    #[pg_test(error = "invalid value for parameter")]
    fn test_guc_invalid_value() -> Result<(), spi::Error> {
        Spi::run("SET walrus.threshold = -1")
    }
}
```

**Background Worker Testing** requires `postgresql_conf_options()`:
```rust
#[cfg(test)]
pub mod pg_test {
    pub fn setup(_options: Vec<&str>) {}

    pub fn postgresql_conf_options() -> Vec<&'static str> {
        vec!["shared_preload_libraries='pg_walrus'"]
    }
}
```

---

**Tier 2: `#[test]` - Pure Rust Unit Tests**

Tests for pure Rust logic that does not require PostgreSQL.

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_new_size_calculation() {
        // current_size * (delta + 1)
        assert_eq!(calculate_new_size(1024, 3), 4096);
    }

    #[test]
    fn test_overflow_protection() {
        let result = calculate_new_size(i32::MAX / 2, 2);
        assert_eq!(result, i32::MAX);
    }
}
```

---

**Tier 3: pg_regress - SQL Regression Tests**

SQL-based tests using PostgreSQL's native pg_regress framework. Tests verify extension behavior via SQL commands in psql with output comparison.

**Directory structure:**
```text
tests/pg_regress/
├── sql/               # Test SQL scripts
│   ├── setup.sql      # Creates extension (runs first, special)
│   ├── guc_params.sql # GUC parameter tests
│   └── extension_info.sql
├── expected/          # Expected output files
│   ├── setup.out
│   ├── guc_params.out
│   └── extension_info.out
└── results/           # Generated during tests (gitignored)
```

**setup.sql** (required - runs first when database is created):
```sql
CREATE EXTENSION pg_walrus;
```

**Test file pattern** (e.g., guc_params.sql):
```sql
-- Test default values
SHOW walrus.enable;
SHOW walrus.max;
SHOW walrus.threshold;

-- Test setting valid values
SET walrus.enable = false;
SHOW walrus.enable;
```

**pg_regress Commands:**
```bash
cargo pgrx regress pg17                 # Run all pg_regress tests
cargo pgrx regress pg17 guc_params      # Run specific test
cargo pgrx regress pg17 --auto          # Auto-accept new output
cargo pgrx regress pg17 --resetdb       # Reset database first
```

---

**Test Type Decision Matrix:**

| Scenario | Required Test Type |
|----------|-------------------|
| GUC parameter defaults (internal values) | `#[pg_test]` |
| GUC parameter SQL syntax and output | pg_regress |
| Background worker visibility in pg_stat_activity | `#[pg_test]` |
| Size calculation formula | `#[test]` |
| Overflow protection | `#[test]` |
| Error message format verification | pg_regress |
| Extension metadata (pg_extension) | pg_regress |

---

**Multi-Version Testing (REQUIRED):**

All tests MUST pass on all supported PostgreSQL versions:

```bash
# pgrx integration tests
cargo pgrx test pg15 && cargo pgrx test pg16 && cargo pgrx test pg17 && cargo pgrx test pg18

# pg_regress SQL tests
cargo pgrx regress pg15 && cargo pgrx regress pg16 && cargo pgrx regress pg17 && cargo pgrx regress pg18
```

---

**REQUIRED:**
- `#[pg_test]` for all functions that interact with PostgreSQL
- `#[test]` for pure Rust logic without database dependencies
- pg_regress for SQL interface verification
- Test each GUC parameter behavior (both tiers: internal via `#[pg_test]`, SQL via pg_regress)
- Test background worker lifecycle (start/stop)
- Test error conditions and edge cases
- Configure `postgresql_conf_options()` for background worker tests
- All tests MUST pass on PostgreSQL 15, 16, 17, and 18

**PROHIBITED:**
- Skipping tests for any supported PostgreSQL version
- `#[pg_test]` for logic that doesn't need database access
- Missing tests for error paths
- Tests that pass without implementation (false positives)
- Using only one test tier when multiple are appropriate

**Rationale**: PostgreSQL extensions are difficult to debug in production. The three-tier testing strategy ensures comprehensive coverage: Rust unit tests catch logic bugs early, pgrx integration tests verify PostgreSQL interaction, and pg_regress tests validate the SQL interface users interact with.

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

### XI. Test Failure Protocol (NON-NEGOTIABLE)

When a test fails, the implementation MUST be fixed. Tests define the specification.

**ABSOLUTE PROHIBITION:**
- "I can make the test more lenient"
- "We could relax the assertion"
- "This test is too strict"
- "Let me adjust the test expectations"
- "For now, let's just skip this test case"
- "This is flaky"
- "This is tricky"
- Any suggestion to weaken, skip, or bypass tests

**ROOT CAUSE ANALYSIS REQUIRED:**

When a test fails, you MUST:
1. Identify the exact code causing the failure
2. Trace execution to find the root cause
3. Fix the implementation, never the test
4. Re-run tests to verify the fix
5. If the test uncovers a design flaw, state `BLOCKER: [specific design issue]` and request clarification

**Test Integrity Non-Negotiable:**
- Tests define the specification. Implementation MUST meet tests.
- A test failure indicates a gap between specification and implementation.
- Specification gaps MUST be resolved through code changes, never through relaxed tests.
- Tests are the contract. The contract never changes to accommodate weak implementations.

**When Tests Fail:**
- Ask: "What is the actual vs expected behavior?"
- Ask: "Which code path causes this difference?"
- Ask: "What must change in the implementation?"
- Do NOT ask: "Should we relax this test?"

**Red Flag Phrases (NEVER USE):**
- "We can relax this"
- "Make it more lenient"
- "For now, let's accept"
- "We can skip this case"
- "This test is overly strict"
- "Weaken the assertions"
- "Adjust expectations"
- "Be more lenient"

**Rationale**: Tests encode the specification. Weakening tests to accommodate broken implementations defeats the purpose of testing and allows bugs to ship. Root cause analysis ensures the implementation meets requirements.

### XII. Git Attribution (NON-NEGOTIABLE)

Commit messages MUST NOT contain AI assistant attribution or co-authorship claims.

**ABSOLUTE PROHIBITION:**
- `Co-Authored-By: Claude` (any variant)
- `Co-Authored-By: Claude Code` (any variant)
- `Co-Authored-By: Anthropic` (any variant)
- Any AI/LLM co-authorship attribution
- Any Claude attribution in commit messages
- Any mention of AI assistance in commit metadata
- Generated with markers (e.g., "Generated with Claude Code")
- Robot emoji indicators of AI involvement

**Commit Message Format:**
- Focus on WHAT changed and WHY
- Use conventional commit format when appropriate
- No attribution to tools or assistants
- No emoji decorations unless project style requires them

**REQUIRED:**
- Commit messages describe the change, not who/what made it
- Focus on technical content and rationale
- Follow project's existing commit message conventions

**Rationale**: Commit history documents code changes, not tooling. Attribution in commits clutters history, provides no technical value, and shifts focus from the change itself to how it was produced.

### XIII. No Simplification (NON-NEGOTIABLE)

When encountering difficulty, increase effort. Never reduce scope.

**ABSOLUTE PROHIBITION:**
- The word "simplify" and all its forms (simplifying, simplified, simpler, simplification)
- Removing tests that fail instead of fixing them
- Reducing scope when encountering difficulty
- "Let's just..." followed by a reduced approach
- Removing features or tests to make things "work"
- Any reduction in functionality to avoid debugging

**REQUIRED behavior:**
- When something doesn't work, debug it until it works
- When a test fails, fix the implementation to pass the test
- When encountering difficulty, increase effort, not decrease scope
- Maintain all originally intended functionality

**Rationale**: Reducing scope to avoid difficulty produces incomplete software. Debugging and fixing problems is the work. Removing functionality to make problems disappear hides rather than solves them.

### XIV. No Regression (NON-NEGOTIABLE)

Never recommend older language/framework versions to avoid compatibility issues. Adapt code to stricter requirements.

**ABSOLUTE PROHIBITION:**
- Recommending older Rust editions (e.g., "use edition 2021 instead of 2024")
- Recommending older language/framework versions to avoid compatibility issues
- Downgrading dependencies to avoid fixing code
- Any suggestion to use older tooling when newer versions have stricter requirements
- Phrases like "try edition 2021", "downgrade to version X", "use the older API"

**REQUIRED behavior:**
- When newer editions/versions have stricter requirements, ADAPT THE CODE
- Use fully qualified syntax when trait resolution is ambiguous (e.g., `GucSetting::<bool>::new(true)` not `GucSetting::new(true)`)
- Fix lifetime issues, not avoid them by downgrading
- Debug and resolve compatibility issues, never regress
- When a newer compiler/runtime rejects code, fix the code to satisfy the new requirements

**Rust 2024 Edition Specific:**
- Method resolution is stricter - use turbofish syntax: `Type::<Generic>::method()`
- Never suggest "use edition 2021" as a solution
- pgrx `GucSetting` requires: `GucSetting::<bool>::new()`, `GucSetting::<i32>::new()`, etc.
- Trait method disambiguation may require fully qualified paths
- When compiler errors mention ambiguous methods, use explicit type annotations or turbofish

**Version Migration Protocol:**

When encountering build failures after version upgrades:
1. Read the error message carefully - identify what changed
2. Research the new API/syntax requirements
3. Update the code to comply with new requirements
4. Verify the fix works with the newer version
5. Do NOT suggest reverting to an older version

**Rationale**: Downgrading versions to avoid fixing code creates technical debt and prevents benefiting from improvements in newer releases. The solution to stricter requirements is better code, not older tooling.

### XV. File Size Limits (NON-NEGOTIABLE)

Source code files MUST NOT exceed 900 lines of code. Large files indicate poor modularization.

**ABSOLUTE PROHIBITION:**
- Source code files exceeding 900 lines of code (LOC)
- Adding code to a file that would push it over 900 LOC
- Justifying large files with "it's all related" or "splitting would be harder"

**REQUIRED behavior:**
- When a file approaches 900 LOC, proactively split into logical modules
- Extract related functionality into separate files before hitting the limit
- Use Rust's module system to organize code (e.g., `mod submodule;`)
- Prefer multiple focused files over monolithic files

**Measurement:**
- Count all lines including comments and blank lines
- Use `wc -l <file>` or equivalent tooling
- Check file sizes during code review and CI

**Splitting Strategies:**
- Extract pure functions into utility modules
- Separate types/structs from implementations
- Split tests into dedicated test files
- Move GUC definitions to dedicated `guc.rs`
- Extract statistics access to dedicated `stats.rs`
- Separate worker logic from initialization

**When Approaching the Limit:**
1. Identify logical boundaries within the file
2. Create new module files for each boundary
3. Move related functions, types, and tests together
4. Update `mod` declarations and `use` statements
5. Verify all tests pass after splitting

**Rationale**: Large files are difficult to navigate, understand, and maintain. They indicate that a module has too many responsibilities. Enforcing a hard limit forces proactive modularization and results in more focused, testable code units.

### XVI. No False Impossibility Claims (NON-NEGOTIABLE)

Claiming that tests or implementations are "impossible" is PROHIBITED. You have full source code access. Find the correct approach.

**ABSOLUTE PROHIBITION:**
- "This test cannot run because..."
- "This can't be tested here"
- "There's no way to test this"
- "The test is fundamentally impossible"
- "This approach won't work"
- ANY claim that something is impossible without exhaustive proof

**You have NO excuse for claiming impossibility. Available resources:**
- `/Users/brandon/src/pgrx/` - Full pgrx source code with examples and tests
- `/Users/brandon/src/postgres/` - Full PostgreSQL source code with implementation details
- `pg_settings` system catalog with `min_val`, `max_val`, `vartype`, `context` columns
- `pg_catalog` schema with complete PostgreSQL metadata
- The ability to read ANY file and find the correct approach

**When a test fails or an approach seems blocked:**
1. Read the pgrx source to understand how it actually works
2. Read the PostgreSQL source to understand the underlying behavior
3. Query system catalogs (`pg_settings`, `pg_catalog`) for metadata
4. Try alternative SQL syntax, different test approaches, or different APIs
5. Search pgrx-examples for similar patterns
6. The answer EXISTS in the source code. FIND IT.

**The test/implementation is NEVER impossible. The approach is wrong. Fix the approach.**

**If genuinely blocked after 5+ different approaches:**
- State `BLOCKER: Attempted [specific list of approaches], all failed because [specific errors from each]`
- Include file paths and line numbers from source code research
- This MUST demonstrate exhaustive investigation, not assumption

**Rationale**: With full source code for both pgrx and PostgreSQL available locally, there is no excuse for claiming something is impossible. The correct solution exists in the source code. Claiming impossibility without exhaustive investigation is intellectual laziness.

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

**Version**: 1.8.0 | **Ratified**: 2025-12-29 | **Last Amended**: 2025-12-30
