# Tasks: Dry-Run Mode

**Input**: Design documents from `/specs/005-dry-run-mode/`
**Prerequisites**: plan.md (required), spec.md (required for user stories), research.md, data-model.md

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

## Path Conventions

- **Single project**: `src/`, `tests/` at repository root (pgrx extension)

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: GUC infrastructure - this is the foundation for all user stories

- [X] T001 Define `WALRUS_DRY_RUN` GUC static variable in `src/guc.rs` using `GucSetting::<bool>::new(false)`
- [X] T002 Register `walrus.dry_run` GUC in `register_gucs()` function in `src/guc.rs` with GucContext::Sighup
- [X] T003 Export `WALRUS_DRY_RUN` from `src/guc.rs` for use in worker module

**Checkpoint**: GUC is defined and registered; `SHOW walrus.dry_run` returns 'off'

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core dry-run check infrastructure that MUST be complete before user story implementation

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [X] T004 Import `WALRUS_DRY_RUN` from guc module into `src/worker.rs`

**Checkpoint**: Foundation ready - user story implementation can now begin

---

## Phase 3: User Story 1 - Validate Extension Before Production Enablement (Priority: P1) 🎯 MVP

**Goal**: Enable dry-run mode to log simulated sizing decisions without executing ALTER SYSTEM or SIGHUP

**Independent Test**: Enable `walrus.dry_run = true` alongside `walrus.enable = true`, trigger checkpoint activity exceeding threshold, verify log message appears and no ALTER SYSTEM is executed

### Implementation for User Story 1

- [X] T005 [US1] Add dry-run check in GROW PATH of `process_checkpoint_stats()` in `src/worker.rs` before `execute_alter_system()` call (~line 160)
- [X] T006 [US1] Implement dry-run log message for grow decisions in `src/worker.rs`: `LOG: pg_walrus [DRY-RUN]: would change max_wal_size from X MB to Y MB (threshold exceeded)`
- [X] T007 [US1] Add dry-run check in SHRINK PATH of `process_checkpoint_stats()` in `src/worker.rs` before `execute_alter_system()` call (~line 270)
- [X] T008 [US1] Implement dry-run log message for shrink decisions in `src/worker.rs`: `LOG: pg_walrus [DRY-RUN]: would change max_wal_size from X MB to Y MB (sustained low activity)`
- [X] T009 [US1] Implement dry-run log message for capped decisions in `src/worker.rs`: `LOG: pg_walrus [DRY-RUN]: would change max_wal_size from X MB to Y MB (capped at walrus.max)`
- [X] T010 [US1] Skip `execute_alter_system()` call when dry-run is enabled in both GROW and SHRINK paths in `src/worker.rs`
- [X] T011 [US1] Skip `send_sighup_to_postmaster()` call when dry-run is enabled in both GROW and SHRINK paths in `src/worker.rs`
- [X] T012 [US1] Ensure shared memory state (`quiet_intervals`, `total_adjustments`, `last_adjustment_time`) updates correctly: skip `total_adjustments` and `last_adjustment_time` for dry-run in `src/worker.rs`
- [X] T013 [US1] Add `#[pg_test]` test `test_guc_dry_run_default` verifying `SHOW walrus.dry_run` returns 'off' in `src/tests.rs` tests module
- [X] T014 [US1] Add `#[pg_test]` test `test_guc_dry_run_visible_in_pg_settings` verifying GUC appears in pg_settings catalog in `src/tests.rs` tests module

**Checkpoint**: User Story 1 complete - dry-run mode prevents ALTER SYSTEM and logs simulated decisions

---

## Phase 4: User Story 2 - Tune Algorithm Parameters Safely (Priority: P2)

**Goal**: Allow DBAs to experiment with threshold/shrink_factor changes while in dry-run mode without affecting actual max_wal_size

**Independent Test**: Change `walrus.threshold` to 5, observe that dry-run decisions respect the new threshold

### Implementation for User Story 2

- [X] T015 [US2] Code review: confirm dry-run logic reads threshold/shrink_factor GUC values at decision time (existing behavior verification)
- [X] T016 [US2] Add `#[pg_test]` test `test_dry_run_respects_threshold_changes` in `src/tests.rs` tests module verifying threshold changes are reflected in dry-run decisions

**Checkpoint**: User Story 2 complete - parameter changes are respected in dry-run mode

---

## Phase 5: User Story 3 - Audit Decision History for Compliance (Priority: P3)

**Goal**: Record all dry-run decisions to walrus.history with action='dry_run' and complete metadata

**Independent Test**: Query `walrus.history` after dry-run decisions and verify records contain `action = 'dry_run'` with `would_apply` metadata

### Implementation for User Story 3

- [X] T017 [US3] Modify GROW PATH dry-run branch in `src/worker.rs` to call `insert_history_record()` with `action = "dry_run"` and metadata containing `{"dry_run": true, "would_apply": "increase", ...}`
- [X] T018 [US3] Modify SHRINK PATH dry-run branch in `src/worker.rs` to call `insert_history_record()` with `action = "dry_run"` and metadata containing `{"dry_run": true, "would_apply": "decrease", ...}`
- [X] T019 [US3] Handle capped dry-run decisions: insert history with `metadata->>'would_apply' = 'capped'` in `src/worker.rs`
- [X] T020 [US3] Include all existing algorithm metadata (delta, multiplier, calculated_size_mb, shrink_factor, quiet_intervals) in dry-run history records in `src/worker.rs`
- [X] T021 [US3] Add `#[pg_test]` test `test_dry_run_history_grow` verifying history record with `action = 'dry_run'` and `would_apply = 'increase'` in `src/history.rs` tests module
- [X] T022 [US3] Add `#[pg_test]` test `test_dry_run_history_shrink` verifying history record with `action = 'dry_run'` and `would_apply = 'decrease'` in `src/history.rs` tests module
- [X] T023 [US3] Add `#[pg_test]` test `test_dry_run_history_metadata_complete` verifying all algorithm fields are present in metadata in `src/history.rs` tests module

**Checkpoint**: User Story 3 complete - dry-run decisions are fully auditable via walrus.history

---

## Phase 6: Edge Cases

**Purpose**: Handle all edge cases defined in the specification

- [X] T024 Document edge case: dry-run mode change mid-cycle takes effect on next iteration (add code comment in `src/worker.rs` at dry-run check location)
- [X] T025 Add `#[pg_test]` test `test_dry_run_with_enable_false` verifying no decisions when `walrus.enable = false` even with `walrus.dry_run = true` in `src/tests.rs` tests module
- [X] T026 [P] Add `#[pg_test]` test `test_dry_run_missing_history_table` verifying graceful handling when history table does not exist in `src/history.rs` tests module
- [X] T027 Add `#[pg_test]` test `test_dry_run_capped_decision` verifying capped dry-run logs and history work correctly in `src/tests.rs` tests module
- [X] T035 [P] Add `#[pg_test]` test `test_dry_run_mid_cycle_change` verifying mode change takes effect on next iteration in `src/tests.rs` tests module
- [X] T036 [P] Add `#[pg_test]` test `test_default_dry_run_false_no_regression` verifying ALTER SYSTEM executes normally when dry_run=false in `src/tests.rs` tests module

**Checkpoint**: All edge cases from specification are handled with explicit tests

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: SQL tests, documentation, and final validation

- [X] T028 [P] Create pg_regress test file `tests/pg_regress/sql/dry_run.sql` with GUC visibility tests
- [X] T029 [P] Create pg_regress expected output file `tests/pg_regress/expected/dry_run.out`
- [X] T030 Run `cargo pgrx test pg18` and verify all tests pass (105 tests passing)
- [X] T031 Run `cargo pgrx regress pg18 --postgresql-conf "shared_preload_libraries='pg_walrus'"` and verify all SQL tests pass (10 tests passing)
- [X] T032 Validate quickstart.md scenarios manually in psql session (implementation matches spec)
- [X] T033 Run `cargo clippy -- -D warnings` and fix any lints (0 warnings)
- [X] T034 Run `cargo fmt --check` and fix any formatting issues (formatted)

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion - BLOCKS all user stories
- **User Stories (Phase 3-5)**: All depend on Foundational phase completion
  - US1 → US2 → US3 in priority order (sequential recommended)
  - US2 depends conceptually on US1 (dry-run infrastructure)
  - US3 depends on US1 (dry-run branches must exist for history insertion)
- **Edge Cases (Phase 6)**: Depends on US1, US2, US3 completion
- **Polish (Phase 7)**: Depends on all implementation phases

### User Story Dependencies

- **User Story 1 (P1)**: Can start after Foundational (Phase 2) - Core dry-run functionality
- **User Story 2 (P2)**: Can start after US1 - Tests that GUC changes are respected
- **User Story 3 (P3)**: Can start after US1 - Adds history recording to existing dry-run branches

### Within Each User Story

- Implementation tasks before test tasks (tests verify implementation)
- GROW path before SHRINK path (establish pattern, then replicate)
- Core behavior before edge case handling

### Parallel Opportunities

- T028 and T029 (pg_regress files) can run in parallel
- Within Phase 3 (US1): T005+T006 (grow path) can be done before T007+T008 (shrink path) but both needed before T010+T011

---

## Parallel Example: Phase 7 (Polish)

```bash
# Launch pg_regress file creation together:
Task: "Create pg_regress test file tests/pg_regress/sql/dry_run.sql"
Task: "Create pg_regress expected output file tests/pg_regress/expected/dry_run.out"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (GUC definition)
2. Complete Phase 2: Foundational (import into worker)
3. Complete Phase 3: User Story 1 (core dry-run behavior)
4. **STOP and VALIDATE**: `SET walrus.dry_run = true; SET walrus.enable = true;` and trigger checkpoint activity
5. Verify LOG messages appear with `[DRY-RUN]` prefix
6. Verify `max_wal_size` does NOT change

### Incremental Delivery

1. Complete Setup + Foundational → GUC available
2. Add User Story 1 → Core dry-run works → MVP complete
3. Add User Story 2 → Validate parameter tuning works
4. Add User Story 3 → Audit trail complete
5. Add Edge Cases → All scenarios handled
6. Polish → Tests pass, documentation validated

### Files Modified

| File | Changes |
|------|---------|
| `src/guc.rs` | Add `WALRUS_DRY_RUN` static, register GUC, add tests |
| `src/worker.rs` | Import GUC, add dry-run branches in process_checkpoint_stats(), add tests |
| `src/history.rs` | Add tests for dry_run action type |
| `src/lib.rs` | Update history table CHECK constraint to include 'dry_run' action |
| `src/tests.rs` | Add dry-run GUC tests, edge case tests, parameter tuning tests |
| `tests/pg_regress/sql/dry_run.sql` | New file: SQL tests for GUC |
| `tests/pg_regress/expected/dry_run.out` | New file: Expected output |

---

## Notes

- [P] tasks = different files, no dependencies
- [Story] label maps task to specific user story for traceability
- Each user story should be independently completable and testable
- Commit after each task or logical group
- Stop at any checkpoint to validate story independently
- pgrx tests use existing `pg_test` module with `postgresql_conf_options()` for shared_preload_libraries
