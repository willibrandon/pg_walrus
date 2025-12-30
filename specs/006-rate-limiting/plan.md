# Implementation Plan: Rate Limiting

**Branch**: `006-rate-limiting` | **Date**: 2025-12-30 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/006-rate-limiting/spec.md`

## Summary

Add rate limiting to pg_walrus to prevent thrashing on unstable workloads by enforcing a minimum cooldown period between adjustments and limiting maximum adjustments per hour. This involves extending shared memory state with rate limiting fields, adding two new GUC parameters, integrating rate limit checks into the worker's grow and shrink paths, extending `walrus.status()` with rate limiting metrics, and recording skipped adjustments in the history table.

## Technical Context

**Language/Version**: Rust 1.83+ (edition 2024) + pgrx 0.16.1
**Primary Dependencies**: pgrx 0.16.1, serde_json 1.x, libc 0.2
**Storage**: PostgreSQL shared memory (ephemeral), walrus.history table (persistent)
**Testing**: cargo pgrx test pg15/16/17/18, cargo pgrx regress pg15/16/17/18
**Target Platform**: PostgreSQL 15, 16, 17, 18 on Linux/macOS
**Project Type**: Single pgrx extension
**Performance Goals**: Rate limit checks must complete in microseconds (timestamp comparisons)
**Constraints**: No additional memory beyond existing shared memory allocation (just add fields to WalrusState)
**Scale/Scope**: Single background worker, state shared across all backends

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Gate | Requirement | Status | Notes |
|------|-------------|--------|-------|
| I. No Task Deferral | Complete all work, no TODOs | PASS | All requirements will be fully implemented |
| II. FFI Boundary Safety | `#[pg_guard]` on callbacks | PASS | No new extern functions needed |
| III. Memory Management | Track allocation ownership | PASS | Extends existing WalrusState in shmem |
| IV. Background Worker Patterns | Signal handlers, transaction wrapping | PASS | Uses existing worker infrastructure |
| V. GUC Configuration | Register in _PG_init, use GucContext::Sighup | PASS | Two new GUCs following existing patterns |
| VI. SPI & Database Access | Parameterized queries, transaction context | PASS | History insertion uses existing patterns |
| VII. Version Compatibility | Feature gates for PG15-18 | PASS | No version-specific code needed |
| VIII. Test Discipline | Three-tier testing (pg_test, test, pg_regress) | PASS | Tests planned for all tiers |
| XI. Test Failure Protocol | Fix implementations, not tests | PASS | Constitutional mandate |
| XII. Git Attribution | No AI attribution in commits | PASS | Constitutional mandate |
| XIII. No Simplification | Maintain full scope | PASS | All 19 FRs will be implemented |
| XIV. No Regression | Use edition 2024, adapt code | PASS | Using current tooling |
| XV. File Size Limits | <900 LOC per file | PASS | Will extend existing files minimally |
| XVI. No False Impossibility | Research before claiming blocked | PASS | Constitutional mandate |

## Project Structure

### Documentation (this feature)

```text
specs/006-rate-limiting/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output (N/A - no external API)
└── tasks.md             # Phase 2 output (/speckit.tasks)
```

### Source Code (repository root)

```text
src/
├── lib.rs               # Entry point, _PG_init, walrus schema functions (MODIFY: add rate limit fields to status())
├── worker.rs            # Background worker main loop (MODIFY: add rate limit checks)
├── shmem.rs             # Shared memory state (MODIFY: add rate limit fields to WalrusState, reset_state)
├── guc.rs               # GUC definitions (MODIFY: add WALRUS_COOLDOWN_SEC, WALRUS_MAX_CHANGES_PER_HOUR)
├── functions.rs         # SQL function implementations (MODIFY: extend status(), add force_adjust bypass logic)
├── history.rs           # History table operations (MODIFY: support action='skipped')
├── algorithm.rs         # Sizing algorithms (NO CHANGE)
├── config.rs            # ALTER SYSTEM implementation (NO CHANGE)
├── stats.rs             # Checkpoint statistics access (NO CHANGE)
└── tests.rs             # Integration tests (MODIFY: add rate limiting tests)

tests/pg_regress/
├── sql/
│   ├── setup.sql        # Extension creation (NO CHANGE)
│   ├── rate_limiting.sql # NEW: Rate limiting SQL tests
│   └── ...
└── expected/
    ├── rate_limiting.out # NEW: Expected output
    └── ...
```

**Structure Decision**: Single pgrx extension project. Rate limiting logic integrates into existing modules (worker.rs, shmem.rs, guc.rs, functions.rs). No new modules needed; rate limiting is a cross-cutting concern applied to existing adjustment paths.

## Complexity Tracking

> No Constitution violations requiring justification. All requirements are straightforward extensions of existing patterns.

| Aspect | Approach | Rationale |
|--------|----------|-----------|
| State Storage | Extend WalrusState struct | Existing shmem infrastructure handles thread-safe access |
| GUC Registration | Follow existing GUC patterns | Existing patterns in guc.rs are well-tested |
| History Recording | Use existing insert_history_record | Adds new action type 'skipped' using existing infrastructure |
| Rate Limit Check Location | Before dry-run check in worker | FR-014 specifies this order for accurate dry-run behavior |
