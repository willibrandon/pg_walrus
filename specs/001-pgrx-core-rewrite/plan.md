# Implementation Plan: pg_walrus Core Extension (pgrx Rewrite)

**Branch**: `001-pgrx-core-rewrite` | **Date**: 2025-12-29 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/001-pgrx-core-rewrite/spec.md`

## Summary

Rewrite the pg_walsizer PostgreSQL extension in Rust using pgrx. The extension monitors checkpoint activity via a background worker, automatically increasing `max_wal_size` when forced checkpoints exceed a configurable threshold. This eliminates performance-degrading forced checkpoints without manual intervention.

## Technical Context

**Language/Version**: Rust 1.83+ (latest stable, edition 2024)
**Primary Dependencies**: pgrx 0.16.1, libc (FFI compatibility)
**Storage**: N/A (extension modifies postgresql.auto.conf via ALTER SYSTEM)
**Testing**: Three complementary approaches (see contracts/testing.md):
- `#[pg_test]` integration tests: `cargo pgrx test pgXX`
- `#[test]` pure Rust unit tests: `cargo test --lib`
- pg_regress SQL tests: `cargo pgrx regress pgXX`
**Target Platform**: Linux, macOS, Windows (PostgreSQL extension, shared_preload_libraries)
**Project Type**: Single PostgreSQL extension
**Performance Goals**: Background worker wake cycle matching checkpoint_timeout (~5 minutes default), sub-second configuration changes
**Constraints**: Memory overhead <1MB, no blocking of PostgreSQL operations, must handle SIGHUP/SIGTERM signals
**Scale/Scope**: Single background worker per PostgreSQL instance
**Technical Notes**:
- `CheckPointTimeout` accessed via extern C declaration (pgrx does not expose - see research.md R8)
- pgrx-tests supports `shared_preload_libraries` via `postgresql_conf_options()` (see research.md R9)

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### Pre-Design Check (Phase 0 Gate)

| Principle | Status | Notes |
|-----------|--------|-------|
| I. No Task Deferral | PASS | All functionality will be implemented completely |
| II. FFI Boundary Safety | PASS | All extern "C-unwind" functions use #[pg_guard] |
| III. Memory Management | PASS | Using Rust allocation for extension state |
| IV. Background Worker Patterns | PASS | BackgroundWorkerBuilder, signal handlers, wait_latch() |
| V. GUC Configuration | PASS | Three GUCs with GucContext::Sighup for runtime changes |
| VI. SPI & Database Access | N/A | Not using SPI; using raw AlterSystemSetConfigFile() |
| VII. Version Compatibility | PASS | #[cfg(feature = "pgXX")] for checkpoint stats API differences |
| VIII. Test Discipline | PASS | #[pg_test] for PostgreSQL tests, #[test] for pure Rust |
| IX. Anti-Patterns | PASS | No threading, proper #[pg_guard] on all callbacks |
| X. Observability | PASS | Logging all resize decisions with before/after values |

### Post-Design Check (Phase 1 Complete)

| Principle | Status | Verification |
|-----------|--------|--------------|
| I. No Task Deferral | PASS | All edge cases from spec.md addressed in data-model.md |
| II. FFI Boundary Safety | PASS | research.md confirms #[pg_guard] on _PG_init, worker_main; R8 documents extern C for CheckPointTimeout |
| III. Memory Management | PASS | Node allocation via pg_sys::makeNode(), list_free() for cleanup |
| IV. Background Worker Patterns | PASS | contracts/background-worker.md defines complete lifecycle |
| V. GUC Configuration | PASS | contracts/guc-interface.md defines all three parameters; R8 documents checkpoint_timeout access |
| VI. SPI & Database Access | N/A | Using raw pg_sys transaction commands, not SPI |
| VII. Version Compatibility | PASS | research.md R2 confirms #[cfg] for num_requested vs requested_checkpoints |
| VIII. Test Discipline | PASS | research.md R9 documents pgrx-tests shared_preload_libraries support; background-worker.md includes test patterns |
| IX. Anti-Patterns | PASS | research.md R5 uses AtomicBool for signal handling |
| X. Observability | PASS | contracts/background-worker.md defines logging contract |

## Project Structure

### Documentation (this feature)

```text
specs/001-pgrx-core-rewrite/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output
│   ├── background-worker.md  # Worker lifecycle contract
│   ├── guc-interface.md      # GUC parameter contract
│   └── testing.md            # Testing guidelines and patterns
└── tasks.md             # Phase 2 output (/speckit.tasks)
```

### Source Code (repository root)

```text
src/
├── lib.rs               # Entry point, _PG_init, pg_module_magic, GUC registration
├── worker.rs            # Background worker main loop, signal handling
├── stats.rs             # Checkpoint statistics access (version-specific)
├── config.rs            # ALTER SYSTEM execution, max_wal_size modification
└── guc.rs               # GUC parameter definitions (walrus.enable, walrus.max, walrus.threshold)

tests/
├── pg_regress/              # SQL-based regression tests (cargo pgrx regress)
│   ├── sql/                 # Test SQL scripts
│   │   ├── setup.sql        # Creates extension (runs first)
│   │   ├── guc_params.sql   # GUC parameter behavior tests
│   │   └── extension_info.sql # Extension metadata tests
│   ├── expected/            # Expected output files
│   │   ├── setup.out
│   │   ├── guc_params.out
│   │   └── extension_info.out
│   └── results/             # Generated during tests (gitignored)
```

**Structure Decision**: Single project structure matching standard pgrx extension layout. The `src/` directory contains the Rust source code organized by concern (worker, stats, config, guc). Tests use three complementary approaches:
- `#[pg_test]` in `src/lib.rs` for PostgreSQL integration tests (GUCs, worker visibility)
- `#[test]` in `src/worker.rs` for pure Rust unit tests (calculations, overflow)
- `pg_regress` in `tests/pg_regress/` for SQL-based verification (GUC syntax, extension loading)

## Complexity Tracking

No violations to justify. The implementation follows all constitution principles without exception.
