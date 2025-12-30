# Tasks: pg_walrus Core Extension (pgrx Rewrite)

**Input**: Design documents from `/specs/001-pgrx-core-rewrite/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/

**Tests**: Required per Constitution VIII (Test Discipline). Test tasks included in Phase 8. See contracts/testing.md for comprehensive testing guidelines.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3, US4)
- Include exact file paths in descriptions

## Path Conventions

- **Single project**: `src/`, `tests/` at repository root (standard pgrx extension layout)

---

## Phase 1: Setup

**Purpose**: Project initialization and correct dependencies

- [ ] T001 Update Cargo.toml: remove pg13/pg14 features, set default to pg15, add libc dependency, update edition to 2024 in Cargo.toml
- [ ] T002 [P] Create empty module files: src/worker.rs, src/stats.rs, src/config.rs, src/guc.rs
- [ ] T003 [P] Add module declarations to src/lib.rs (mod worker, mod stats, mod config, mod guc)

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core infrastructure that MUST be complete before ANY user story can be implemented

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [ ] T004 Implement GUC parameter definitions in src/guc.rs: WALRUS_ENABLE (bool, default true), WALRUS_MAX (i32 with UNIT_MB, default 4096, min 2), WALRUS_THRESHOLD (i32, default 2, min 1, max 1000) per research.md R5
- [ ] T005 Implement register_gucs() function in src/guc.rs that registers all three GUCs with GucContext::Sighup
- [ ] T006 Implement _PG_init() skeleton in src/lib.rs: check process_shared_preload_libraries_in_progress, call register_gucs(), register background worker with BackgroundWorkerBuilder per research.md R6

**Checkpoint**: Foundation ready - user story implementation can now begin

---

## Phase 3: User Story 1 - Automatic WAL Size Adjustment (Priority: P1) 🎯 MVP

**Goal**: Monitor checkpoint activity and automatically increase max_wal_size when forced checkpoints exceed threshold

**Independent Test**: Generate WAL activity triggering forced checkpoints, verify max_wal_size increases in postgresql.auto.conf

### Implementation for User Story 1

- [ ] T007 [P] [US1] Implement get_requested_checkpoints() in src/stats.rs with version-specific field access using #[cfg(any(feature = "pg15", feature = "pg16"))] for requested_checkpoints and #[cfg(any(feature = "pg17", feature = "pg18"))] for num_requested per research.md R2
- [ ] T008 [P] [US1] Implement get_current_max_wal_size() in src/stats.rs returning pg_sys::max_wal_size_mb per research.md R4
- [ ] T009 [P] [US1] Implement checkpoint_timeout() in src/stats.rs: declare extern C block for CheckPointTimeout (not exposed by pgrx), return Duration per research.md R8
- [ ] T010 [P] [US1] Implement alter_max_wal_size() in src/config.rs constructing AlterSystemStmt, VariableSetStmt, A_Const nodes for max_wal_size per research.md R1
- [ ] T011 [US1] Implement execute_alter_system() in src/config.rs with ResourceOwner setup, StartTransactionCommand, AlterSystemSetConfigFile, CommitTransactionCommand per research.md R7
- [ ] T012 [US1] Implement SUPPRESS_NEXT_SIGHUP atomic flag and send_sighup_to_postmaster() in src/worker.rs using libc::kill() per research.md R3
- [ ] T013 [US1] Implement should_skip_iteration() in src/worker.rs checking atomic flag to detect self-triggered SIGHUP
- [ ] T014 [US1] Implement process_checkpoint_stats() in src/worker.rs: fetch stats, calculate delta, compare to threshold, calculate new size, cap at walrus.max, call execute_alter_system, send SIGHUP per data-model.md Data Flow
- [ ] T015 [US1] Implement walrus_worker_main() in src/worker.rs with main loop using BackgroundWorker::wait_latch(checkpoint_timeout), first_iteration baseline skip, and call to process_checkpoint_stats per background-worker.md Main Loop
- [ ] T016 [US1] Export walrus_worker_main as extern "C-unwind" with #[pg_guard] and #[unsafe(no_mangle)] in src/worker.rs
- [ ] T017 [US1] Wire worker entry point: update BackgroundWorkerBuilder in src/lib.rs to call "walrus_worker_main" function

**Checkpoint**: User Story 1 should be fully functional - forced checkpoints trigger automatic max_wal_size increase

---

## Phase 4: User Story 2 - Runtime Configuration Control (Priority: P2)

**Goal**: Allow runtime modification of GUC parameters without PostgreSQL restart

**Independent Test**: ALTER SYSTEM SET walrus.enable = false; SELECT pg_reload_conf(); verify worker stops making adjustments

### Implementation for User Story 2

- [ ] T018 [US2] Add WALRUS_ENABLE.get() check in walrus_worker_main() main loop to skip processing when disabled in src/worker.rs
- [ ] T019 [US2] Add BackgroundWorker::sighup_received() check with debug logging for configuration reload in src/worker.rs per background-worker.md Signal Handling
- [ ] T020 [US2] Implement reading WALRUS_MAX.get() and WALRUS_THRESHOLD.get() dynamically in process_checkpoint_stats() in src/worker.rs

**Checkpoint**: GUC changes via ALTER SYSTEM + pg_reload_conf() take effect within one monitoring cycle

---

## Phase 5: User Story 4 - Multi-Version PostgreSQL Support (Priority: P2)

**Goal**: Support PostgreSQL 15, 16, 17, and 18 with correct API access for each version

**Independent Test**: cargo pgrx build --features pgXX succeeds for each version; cargo pgrx test pgXX passes

### Implementation for User Story 4

- [ ] T021 [US4] Verify get_requested_checkpoints() compiles with pg15 feature: cargo check --features pg15 --no-default-features
- [ ] T022 [US4] Verify get_requested_checkpoints() compiles with pg16 feature: cargo check --features pg16 --no-default-features
- [ ] T023 [US4] Verify get_requested_checkpoints() compiles with pg17 feature: cargo check --features pg17 --no-default-features
- [ ] T024 [US4] Verify get_requested_checkpoints() compiles with pg18 feature: cargo check --features pg18 --no-default-features

**Checkpoint**: Extension builds and runs correctly on PostgreSQL 15, 16, 17, and 18

---

## Phase 6: User Story 3 - Extension Lifecycle Management (Priority: P3)

**Goal**: Clean startup after recovery, proper signal handling, graceful shutdown

**Independent Test**: Start PostgreSQL with pg_walrus in shared_preload_libraries, verify worker in pg_stat_activity; stop PostgreSQL, verify clean shutdown in logs

### Implementation for User Story 3

- [ ] T025 [US3] Add startup log message "pg_walrus worker started" at beginning of walrus_worker_main() in src/worker.rs per background-worker.md Logging Contract
- [ ] T026 [US3] Add shutdown log message "pg_walrus worker shutting down" after main loop exits in src/worker.rs per background-worker.md Logging Contract
- [ ] T027 [US3] Verify BackgroundWorkerBuilder uses BgWorkerStart_RecoveryFinished (set_start_time) in src/lib.rs per background-worker.md Worker Registration
- [ ] T028 [US3] Verify BackgroundWorker::attach_signal_handlers includes SIGHUP and SIGTERM in src/worker.rs per background-worker.md Signal Handling

**Checkpoint**: Worker starts after recovery, appears in pg_stat_activity as 'pg_walrus', shuts down cleanly

---

## Phase 7: Polish & Edge Cases

**Purpose**: Handle all edge cases from spec.md and cross-cutting concerns

- [ ] T029 Handle null pointer from pgstat_fetch_stat_checkpointer(): return -1 and log warning in src/stats.rs per data-model.md E3 Error Handling
- [ ] T030 Handle already at walrus.max: skip ALTER SYSTEM and log debug message in src/worker.rs per background-worker.md Logging Contract
- [ ] T031 Handle calculated size exceeding walrus.max: cap to walrus.max and log warning in src/worker.rs per guc-interface.md walrus.max Behavior
- [ ] T032 Handle i32 overflow in new size calculation: use saturating_mul and cap before applying walrus.max in src/worker.rs per spec.md Edge Cases
- [ ] T033 [P] Add debug logging for baseline establishment in first iteration in src/worker.rs per background-worker.md Logging Contract
- [ ] T034 [P] Add LOG level message for resize decisions with before/after values in src/worker.rs per background-worker.md Logging Contract
- [ ] T035 Run quickstart.md validation: build, install, configure shared_preload_libraries, restart, verify worker runs
- [ ] T036 Handle ALTER SYSTEM failure: wrap AlterSystemSetConfigFile in error handling, log warning "pg_walrus: failed to execute ALTER SYSTEM, will retry next cycle", and continue monitoring without crashing in src/config.rs per spec.md Edge Cases

---

## Phase 8: Tests (Constitution VIII)

**Purpose**: Verify functionality per Constitution VIII (Test Discipline)

### GUC Parameter Tests

- [ ] T037 [P] Create #[pg_test] test for GUC walrus.enable: verify default is true, SHOW walrus.enable returns 'on' in src/lib.rs
- [ ] T038 [P] Create #[pg_test] test for GUC walrus.max: verify default is 4096, SHOW walrus.max returns '4096' in src/lib.rs
- [ ] T039 [P] Create #[pg_test] test for GUC walrus.threshold: verify default is 2, SHOW walrus.threshold returns '2' in src/lib.rs

### Background Worker Tests

- [ ] T040 Create pg_test module in src/lib.rs with setup() and postgresql_conf_options() returning vec!["shared_preload_libraries='pg_walrus'"] per research.md R9
- [ ] T041 Create #[pg_test] test verifying background worker appears in pg_stat_activity with backend_type = 'pg_walrus' in src/lib.rs per research.md R9

### Pure Rust Unit Tests

- [ ] T042 [P] Create #[test] (pure Rust) test for new size calculation: verify current_size * (delta + 1) formula with inputs (1024, 3) → 4096 in src/worker.rs
- [ ] T043 [P] Create #[test] (pure Rust) test for i32 overflow handling: verify saturating_mul caps at i32::MAX for large inputs in src/worker.rs

### pg_regress SQL Tests

pg_regress tests verify extension behavior via SQL commands. Run with `cargo pgrx regress pgXX`.

- [ ] T044 Update tests/pg_regress/sql/setup.sql: verify CREATE EXTENSION pg_walrus succeeds and extension loads correctly
- [ ] T045 [P] Create tests/pg_regress/sql/guc_params.sql: test SHOW for all three GUCs, SET valid values, SET boundary values (threshold 1 and 1000), verify error on invalid values
- [ ] T046 [P] Create tests/pg_regress/sql/extension_info.sql: verify extension metadata via pg_extension, check extversion matches Cargo.toml version

### pg_regress Expected Output

- [ ] T047 [P] Generate tests/pg_regress/expected/setup.out: run `cargo pgrx regress pg17 --auto` and accept output for setup.sql
- [ ] T048 [P] Generate tests/pg_regress/expected/guc_params.out: run `cargo pgrx regress pg17 --auto` and accept output for guc_params.sql
- [ ] T049 [P] Generate tests/pg_regress/expected/extension_info.out: run `cargo pgrx regress pg17 --auto` and accept output for extension_info.sql

**Checkpoint**: All tests pass with `cargo pgrx test pg17` and `cargo pgrx regress pg17`

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion - BLOCKS all user stories
- **User Stories (Phases 3-6)**: All depend on Foundational phase completion
  - US1 (Phase 3) should be completed first as it implements core functionality
  - US2 (Phase 4) refines worker behavior, can start after T015 is complete
  - US4 (Phase 5) is verification, can run after T007 is complete
  - US3 (Phase 6) is verification, can run after T015-T016 are complete
- **Polish (Phase 7)**: Depends on all user stories being complete
- **Tests (Phase 8)**: Can run after Phase 2 (GUC tests) and Phase 3 (worker tests) are complete

### User Story Dependencies

- **User Story 1 (P1)**: Can start after Foundational (Phase 2) - No dependencies on other stories
- **User Story 2 (P2)**: Requires T015 (main loop) from US1 - adds enable check and reload handling
- **User Story 4 (P2)**: Requires T007 (version-specific stats) from US1 - verification only
- **User Story 3 (P3)**: Requires T015-T016 (worker main) from US1 - adds logging verification

### Within Each User Story

- Models/utilities before main logic
- Core implementation before refinements
- All edge cases handled inline, not deferred

### Parallel Opportunities

- T002, T003 can run in parallel (different files)
- T007, T008, T009, T010 can run in parallel (different files: stats.rs, config.rs)
- T021, T022, T023, T024 can run in parallel (independent build checks)
- T033, T034 can run in parallel (independent logging additions)
- T037, T038, T039 can run in parallel (independent GUC tests)
- T042, T043 can run in parallel (independent pure Rust tests)
- T045, T046 can run in parallel (independent pg_regress SQL tests)
- T047, T048, T049 can run in parallel (independent pg_regress expected output generation)

---

## Parallel Example: User Story 1 Core Implementation

```bash
# Launch stats.rs and config.rs implementations in parallel:
Task: T007 "Implement get_requested_checkpoints() in src/stats.rs"
Task: T008 "Implement get_current_max_wal_size() in src/stats.rs"
Task: T009 "Implement get_checkpoint_timeout() in src/stats.rs"
Task: T010 "Implement alter_max_wal_size() in src/config.rs"

# Then sequentially: T011 depends on T010, T012-T017 depend on T007-T011
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (T001-T003)
2. Complete Phase 2: Foundational (T004-T006)
3. Complete Phase 3: User Story 1 (T007-T017)
4. **STOP and VALIDATE**: Test by generating WAL load, verify max_wal_size changes
5. Deploy/demo if ready

### Incremental Delivery

1. Complete Setup + Foundational → Foundation ready
2. Add User Story 1 → Test with WAL generation → Deploy (MVP!)
3. Add User Story 2 → Test GUC changes → Verify runtime control
4. Add User Story 4 → Build on each PG version → Verify compatibility
5. Add User Story 3 → Check logs and pg_stat_activity → Verify lifecycle
6. Add Polish → Handle edge cases → Edge cases complete
7. Add Tests → Run `cargo pgrx test pg17` → Production ready

---

## Summary

| Phase | Story | Task Count | Parallel Tasks |
|-------|-------|------------|----------------|
| Setup | - | 3 | 2 |
| Foundational | - | 3 | 0 |
| US1 (P1) | Automatic WAL Sizing | 11 | 4 |
| US2 (P2) | Runtime Config | 3 | 0 |
| US4 (P2) | Multi-Version | 4 | 4 |
| US3 (P3) | Lifecycle | 4 | 0 |
| Polish | - | 8 | 2 |
| Tests | Constitution VIII | 13 | 9 |
| **Total** | | **49** | **21** |

### MVP Scope

User Story 1 alone (Phases 1-3) provides the core value proposition: automatic WAL size adjustment. This is 17 tasks and delivers a functional extension.

### Files Modified

| File | Tasks |
|------|-------|
| Cargo.toml | T001 |
| src/lib.rs | T003, T006, T017, T037-T041 |
| src/guc.rs | T002, T004, T005 |
| src/stats.rs | T002, T007, T008, T009, T029 |
| src/config.rs | T002, T010, T011, T036 |
| src/worker.rs | T002, T012-T016, T018-T020, T025-T026, T028, T030-T034, T042-T043 |
| tests/pg_regress/sql/setup.sql | T044, T047 |
| tests/pg_regress/sql/guc_params.sql | T045, T048 |
| tests/pg_regress/sql/extension_info.sql | T046, T049 |
| tests/pg_regress/expected/*.out | T047, T048, T049 |
