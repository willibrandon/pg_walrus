# Implementation Plan: History Table

**Branch**: `003-history-table` | **Date**: 2025-12-30 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/003-history-table/spec.md`

## Summary

Implement a persistent audit trail for all pg_walrus sizing decisions. The history table stores timestamped records of max_wal_size adjustments (increases, decreases, capped) with checkpoint statistics and algorithm metadata. Includes automatic cleanup based on configurable retention period.

**Technical Approach**: Use pgrx `extension_sql!` macro to create schema and table during extension installation. Add a new `history.rs` module for SPI-based insert operations. Integrate history logging into existing worker loop in `worker.rs`. Add `walrus.cleanup_history()` SQL function and `walrus.history_retention_days` GUC.

## Technical Context

**Language/Version**: Rust 1.83+ (latest stable, edition 2024)
**Primary Dependencies**: pgrx 0.16.1, libc 0.2
**Storage**: PostgreSQL table (`walrus.history`) with BIGSERIAL primary key, TIMESTAMPTZ, JSONB
**Testing**: cargo pgrx test (pg15, pg16, pg17, pg18), cargo pgrx regress
**Target Platform**: PostgreSQL 15, 16, 17, 18 on Linux/macOS
**Project Type**: Single project (pgrx extension)
**Performance Goals**: History insert < 1ms overhead, cleanup via indexed DELETE
**Constraints**: Background worker SPI transaction isolation, schema creation via extension_sql!
**Scale/Scope**: Up to 1M history records, 7-day default retention

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. No Task Deferral | PASS | All features implemented in full |
| II. FFI Boundary Safety | PASS | History insert uses Spi::connect, no raw FFI |
| III. Memory Management | PASS | Standard Rust types, SPI handles Postgres memory |
| IV. Background Worker Patterns | PASS | Existing worker already follows patterns; add history call |
| V. GUC Configuration | PASS | New GUC follows existing pattern in guc.rs |
| VI. SPI & Database Access | PASS | Parameterized queries, transaction scope via SPI |
| VII. Version Compatibility | PASS | No version-specific code needed for history |
| VIII. Test Discipline | PASS | Three-tier testing: #[pg_test], #[test], pg_regress |
| IX. Anti-Patterns | PASS | No threading, proper guards already in worker |
| X. Observability | PASS | History table IS the observability feature |
| XI. Test Failure Protocol | PASS | Tests define spec, fix implementation not tests |
| XII. Git Attribution | PASS | No AI attribution in commits |
| XIII. No Simplification | PASS | Full implementation, no scope reduction |
| XIV. No Regression | PASS | Using latest Rust 2024 edition |
| XV. File Size Limits | PASS | New history.rs module, lib.rs under 900 LOC |
| XVI. No False Impossibility | PASS | Full pgrx/postgres source available |

## Project Structure

### Documentation (this feature)

```text
specs/003-history-table/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output (SQL function signatures)
└── tasks.md             # Phase 2 output (/speckit.tasks command)
```

### Source Code (repository root)

```text
src/
├── lib.rs               # Entry point, _PG_init, extension_sql! for schema/table
├── guc.rs               # Add WALRUS_HISTORY_RETENTION_DAYS
├── worker.rs            # Add history logging calls after resize actions
├── history.rs           # NEW: History insert and cleanup functions
├── config.rs            # Existing ALTER SYSTEM implementation
└── stats.rs             # Existing checkpoint statistics

tests/pg_regress/
├── sql/
│   ├── setup.sql        # Existing
│   ├── history.sql      # NEW: History table SQL tests
│   └── cleanup.sql      # NEW: Cleanup function tests
└── expected/
    ├── history.out      # NEW
    └── cleanup.out      # NEW
```

**Structure Decision**: Single pgrx extension project. New `history.rs` module isolates history functionality. Schema and table created via `extension_sql!` in `lib.rs` for proper installation ordering.

## Complexity Tracking

> No violations. All complexity within constitution limits.

| Item | Justification |
|------|---------------|
| New module (history.rs) | Required for separation of concerns, keeps lib.rs under 900 LOC |
| JSONB metadata column | Spec requirement for algorithm details; standard PostgreSQL type |
| Schema creation | Required for namespace isolation (`walrus.history` vs `history`) |
