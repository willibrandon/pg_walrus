# Implementation Plan: Auto-Shrink

**Branch**: `002-auto-shrink` | **Date**: 2025-12-30 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/002-auto-shrink/spec.md`

## Summary

Extend pg_walrus to automatically decrease `max_wal_size` when workload decreases, preventing permanent storage growth. The feature tracks consecutive "quiet intervals" (checkpoint intervals with forced checkpoints below threshold) and shrinks `max_wal_size` by a configurable factor after sufficient quiet time, while respecting a minimum floor.

**Technical Approach**: Add four new GUC parameters (`walrus.shrink_enable`, `walrus.shrink_factor`, `walrus.shrink_intervals`, `walrus.min_size`), extend the background worker state to track quiet intervals, and add shrink logic that runs after grow evaluation. Reuses existing ALTER SYSTEM + SIGHUP mechanism.

## Technical Context

**Language/Version**: Rust 1.83+ (latest stable, edition 2024)
**Primary Dependencies**: pgrx 0.16.1, libc 0.2
**Storage**: N/A (modifies postgresql.auto.conf via ALTER SYSTEM)
**Testing**: cargo pgrx test (pg15, pg16, pg17, pg18), cargo pgrx regress, cargo test --lib
**Target Platform**: PostgreSQL 15, 16, 17, 18 on Linux/macOS
**Project Type**: PostgreSQL extension (single crate)
**Performance Goals**: Shrink evaluation completes within same cycle time as grow (<1ms)
**Constraints**: Background worker cannot block PostgreSQL operations; config reload <1s
**Scale/Scope**: Single background worker per database cluster

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. No Task Deferral | ✅ Pass | All edge cases in spec have requirements coverage |
| II. FFI Boundary Safety | ✅ Pass | Using existing `#[pg_guard]` patterns |
| III. Memory Management | ✅ Pass | Quiet interval counter is stack-allocated i32 |
| IV. Background Worker Patterns | ✅ Pass | Extends existing worker, maintains signal handling |
| V. GUC Configuration | ✅ Pass | Four new GUCs with SIGHUP context |
| VI. SPI & Database Access | ✅ Pass | Reuses existing ALTER SYSTEM mechanism |
| VII. Version Compatibility | ✅ Pass | No version-specific changes needed |
| VIII. Test Discipline | ✅ Pass | Three-tier testing required for all new functionality |
| IX. Anti-Patterns | ✅ Pass | No threading, proper guards in place |
| X. Observability | ✅ Pass | Logging shrink events same as grow |
| XI. Test Failure Protocol | ✅ Pass | Tests define specification |
| XII. Git Attribution | ✅ Pass | No AI attribution in commits |
| XIII. No Simplification | ✅ Pass | Full implementation required |
| XIV. No Regression | ✅ Pass | Rust 2024 edition, turbofish syntax |

**Gate Result**: ✅ PASS - No constitution violations

## Project Structure

### Documentation (this feature)

```text
specs/002-auto-shrink/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
└── tasks.md             # Phase 2 output (/speckit.tasks command)
```

### Source Code (repository root)

```text
src/
├── lib.rs               # Extension entry point, _PG_init, tests
├── worker.rs            # Background worker (MODIFY: add shrink logic)
├── guc.rs               # GUC definitions (MODIFY: add 4 shrink GUCs)
├── stats.rs             # Checkpoint statistics access
├── config.rs            # ALTER SYSTEM implementation (reuse)
└── bin/
    └── pgrx_embed.rs    # SQL generation binary

tests/
└── pg_regress/
    ├── sql/
    │   ├── setup.sql            # Extension creation
    │   ├── guc_params.sql       # Existing GUC tests
    │   └── shrink_gucs.sql      # NEW: Shrink GUC parameter tests
    └── expected/
        ├── setup.out
        ├── guc_params.out
        └── shrink_gucs.out      # NEW: Expected output
```

**Structure Decision**: Single pgrx crate structure maintained. New shrink functionality integrates into existing modules (guc.rs, worker.rs) rather than creating new modules, as the logic is tightly coupled with existing grow functionality.

## Complexity Tracking

> No constitution violations requiring justification.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| (none) | N/A | N/A |
