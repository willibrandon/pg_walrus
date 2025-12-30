# Tasks: SQL Observability Functions

**Input**: Design documents from `/specs/004-sql-observability-functions/`
**Prerequisites**: plan.md (required), spec.md (required), research.md, data-model.md, contracts/

**Tests**: This feature includes tests as required by Constitution VIII (Test Discipline) - three-tier testing with `#[pg_test]`, `#[test]`, and pg_regress.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2)
- Include exact file paths in descriptions

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Create new modules and establish shared memory infrastructure

- [ ] T001 Create shared memory state struct `WalrusState` with 5 fields (quiet_intervals, total_adjustments, prev_requested, last_check_time, last_adjustment_time) in `src/shmem.rs`
- [ ] T002 Implement `PGRXSharedMemory` trait and `PgLwLock<WalrusState>` static in `src/shmem.rs`
- [ ] T003 Add helper functions `read_state()`, `update_state()`, `reset_state()` in `src/shmem.rs`
- [ ] T004 Add `mod shmem` declaration and `pg_shmem_init!(WALRUS_STATE)` to `_PG_init()` in `src/lib.rs`
- [ ] T005 [P] Create `src/algorithm.rs` module stub with function signatures from plan.md
- [ ] T006 [P] Create `src/functions.rs` module stub with `#[pg_schema] mod walrus` namespace

---

## Phase 2: Foundational (Algorithm Extraction)

**Purpose**: Extract and generalize sizing algorithm for use by both worker and SQL functions

**CRITICAL**: Worker refactoring depends on algorithm module completion

- [ ] T007 Move `calculate_new_size()` from `src/worker.rs` to `src/algorithm.rs`, keep public export
- [ ] T008 Move `calculate_shrink_size()` from `src/worker.rs` to `src/algorithm.rs`, keep public export
- [ ] T009 Implement `Recommendation` struct with fields: current_size_mb, recommended_size_mb, action, reason, confidence in `src/algorithm.rs`
- [ ] T010 Implement `compute_confidence()` function per data-model.md formula in `src/algorithm.rs`
- [ ] T011 Implement `compute_recommendation()` function that computes grow/shrink/none/error action in `src/algorithm.rs`
- [ ] T012 [P] Add pure Rust unit tests for `compute_confidence()` in `src/algorithm.rs`
- [ ] T013 [P] Add pure Rust unit tests for `compute_recommendation()` in `src/algorithm.rs`
- [ ] T014 Update `src/worker.rs` to import `calculate_new_size` and `calculate_shrink_size` from algorithm module
- [ ] T015 Refactor `src/worker.rs` to use `shmem::update_state()` for storing quiet_intervals, prev_requested, timestamps
- [ ] T016 Refactor `src/worker.rs` to increment `total_adjustments` in shmem after each resize
- [ ] T017 Add `mod algorithm` declaration to `src/lib.rs`
- [ ] T018 Verify existing tests pass with refactored worker via `cargo pgrx test pg18`

**Checkpoint**: Foundation ready - shmem working, algorithm extracted, worker refactored

---

## Phase 3: User Story 1 - Monitor Extension Health (Priority: P1) MVP

**Goal**: `walrus.status()` returns JSONB with current extension state

**Independent Test**: Call `SELECT walrus.status()` and verify all 15 fields present with valid values

### Tests for User Story 1

- [ ] T019 [P] [US1] Add `#[pg_test]` for `walrus.status()` returns valid JSONB in `src/lib.rs`
- [ ] T020 [P] [US1] Add `#[pg_test]` for `walrus.status()` contains `enabled` field matching GUC in `src/lib.rs`
- [ ] T021 [P] [US1] Add `#[pg_test]` for `walrus.status()` contains `worker_running` field in `src/lib.rs`
- [ ] T022 [P] [US1] Add `#[pg_test]` for `walrus.status()` contains `at_ceiling` field in `src/lib.rs`
- [ ] T023 [P] [US1] Add pg_regress test for `walrus.status()` in `tests/pg_regress/sql/observability.sql`

### Implementation for User Story 1

- [ ] T024 [US1] Implement `check_worker_running()` helper that queries `pg_stat_activity` in `src/functions.rs`
- [ ] T025 [US1] Implement `unix_timestamp_to_iso()` helper for timestamp formatting in `src/functions.rs`
- [ ] T026 [US1] Implement `walrus.status()` function returning JsonB with all 15 fields per contract in `src/functions.rs`
- [ ] T027 [US1] Handle edge case: null timestamps when worker hasn't completed first cycle in `src/functions.rs`
- [ ] T028 [US1] Handle edge case: `at_ceiling` when `current_max_wal_size_mb >= configured_maximum_mb` in `src/functions.rs`
- [ ] T029 [US1] Add `mod functions` declaration to `src/lib.rs`
- [ ] T030 [US1] Verify `walrus.status()` execution time < 100ms via `#[pg_test]` with timing in `src/lib.rs`

**Checkpoint**: `SELECT walrus.status()` returns complete JSONB with all configuration and worker state

---

## Phase 4: User Story 2 - View Adjustment History (Priority: P1)

**Goal**: `walrus.history()` returns SETOF RECORD from walrus.history table

**Independent Test**: Call `SELECT * FROM walrus.history()` and verify column structure matches contract

### Tests for User Story 2

- [ ] T031 [P] [US2] Add `#[pg_test]` for `walrus.history()` returns empty set when no adjustments in `src/lib.rs`
- [ ] T032 [P] [US2] Add `#[pg_test]` for `walrus.history()` returns rows after insert_history_record in `src/lib.rs`
- [ ] T033 [P] [US2] Add `#[pg_test]` for `walrus.history()` column types match contract in `src/lib.rs`
- [ ] T034 [P] [US2] Add pg_regress test for `walrus.history()` in `tests/pg_regress/sql/observability.sql`

### Implementation for User Story 2

- [ ] T035 [US2] Implement `walrus.history()` function using `TableIterator` with `name!()` macro in `src/functions.rs`
- [ ] T036 [US2] Use SPI to SELECT from walrus.history table with proper column mapping in `src/functions.rs`
- [ ] T037 [US2] Handle edge case: return SQL error if history table was dropped in `src/functions.rs`
- [ ] T038 [US2] Move existing `walrus.cleanup_history()` from `src/lib.rs` to `src/functions.rs`

**Checkpoint**: `SELECT * FROM walrus.history()` returns complete history with correct column types

---

## Phase 5: User Story 3 - Preview Recommendations (Priority: P2)

**Goal**: `walrus.recommendation()` returns JSONB with computed recommendation without applying

**Independent Test**: Call `SELECT walrus.recommendation()` and verify action/confidence fields

### Tests for User Story 3

- [ ] T039 [P] [US3] Add `#[pg_test]` for `walrus.recommendation()` returns valid JSONB in `src/lib.rs`
- [ ] T040 [P] [US3] Add `#[pg_test]` for `walrus.recommendation()` contains `confidence` 0-100 in `src/lib.rs`
- [ ] T041 [P] [US3] Add `#[pg_test]` for `walrus.recommendation()` action is one of: increase/decrease/none/error in `src/lib.rs`
- [ ] T042 [P] [US3] Add pg_regress test for `walrus.recommendation()` in `tests/pg_regress/sql/observability.sql`

### Implementation for User Story 3

- [ ] T043 [US3] Implement `walrus.recommendation()` using `algorithm::compute_recommendation()` in `src/functions.rs`
- [ ] T044 [US3] Read shmem state (prev_requested, quiet_intervals) for recommendation calculation in `src/functions.rs`
- [ ] T045 [US3] Fetch current checkpoint stats via `stats::get_requested_checkpoints()` in `src/functions.rs`
- [ ] T046 [US3] Handle edge case: return `action: "error"` when checkpoint stats unavailable in `src/functions.rs`

**Checkpoint**: `SELECT walrus.recommendation()` returns actionable recommendation without modifying state

---

## Phase 6: User Story 4 - Trigger Immediate Analysis (Priority: P2)

**Goal**: `walrus.analyze(apply)` triggers analysis with optional application of changes

**Independent Test**: Call `SELECT walrus.analyze()` and verify analyzed/recommendation/applied fields

### Tests for User Story 4

- [ ] T047 [P] [US4] Add `#[pg_test]` for `walrus.analyze()` returns `analyzed: true` and completes within 5 seconds (SC-005) in `src/lib.rs`
- [ ] T048 [P] [US4] Add `#[pg_test]` for `walrus.analyze()` returns `applied: false` by default in `src/lib.rs`
- [ ] T049 [P] [US4] Add `#[pg_test]` for `walrus.analyze(apply := true)` requires superuser (expect error) in `src/lib.rs`
- [ ] T050 [P] [US4] Add pg_regress test for `walrus.analyze()` in `tests/pg_regress/sql/observability.sql`

### Implementation for User Story 4

- [ ] T051 [US4] Implement `walrus.analyze(apply boolean DEFAULT false)` with `default!()` macro in `src/functions.rs`
- [ ] T052 [US4] Add superuser check for `apply = true` using `pg_sys::superuser()` in `src/functions.rs`
- [ ] T053 [US4] Use `algorithm::compute_recommendation()` for analysis in `src/functions.rs`
- [ ] T054 [US4] Execute `config::execute_alter_system()` when `apply = true` and action != "none" in `src/functions.rs`
- [ ] T055 [US4] Handle edge case: return `analyzed: false, reason: "extension is disabled"` when `walrus.enable = false` in `src/functions.rs`
- [ ] T056 [US4] Handle edge case: `applied` only true when `apply` param is true AND change executed in `src/functions.rs`

**Checkpoint**: `SELECT walrus.analyze()` performs analysis; `analyze(apply := true)` executes changes

---

## Phase 7: User Story 5 - Reset Extension State (Priority: P3)

**Goal**: `walrus.reset()` clears history and resets shmem counters

**Independent Test**: Call `SELECT walrus.reset()` then verify history empty and counters zero via status()

### Tests for User Story 5

- [ ] T057 [P] [US5] Add `#[pg_test]` for `walrus.reset()` requires superuser (expect error as non-super) in `src/lib.rs`
- [ ] T058 [P] [US5] Add `#[pg_test]` for `walrus.reset()` returns true on success in `src/lib.rs`
- [ ] T059 [P] [US5] Add `#[pg_test]` for `walrus.reset()` clears shmem counters (verify via status) in `src/lib.rs`
- [ ] T060 [P] [US5] Add `#[pg_test]` for `walrus.reset()` clears history table in `src/lib.rs`
- [ ] T061 [P] [US5] Add pg_regress test for `walrus.reset()` in `tests/pg_regress/sql/observability.sql`

### Implementation for User Story 5

- [ ] T062 [US5] Implement `walrus.reset()` with superuser check in `src/functions.rs`
- [ ] T063 [US5] Clear shmem state via `shmem::reset_state()` in `src/functions.rs`
- [ ] T064 [US5] Delete all rows from walrus.history via SPI in `src/functions.rs`
- [ ] T065 [US5] Handle edge case: return true with WARNING if history table dropped (shmem still reset) in `src/functions.rs`

**Checkpoint**: `SELECT walrus.reset()` clears all state; worker sees zeros on next cycle

---

## Phase 8: Polish & Cross-Cutting Concerns

**Purpose**: Final integration, documentation, and multi-version testing

- [ ] T066 Create expected output file `tests/pg_regress/expected/observability.out`
- [ ] T067 Run `cargo pgrx regress pg18 --postgresql-conf "shared_preload_libraries='pg_walrus'"` and verify pass
- [ ] T068 Run full test suite on pg15: `cargo pgrx test pg15`
- [ ] T069 Run full test suite on pg16: `cargo pgrx test pg16`
- [ ] T070 Run full test suite on pg17: `cargo pgrx test pg17`
- [ ] T071 Run full test suite on pg18: `cargo pgrx test pg18`
- [ ] T072 Verify all files < 900 LOC via `wc -l src/*.rs`
- [ ] T073 Run `cargo clippy -- -D warnings` and fix any warnings
- [ ] T074 Run `cargo fmt --check` and fix any formatting issues
- [ ] T075 Validate quickstart.md examples work via manual testing

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: No dependencies - can start immediately
- **Phase 2 (Foundational)**: Depends on Phase 1 completion - BLOCKS all user stories
- **Phase 3-7 (User Stories)**: All depend on Phase 2 completion
- **Phase 8 (Polish)**: Depends on all user stories being complete

### User Story Dependencies

- **User Story 1 (P1)**: Can start after Phase 2 - No dependencies on other stories
- **User Story 2 (P1)**: Can start after Phase 2 - No dependencies on other stories
- **User Story 3 (P2)**: Can start after Phase 2 - Uses algorithm from Phase 2
- **User Story 4 (P2)**: Can start after Phase 2 - Uses algorithm from Phase 2
- **User Story 5 (P3)**: Can start after Phase 2 - Uses shmem from Phase 1

### Within Each User Story

- Tests FIRST (write tests, ensure they FAIL before implementation)
- Implementation tasks in order listed
- Story complete before moving to next priority

### Parallel Opportunities

**Phase 1**: T005 and T006 can run in parallel (different files)

**Phase 2**: T007-T08 sequential, T012-T013 in parallel (pure tests)

**User Stories**: Once Phase 2 complete, US1+US2 can run in parallel (both P1)

**Within US1**: T019-T023 tests all parallel, then T024-T030 implementation

**Within US2**: T031-T034 tests all parallel, then T035-T038 implementation

**Phase 8**: T068-T071 multi-version tests can run in parallel

---

## Parallel Example: User Story 1 + 2 (Both P1)

```text
After Phase 2 completion, launch in parallel:

# Team Member A - User Story 1
T019: #[pg_test] walrus.status() returns valid JSONB
T020: #[pg_test] walrus.status() enabled field
T021: #[pg_test] walrus.status() worker_running field
T022: #[pg_test] walrus.status() at_ceiling field
T023: pg_regress test for status()
# Then implement T024-T030

# Team Member B - User Story 2
T031: #[pg_test] walrus.history() returns empty set
T032: #[pg_test] walrus.history() returns rows after insert
T033: #[pg_test] walrus.history() column types
T034: pg_regress test for history()
# Then implement T035-T038
```

---

## Implementation Strategy

### MVP First (User Stories 1 + 2 Only)

1. Complete Phase 1: Setup (T001-T006)
2. Complete Phase 2: Foundational (T007-T018)
3. Complete Phase 3: User Story 1 - status() (T019-T030)
4. Complete Phase 4: User Story 2 - history() (T031-T038)
5. **STOP and VALIDATE**: Both P1 stories independently testable
6. Run Phase 8 multi-version tests (T068-T071)

### Full Delivery

1. MVP (above)
2. Add Phase 5: User Story 3 - recommendation() (T039-T046)
3. Add Phase 6: User Story 4 - analyze() (T047-T056)
4. Add Phase 7: User Story 5 - reset() (T057-T065)
5. Complete Phase 8: Polish (T066-T075)

### Incremental Value

Each user story delivers value independently:
- **US1 + US2**: Monitoring and audit trail (core observability)
- **US3**: Preview recommendations (trust building)
- **US4**: Manual intervention capability (incident response)
- **US5**: Administrative reset (troubleshooting)

---

## Notes

- [P] tasks = different files, no dependencies
- [USn] label maps task to specific user story for traceability
- Edge cases from spec.md have explicit tasks (T027, T028, T037, T046, T055, T056, T065)
- pg_test module already exists in src/lib.rs with `postgresql_conf_options()`
- All functions use `#[pg_extern]` in `#[pg_schema] mod walrus` per FR-014, FR-015
- Superuser checks use `pg_sys::superuser()` per research.md
- SC-005 timing requirement (analyze within checkpoint cycle) verified in T047 with 5-second bound
