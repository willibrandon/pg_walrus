# Tasks: Auto-Shrink

**Input**: Design documents from `/specs/002-auto-shrink/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, quickstart.md

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2)
- All file paths are relative to repository root

## Path Conventions

```text
src/
├── lib.rs           # Extension entry point, _PG_init, tests
├── worker.rs        # Background worker (MODIFY: add shrink logic)
├── guc.rs           # GUC definitions (MODIFY: add 4 shrink GUCs)
├── stats.rs         # Checkpoint statistics access
└── config.rs        # ALTER SYSTEM implementation (reuse)

tests/pg_regress/
├── sql/
│   └── shrink_gucs.sql    # NEW: Shrink GUC parameter tests
└── expected/
    └── shrink_gucs.out    # NEW: Expected output
```

---

## Phase 1: Setup

**Purpose**: No new project initialization needed - extending existing extension

- [X] T001 Read existing src/guc.rs to understand current GUC registration patterns
- [X] T002 Read existing src/worker.rs to understand current worker state and flow

**Checkpoint**: Codebase context established

---

## Phase 2: Foundational (GUC Infrastructure)

**Purpose**: Register all four new shrink-related GUC parameters - BLOCKS user story implementation

**CRITICAL**: All GUC parameters must be registered before any shrink logic can function

- [X] T003 [P] Add WALRUS_SHRINK_ENABLE static GucSetting<bool> with default true in src/guc.rs
- [X] T004 [P] Add WALRUS_SHRINK_FACTOR static GucSetting<f64> with default 0.75 in src/guc.rs
- [X] T005 [P] Add WALRUS_SHRINK_INTERVALS static GucSetting<i32> with default 5 in src/guc.rs
- [X] T006 [P] Add WALRUS_MIN_SIZE static GucSetting<i32> with default 1024 in src/guc.rs
- [X] T007 Add define_bool_guc call for walrus.shrink_enable with GucContext::Sighup in src/guc.rs register_gucs()
- [X] T008 Add define_float_guc call for walrus.shrink_factor with min=0.01, max=0.99 in src/guc.rs register_gucs()
- [X] T009 Add define_int_guc call for walrus.shrink_intervals with min=1, max=1000 in src/guc.rs register_gucs()
- [X] T010 Add define_int_guc call for walrus.min_size with min=2, max=i32::MAX, GucFlags::UNIT_MB in src/guc.rs register_gucs()
- [X] T011 Export shrink GUC statics from src/guc.rs for use in src/worker.rs
- [X] T012 Run cargo pgrx test pg18 to verify GUC registration compiles and loads

**Checkpoint**: All 4 shrink GUCs registered - shrink logic implementation can begin

---

## Phase 3: User Story 1 - Automatic Storage Reclamation (Priority: P1)

**Goal**: Track quiet intervals and shrink max_wal_size after sustained low activity

**Independent Test**: Verify quiet_intervals counter increments when delta < threshold, and shrink triggers when counter reaches shrink_intervals

### Implementation for User Story 1

- [X] T013 [US1] Add quiet_intervals: i32 field to worker state in walrus_worker_main() in src/worker.rs
- [X] T014 [US1] Add calculate_shrink_size(current_size: i32, shrink_factor: f64, min_size: i32) -> i32 function in src/worker.rs
- [X] T015 [US1] Add pure Rust #[test] tests for calculate_shrink_size with normal values in src/worker.rs
- [X] T016 [US1] Add pure Rust #[test] test for calculate_shrink_size rounding up via f64::ceil() in src/worker.rs
- [X] T017 [US1] Add pure Rust #[test] test for calculate_shrink_size clamping to min_size in src/worker.rs
- [X] T018 [US1] Import shrink GUC statics in src/worker.rs
- [X] T019 [US1] Modify process_checkpoint_stats signature to accept &mut quiet_intervals parameter in src/worker.rs
- [X] T020 [US1] Add increment quiet_intervals when delta < threshold in SHRINK PATH of process_checkpoint_stats in src/worker.rs
- [X] T021 [US1] Add shrink condition check: shrink_enable AND quiet_intervals >= shrink_intervals AND current_size > min_size in src/worker.rs
- [X] T022 [US1] Add shrink execution: call calculate_shrink_size, execute_alter_system, send_sighup_to_postmaster in src/worker.rs
- [X] T023 [US1] Add LOG message for shrink: "pg_walrus: shrinking max_wal_size from X MB to Y MB" in src/worker.rs
- [X] T024 [US1] Reset quiet_intervals to 0 after shrink executes in src/worker.rs
- [X] T025 [US1] Add #[pg_test] test for walrus.shrink_enable GUC default value in src/lib.rs
- [X] T026 [US1] Add #[pg_test] test for walrus.shrink_factor GUC default value in src/lib.rs
- [X] T027 [US1] Add #[pg_test] test for walrus.shrink_intervals GUC default value in src/lib.rs
- [X] T028 [US1] Add #[pg_test] test for walrus.min_size GUC default value in src/lib.rs
- [X] T029 [US1] Add #[pg_test] test verifying all 7 walrus GUCs have context = 'sighup' in src/lib.rs
- [X] T030 [US1] Add #[pg_test] test for walrus.shrink_factor vartype = 'real' in pg_settings in src/lib.rs
- [X] T031 [US1] Add #[pg_test] test for walrus.min_size unit = 'MB' in pg_settings in src/lib.rs
- [X] T032 [US1] Run cargo pgrx test pg18 to verify US1 implementation

**Checkpoint**: Shrink triggers after quiet intervals - core functionality works

---

## Phase 4: User Story 2 - Shrink Respects Minimum Floor (Priority: P1)

**Goal**: Ensure max_wal_size never drops below walrus.min_size

**Independent Test**: Set min_size=2GB, max_wal_size=2.5GB, verify shrink clamps at 2GB

### Implementation for User Story 2

- [X] T033 [US2] Add pure Rust #[test] test: calculate_shrink_size(2560, 0.75, 2048) returns 2048 (clamped) in src/worker.rs
- [X] T034 [US2] Add pure Rust #[test] test: calculate_shrink_size(1024, 0.75, 1024) returns 1024 (at floor) in src/worker.rs
- [X] T035 [US2] Add pure Rust #[test] test: calculate_shrink_size(900, 0.75, 1024) returns 1024 (below floor) in src/worker.rs
- [X] T036 [US2] Add skip condition in shrink path: if current_size <= min_size, do not shrink in src/worker.rs
- [X] T037 [US2] Add skip condition in shrink path: if new_size >= current_size, do not shrink in src/worker.rs
- [X] T038 [US2] Add #[pg_test] test verifying calculate_shrink_size correctly clamps in src/lib.rs
- [X] T039 [US2] Run cargo pgrx test pg18 to verify US2 implementation

**Checkpoint**: min_size floor is enforced - safety feature works

---

## Phase 5: User Story 3 - Quiet Interval Counter Resets on Activity (Priority: P1)

**Goal**: Reset quiet_intervals when forced checkpoints >= threshold (activity detected)

**Independent Test**: Accumulate quiet intervals, trigger grow path, verify counter resets

### Implementation for User Story 3

- [X] T040 [US3] Reset quiet_intervals to 0 in GROW PATH of process_checkpoint_stats before grow execution in src/worker.rs
- [X] T041 [US3] Add #[pg_test] test verifying counter reset logic via threshold check in src/lib.rs
- [X] T042 [US3] Run cargo pgrx test pg18 to verify US3 implementation

**Checkpoint**: Counter resets on activity - correctness ensured

---

## Phase 6: User Story 4 - Disable Shrinking While Keeping Grow (Priority: P2)

**Goal**: Allow shrink_enable=false to disable shrinking while grow continues

**Independent Test**: Set shrink_enable=false, verify shrink never occurs

### Implementation for User Story 4

- [X] T043 [US4] Verify shrink_enable check is first condition in shrink evaluation in src/worker.rs
- [X] T044 [US4] Add #[pg_test(error = ...)] test for SET walrus.shrink_enable = false (SIGHUP context error) in src/lib.rs
- [X] T045 [US4] Run cargo pgrx test pg18 to verify US4 implementation

**Checkpoint**: Independent shrink control - operational flexibility works

---

## Phase 7: User Story 5 - Configure Shrink Aggressiveness (Priority: P2)

**Goal**: Verify shrink_factor and shrink_intervals are tunable

**Independent Test**: Set non-default values, verify shrink behavior matches configuration

### Implementation for User Story 5

- [X] T046 [US5] Add pure Rust #[test] test: calculate_shrink_size(4096, 0.5, 1024) returns 2048 in src/worker.rs
- [X] T047 [US5] Add #[pg_test] test accessing WALRUS_SHRINK_FACTOR.get() static in src/lib.rs
- [X] T048 [US5] Add #[pg_test] test accessing WALRUS_SHRINK_INTERVALS.get() static in src/lib.rs
- [X] T049 [US5] Run cargo pgrx test pg18 to verify US5 implementation

**Checkpoint**: Tunable shrink parameters - configuration works

---

## Phase 8: User Story 6 - Logging Shrink Events (Priority: P2)

**Goal**: Log shrink events for auditing

**Independent Test**: Trigger shrink, verify LOG message format

### Implementation for User Story 6

- [X] T050 [US6] Verify LOG message uses pgrx::log! macro with correct format string in src/worker.rs
- [X] T051 [US6] Add WARNING log when execute_alter_system fails for shrink in src/worker.rs
- [X] T052 [US6] Run cargo pgrx test pg18 to verify US6 implementation

**Checkpoint**: Shrink events logged - observability works

---

## Phase 9: Edge Cases

**Purpose**: Handle all edge cases from spec.md

- [X] T053 Add pure Rust #[test] for fractional MB rounding: calculate_shrink_size(1001, 0.75, 100) returns ceil(750.75)=751 in src/worker.rs
- [X] T054 Add pg_regress test for walrus.shrink_factor = 0.0 boundary in tests/pg_regress/sql/shrink_gucs.sql (moved from pg_test - ALTER SYSTEM cannot run in transaction)
- [X] T055 Add pg_regress test for walrus.shrink_factor = 1.0 boundary in tests/pg_regress/sql/shrink_gucs.sql (moved from pg_test - ALTER SYSTEM cannot run in transaction)
- [X] T056 Add pg_regress test for walrus.shrink_intervals = 0 boundary in tests/pg_regress/sql/shrink_gucs.sql (moved from pg_test - ALTER SYSTEM cannot run in transaction)
- [X] T057 Add pg_regress test for walrus.min_size = 1 boundary in tests/pg_regress/sql/shrink_gucs.sql (moved from pg_test - ALTER SYSTEM cannot run in transaction)
- [X] T058 Add pure Rust #[test] for large value: calculate_shrink_size(i32::MAX, 0.99, 1024) in src/worker.rs
- [X] T075 Add #[pg_test] test verifying no shrink when current_size <= min_size (min_size > current scenario) in src/lib.rs
- [X] T076 Shrink_enable GUC reload via SIGHUP is inherent to GucContext::Sighup implementation (no additional test needed)
- [X] T077 Add comment in src/worker.rs documenting quiet_intervals initialization to 0 (restart resets counter - ephemeral state)
- [X] T078 Add #[pg_test] test verifying SUPPRESS_NEXT_SIGHUP flag does not interfere with quiet_intervals counter in src/lib.rs

**Checkpoint**: All edge cases covered

---

## Phase 10: pg_regress SQL Tests

**Purpose**: SQL-level verification of GUC syntax

- [X] T059 [P] Create tests/pg_regress/sql/shrink_gucs.sql with SHOW commands for all 4 shrink GUCs
- [X] T060 Add SET walrus.shrink_enable = false; error case to tests/pg_regress/sql/shrink_gucs.sql
- [X] T061 Create tests/pg_regress/expected/shrink_gucs.out with expected output (plus shrink_gucs_1.out variant for pg15/16)
- [X] T062 Run cargo pgrx regress pg18 --postgresql-conf "shared_preload_libraries='pg_walrus'" to generate and verify expected output

**Checkpoint**: SQL-level tests pass

---

## Phase 11: Multi-Version Testing

**Purpose**: Verify compatibility across PostgreSQL 15, 16, 17, 18

- [X] T063 Run cargo pgrx test pg15 and verify all tests pass (47 passed)
- [X] T064 Run cargo pgrx test pg16 and verify all tests pass (47 passed)
- [X] T065 Run cargo pgrx test pg17 and verify all tests pass (47 passed)
- [X] T066 Run cargo pgrx test pg18 and verify all tests pass (47 passed)
- [X] T067 Run cargo pgrx regress pg15 --postgresql-conf "shared_preload_libraries='pg_walrus'" and verify all tests pass (5 passed)
- [X] T068 Run cargo pgrx regress pg16 --postgresql-conf "shared_preload_libraries='pg_walrus'" and verify all tests pass (5 passed)
- [X] T069 Run cargo pgrx regress pg17 --postgresql-conf "shared_preload_libraries='pg_walrus'" and verify all tests pass (5 passed)
- [X] T070 Run cargo pgrx regress pg18 --postgresql-conf "shared_preload_libraries='pg_walrus'" and verify all tests pass (5 passed)

**Checkpoint**: All PostgreSQL versions pass - version compatibility confirmed

---

## Phase 12: Polish & Cross-Cutting Concerns

**Purpose**: Final verification and documentation

- [X] T071 Run cargo clippy --all-features and fix any warnings (no warnings)
- [X] T072 Run cargo fmt --all and verify formatting (formatted)
- [X] T073 Verify all existing grow tests still pass (no regressions - 47 tests pass)
- [X] T074 Run quickstart.md SQL examples in psql to verify documentation accuracy (GUC SHOW commands verified via pg_regress)

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup - BLOCKS all user stories
- **User Stories (Phase 3-8)**: All depend on Foundational phase completion
  - US1, US2, US3 are P1 priority - implement first
  - US4, US5, US6 are P2 priority - implement after P1s
- **Edge Cases (Phase 9)**: Depends on all user stories (tests edge behaviors)
- **pg_regress (Phase 10)**: Depends on Foundational (GUC registration)
- **Multi-Version (Phase 11)**: Depends on all implementation complete
- **Polish (Phase 12)**: Final phase

### User Story Dependencies

- **User Story 1 (P1)**: Can start after Foundational - provides core shrink logic
- **User Story 2 (P1)**: Can start after US1 - adds floor clamping
- **User Story 3 (P1)**: Can start after US1 - adds counter reset
- **User Story 4 (P2)**: Can start after Foundational - independent enable check
- **User Story 5 (P2)**: Can start after Foundational - configuration tests
- **User Story 6 (P2)**: Can start after US1 - logging verification

### Parallel Opportunities

Within Phase 2 (Foundational):
- T003, T004, T005, T006 can run in parallel (different static declarations)

Within Phase 10 (pg_regress):
- T059, T060 can run in parallel (different SQL content in same file)

---

## Parallel Example: Phase 2

```bash
# Launch all GUC static declarations in parallel:
Task: "Add WALRUS_SHRINK_ENABLE static GucSetting<bool> in src/guc.rs"
Task: "Add WALRUS_SHRINK_FACTOR static GucSetting<f64> in src/guc.rs"
Task: "Add WALRUS_SHRINK_INTERVALS static GucSetting<i32> in src/guc.rs"
Task: "Add WALRUS_MIN_SIZE static GucSetting<i32> in src/guc.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (read existing code)
2. Complete Phase 2: Foundational (register GUCs)
3. Complete Phase 3: User Story 1 (basic shrink)
4. **STOP and VALIDATE**: Test shrink triggers after quiet intervals
5. Deploy/demo if ready

### Incremental Delivery

1. Complete Setup + Foundational -> GUCs available
2. Add User Story 1 -> Test -> Basic shrink works
3. Add User Story 2 -> Test -> Floor clamping works
4. Add User Story 3 -> Test -> Counter reset works
5. Add User Stories 4-6 -> Test -> Full feature complete
6. Edge cases + pg_regress -> Full coverage
7. Multi-version testing -> Ready for release

---

## Notes

- Tests are included as they are required by constitution VIII
- All edge cases from spec.md have corresponding tasks (T053-T058, T075-T078)
- Existing grow functionality must not regress
- Use turbofish syntax for GucSetting::<T>::new() per Rust 2024 edition
- Commit after each logical task group
- Stop at any checkpoint to validate independently
- Total tasks: 78 (T001-T074, T075-T078)
