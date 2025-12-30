# Implementation Plan: Dry-Run Mode

**Branch**: `005-dry-run-mode` | **Date**: 2025-12-30 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/005-dry-run-mode/spec.md`

## Summary

Implement a dry-run mode for pg_walrus that logs what configuration changes WOULD be made without actually executing `ALTER SYSTEM` or sending SIGHUP. This allows DBAs to safely validate extension behavior before enabling automatic sizing in production.

**Primary Requirements**:
- New `walrus.dry_run` boolean GUC (default: false, SIGHUP context)
- Log messages with `[DRY-RUN]` prefix for simulated decisions
- History records with `action = 'dry_run'` and `would_apply` metadata
- No `ALTER SYSTEM` or SIGHUP when dry-run is enabled

**Technical Approach**:
- Add GUC to `src/guc.rs` following existing patterns
- Modify `src/worker.rs` `process_checkpoint_stats()` to check dry-run flag and branch behavior
- Use existing `insert_history_record()` with new action type and metadata fields

## Technical Context

**Language/Version**: Rust 1.83+ (edition 2024) + pgrx 0.16.1
**Primary Dependencies**: pgrx 0.16.1, serde_json 1.x, libc 0.2
**Storage**: PostgreSQL `walrus.history` table (existing from feature 004)
**Testing**: `cargo pgrx test pgXX`, `cargo pgrx regress pgXX`, `cargo test --lib`
**Target Platform**: PostgreSQL 15, 16, 17, 18 on Linux/macOS
**Project Type**: Single project (pgrx PostgreSQL extension)
**Performance Goals**: No additional overhead beyond existing logging; dry-run path skips ALTER SYSTEM (faster)
**Constraints**: Shared memory state must update identically in both modes for seamless transitions
**Scale/Scope**: Single GUC, ~50 lines of logic changes in worker.rs

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. No Task Deferral | ✅ PASS | All requirements will be fully implemented |
| II. FFI Boundary Safety | ✅ PASS | No new FFI boundaries; uses existing patterns |
| III. Memory Management | ✅ PASS | No new allocations; reuses existing history insertion |
| IV. Background Worker Patterns | ✅ PASS | Modifies existing worker loop; maintains signal handling |
| V. GUC Configuration | ✅ PASS | New boolean GUC follows existing `WALRUS_ENABLE` pattern |
| VI. SPI & Database Access | ✅ PASS | Reuses existing `insert_history_record()` |
| VII. Version Compatibility | ✅ PASS | No version-specific code needed |
| VIII. Test Discipline | ✅ PASS | `#[pg_test]` for GUC/history, pg_regress for SQL |
| IX. Anti-Patterns | ✅ PASS | No threading, no missing guards |
| X. Observability | ✅ PASS | Explicit log messages with `[DRY-RUN]` prefix |
| XI. Test Failure Protocol | ✅ PASS | Tests define spec; fix implementation if tests fail |
| XII. Git Attribution | ✅ PASS | No AI attribution in commits |
| XIII. No Simplification | ✅ PASS | Full implementation required |
| XIV. No Regression | ✅ PASS | Using edition 2024 patterns |
| XV. File Size Limits | ✅ PASS | worker.rs is ~500 LOC; changes add ~50 LOC |
| XVI. No False Impossibility | ✅ PASS | Full source access for debugging |

**Gate Status**: ✅ PASS - Proceed to Phase 0

## Project Structure

### Documentation (this feature)

```text
specs/005-dry-run-mode/
├── spec.md              # Feature specification
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
└── tasks.md             # Phase 2 output (/speckit.tasks command)
```

### Source Code (repository root)

```text
src/
├── lib.rs              # Entry point, _PG_init, GUC registration
├── guc.rs              # GUC definitions (add WALRUS_DRY_RUN)
├── worker.rs           # Background worker (add dry-run branch in process_checkpoint_stats)
├── history.rs          # History insertion (no changes - reuse existing)
├── config.rs           # ALTER SYSTEM execution (no changes - skip when dry-run)
├── algorithm.rs        # Sizing algorithms (no changes)
├── stats.rs            # Checkpoint statistics (no changes)
├── shmem.rs            # Shared memory (no changes)
└── functions.rs        # SQL functions (no changes)

tests/
└── pg_regress/
    ├── sql/
    │   └── dry_run.sql        # New: dry-run SQL tests
    └── expected/
        └── dry_run.out        # New: expected output
```

**Structure Decision**: Single pgrx extension project. Dry-run mode adds one GUC and modifies the decision execution path in `worker.rs`. No new modules required.

## Complexity Tracking

No constitution violations requiring justification. The feature is a straightforward addition:
- 1 new GUC following existing patterns
- Conditional branching in existing decision logic
- New action type for existing history table
