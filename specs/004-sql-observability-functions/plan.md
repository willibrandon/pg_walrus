# Implementation Plan: SQL Observability Functions

**Branch**: `004-sql-observability-functions` | **Date**: 2025-12-30 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/004-sql-observability-functions/spec.md`

## Summary

Expose pg_walrus extension state and controls via five SQL functions for monitoring and management. The implementation adds PostgreSQL shared memory for real-time worker state access, JSONB-returning status/recommendation functions, a set-returning history function, and privileged analyze/reset operations.

**Technical Approach**:
- Use pgrx `PgLwLock<WalrusState>` for shared memory state accessible by both worker and SQL functions
- Implement `walrus.status()`, `walrus.recommendation()`, `walrus.analyze()` returning `pgrx::JsonB`
- Implement `walrus.history()` returning `TableIterator` for set-returning behavior
- Implement `walrus.reset()` for administrative state clearing
- Extract sizing algorithm to shared module for use by both worker and `analyze()`

## Technical Context

**Language/Version**: Rust 1.83+ (edition 2024) + pgrx 0.16.1
**Primary Dependencies**: pgrx 0.16.1, serde_json 1.x, libc 0.2
**Storage**: PostgreSQL shared memory (ephemeral), walrus.history table (persistent)
**Testing**: `cargo pgrx test pg15/pg16/pg17/pg18`, `cargo pgrx regress`, `cargo test --lib`
**Target Platform**: PostgreSQL 15, 16, 17, 18 on Linux/macOS
**Project Type**: Single pgrx extension project
**Performance Goals**: `walrus.status()` < 100ms execution time (SC-002)
**Constraints**: < 1MB memory overhead for shared memory, no blocking of PostgreSQL operations
**Scale/Scope**: 5 new SQL functions, 1 new shmem module, algorithm extraction

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Gate | Status | Notes |
|------|--------|-------|
| I. No Task Deferral | PASS | All 5 functions fully specified with edge cases |
| II. FFI Boundary Safety | PASS | `#[pg_extern]` for SQL functions, `#[pg_guard]` for callbacks |
| III. Memory Management | PASS | Using pgrx `PgLwLock<T>` for safe shmem access |
| IV. Background Worker Patterns | PASS | Worker state moved to shmem, `pg_test` module present |
| V. GUC Configuration | PASS | Existing GUCs, no new ones required |
| VI. SPI & Database Access | PASS | `history()` uses parameterized SPI queries |
| VII. Version Compatibility | PASS | No version-specific code in new functions |
| VIII. Test Discipline | REQUIRES TASKS | Tests for all 5 functions across all tiers |
| IX. Anti-Patterns | PASS | No threading, proper guard usage |
| X. Observability | PASS | Functions ARE the observability layer |
| XI. Test Failure Protocol | ACKNOWLEDGED | Fix implementations, never tests |
| XII. Git Attribution | ACKNOWLEDGED | No AI attribution in commits |
| XIII. No Simplification | ACKNOWLEDGED | Full implementation required |
| XIV. No Regression | ACKNOWLEDGED | Rust 2024 edition maintained |
| XV. File Size Limits | REQUIRES TASKS | New modules to keep files < 900 LOC |
| XVI. No False Impossibility Claims | ACKNOWLEDGED | Source code available for reference |

## Project Structure

### Documentation (this feature)

```text
specs/004-sql-observability-functions/
├── plan.md              # This file
├── research.md          # Phase 0 output - shmem patterns, JSONB, TableIterator
├── data-model.md        # Phase 1 output - WalrusState, Recommendation, Status entities
├── quickstart.md        # Phase 1 output - usage examples
├── contracts/           # Phase 1 output
│   └── sql-functions.md # Function signatures and return structures
└── tasks.md             # Phase 2 output (NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
src/
├── lib.rs               # Extension entry, _PG_init, pg_test module, existing tests
├── worker.rs            # Background worker (MODIFY: use shmem state)
├── guc.rs               # GUC definitions (no changes)
├── stats.rs             # Checkpoint stats access (no changes)
├── config.rs            # ALTER SYSTEM implementation (no changes)
├── history.rs           # History table operations (no changes)
├── shmem.rs             # NEW: Shared memory state definition and initialization
├── algorithm.rs         # NEW: Extracted recommendation algorithm
└── functions.rs         # NEW: SQL-callable functions (status, history, etc.)

tests/pg_regress/
├── sql/
│   ├── setup.sql        # Creates extension
│   ├── guc_params.sql   # Existing GUC tests
│   ├── shrink_gucs.sql  # Existing shrink GUC tests
│   └── observability.sql # NEW: Tests for all 5 functions
└── expected/
    ├── setup.out
    ├── guc_params.out
    ├── shrink_gucs.out
    └── observability.out # NEW: Expected output for function tests
```

**Structure Decision**: Extension follows existing pgrx single-project structure. New modules added for:
- `shmem.rs`: Isolates shared memory complexity from other code
- `algorithm.rs`: DRY extraction of sizing logic used by worker and `analyze()`
- `functions.rs`: Groups all 5 SQL functions in `walrus` schema

## Module Responsibilities

### shmem.rs (NEW)
- Define `WalrusState` struct (Copy, Clone, Default)
- Implement `PGRXSharedMemory` for `WalrusState`
- Export `WALRUS_STATE: PgLwLock<WalrusState>` static
- Provide helper functions:
  - `read_state() -> WalrusState` (shared lock)
  - `update_state(f: impl FnOnce(&mut WalrusState))` (exclusive lock)
  - `reset_state()` (zeros all fields)

### algorithm.rs (NEW)
- Extract `calculate_new_size()` from worker.rs (already public)
- Extract `calculate_shrink_size()` from worker.rs (already public)
- NEW: `compute_recommendation(state: &WalrusState, gucs: &GucSnapshot) -> Recommendation`
- NEW: `compute_confidence(state: &WalrusState, checkpoint_count: i64) -> i32`

### functions.rs (NEW)
- `#[pg_schema] mod walrus { ... }` for schema namespace
- `#[pg_extern] fn status() -> JsonB`
- `#[pg_extern] fn history() -> TableIterator<...>`
- `#[pg_extern] fn recommendation() -> JsonB`
- `#[pg_extern] fn analyze(apply: bool) -> JsonB`
- `#[pg_extern] fn reset() -> Result<bool, spi::Error>`

### worker.rs (MODIFY)
- Replace local state variables with shmem reads/writes
- Call `shmem::update_state()` after each iteration
- Use `algorithm::compute_recommendation()` for sizing decisions

### lib.rs (MODIFY)
- Add `pg_shmem_init!(WALRUS_STATE)` in `_PG_init()`
- Add `mod shmem`, `mod algorithm`, `mod functions`
- Move `walrus::cleanup_history()` to functions.rs with other walrus schema functions

## Implementation Sequence

1. **shmem.rs**: Define shared memory structure and initialization
2. **algorithm.rs**: Extract and generalize sizing algorithm
3. **functions.rs**: Implement 5 SQL functions using shmem and algorithm
4. **worker.rs**: Refactor to use shmem state
5. **lib.rs**: Wire up modules and shmem initialization
6. **Tests**: Unit tests, pg_test integration tests, pg_regress SQL tests

## Complexity Tracking

No constitution violations requiring justification. All design decisions align with existing patterns:

| Decision | Constitution Alignment |
|----------|----------------------|
| 3 new modules | XV: Keeps files < 900 LOC |
| `PgLwLock<T>` for shmem | III: Proper memory management |
| `#[pg_extern]` for functions | II: FFI boundary safety |
| Algorithm extraction | DRY principle, testability |

## Post-Design Constitution Re-Check

| Gate | Status | Verification |
|------|--------|--------------|
| VIII. Test Discipline | WILL PASS | Tasks include: pg_test for each function, pg_regress, unit tests |
| XV. File Size Limits | WILL PASS | Three new modules distribute code, lib.rs stays < 900 LOC |

## Dependencies

### Internal
- Existing `stats::get_requested_checkpoints()` for checkpoint data
- Existing `stats::get_current_max_wal_size()` for current size
- Existing `config::execute_alter_system()` for `analyze(apply := true)`
- Existing `history::insert_history_record()` (used by worker after algorithm runs)
- Existing GUC statics from `guc.rs`

### External (Cargo.toml)
- `serde_json`: Already present for JSONB metadata
- `pgrx`: Already present, provides `JsonB`, `TableIterator`, `PgLwLock`, `PgAtomic`
- `libc`: Already present for SIGHUP signaling

No new dependencies required.

## Risk Assessment

| Risk | Mitigation |
|------|------------|
| Shared memory initialization order | `pg_shmem_init!` before `BackgroundWorkerBuilder::load()` |
| Lock contention between worker and SQL functions | Worker holds lock briefly; SQL functions use `.share()` |
| ALTER SYSTEM in transaction context | `analyze(apply := true)` runs outside transaction (matches worker) |
| Test isolation with shared state | Each pg_test runs in separate instance; reset available |
