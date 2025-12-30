# Tasks: History Table

**Input**: Design documents from `/specs/003-history-table/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/sql-functions.md, quickstart.md

**Organization**: Tasks are grouped by user story to enable independent implementation and testing.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

---

## Phase 1: Setup (Infrastructure)

**Purpose**: Project structure changes and module scaffolding

- [ ] T001 Create history module file `src/history.rs` with module documentation
- [ ] T002 Add `mod history;` declaration to `src/lib.rs`
- [ ] T003 [P] Add `serde` and `serde_json` dependencies to `Cargo.toml` for JSONB handling

---

## Phase 2: Foundational (Schema & GUC)

**Purpose**: Core infrastructure that MUST be complete before user story implementation

**CRITICAL**: Schema, table, and GUC must exist before any history operations

### Schema Creation (FR-001, FR-010)

- [ ] T004 Add `extension_sql!` block to `src/lib.rs` with `bootstrap` positioning to create `walrus` schema
- [ ] T005 Add `walrus.history` table DDL in the `extension_sql!` block with all columns per data-model.md
- [ ] T006 Add CHECK constraints for action type ('increase', 'decrease', 'capped') and positive integers
- [ ] T007 Add `walrus_history_timestamp_idx` index on timestamp column in the `extension_sql!` block
- [ ] T008 Add COMMENT statements for table and key columns in the `extension_sql!` block

### GUC Registration (FR-007)

- [ ] T009 Add `WALRUS_HISTORY_RETENTION_DAYS` static to `src/guc.rs` with default 7
- [ ] T010 Add `GucRegistry::define_int_guc()` call in `register_gucs()` for `walrus.history_retention_days` with range 0-3650

**Checkpoint**: Schema, table, index, and GUC ready - history module implementation can begin

---

## Phase 3: User Story 1 - Query Sizing Decision History (Priority: P1)

**Goal**: DBA can query the history table to analyze sizing decisions

**Independent Test**: Insert test records, query with filters, verify results match expectations

### Implementation for User Story 1

- [ ] T011 [US1] Write `#[pg_test]` in `src/lib.rs` tests module: `test_history_table_exists` - verify table exists after CREATE EXTENSION
- [ ] T012 [US1] Write `#[pg_test]` in `src/lib.rs` tests module: `test_history_table_columns` - verify all 9 columns exist with correct types
- [ ] T013 [US1] Write `#[pg_test]` in `src/lib.rs` tests module: `test_history_timestamp_index_exists` - verify index exists
- [ ] T014 [US1] Write `#[pg_test]` in `src/lib.rs` tests module: `test_guc_history_retention_days_default` - verify GUC default is 7
- [ ] T015 [US1] Write `#[pg_test]` in `src/lib.rs` tests module: `test_guc_history_retention_days_range` - verify range 0-3650 via pg_settings
- [ ] T016 [US1] Create `tests/pg_regress/sql/history.sql` with queries per spec acceptance scenarios
- [ ] T017 [US1] Create `tests/pg_regress/expected/history.out` with expected output

**Checkpoint**: User Story 1 complete - history table is queryable with all columns and proper indexing

---

## Phase 4: User Story 2 - Automatic Event Logging (Priority: P1)

**Goal**: Background worker automatically logs sizing decisions to history table

**Independent Test**: Trigger resize events, verify history records are created with correct values

### History Module Core (FR-002, FR-003, FR-004, FR-005, FR-006, FR-012)

- [ ] T018 [US2] Implement `insert_history_record()` function in `src/history.rs` with SPI parameterized INSERT
- [ ] T019 [US2] Add `pgrx::JsonB` handling for metadata parameter in `insert_history_record()`
- [ ] T020 [US2] Implement proper error handling in `insert_history_record()` returning `Result<(), spi::Error>`

### Worker Integration (FR-002, FR-003, FR-004, FR-011)

- [ ] T021 [US2] Add `use crate::history;` import to `src/worker.rs`
- [ ] T022 [US2] Call `insert_history_record()` in GROW PATH after successful `execute_alter_system()` with action='increase'
- [ ] T023 [US2] Call `insert_history_record()` in GROW PATH when size is capped at walrus.max with action='capped'
- [ ] T024 [US2] Call `insert_history_record()` in SHRINK PATH after successful `execute_alter_system()` with action='decrease'
- [ ] T025 [US2] Wrap history insert calls in `BackgroundWorker::transaction()` per research.md pattern
- [ ] T026 [US2] Add graceful error handling: log warning and continue if history insert fails (FR-011)

### Metadata Construction

- [ ] T027 [US2] Create metadata JSON for 'increase' action: `{"delta": N, "multiplier": M, "calculated_size_mb": X}`
- [ ] T028 [US2] Create metadata JSON for 'decrease' action: `{"shrink_factor": F, "quiet_intervals": N, "calculated_size_mb": X}`
- [ ] T029 [US2] Create metadata JSON for 'capped' action: `{"delta": N, "multiplier": M, "calculated_size_mb": X, "walrus_max_mb": Y}`

### Tests for User Story 2

- [ ] T030 [US2] Write `#[pg_test]` in `src/history.rs`: `test_insert_history_record_increase` - verify insert with action='increase'
- [ ] T031 [US2] Write `#[pg_test]` in `src/history.rs`: `test_insert_history_record_decrease` - verify insert with action='decrease'
- [ ] T032 [US2] Write `#[pg_test]` in `src/history.rs`: `test_insert_history_record_capped` - verify insert with action='capped'
- [ ] T033 [US2] Write `#[pg_test]` in `src/history.rs`: `test_insert_history_record_with_metadata` - verify JSONB metadata stored correctly
- [ ] T034 [US2] Write `#[pg_test]` in `src/history.rs`: `test_insert_history_record_null_metadata` - verify NULL metadata works

**Checkpoint**: User Story 2 complete - worker logs all sizing events automatically

---

## Phase 5: User Story 3 - Automatic History Cleanup (Priority: P2)

**Goal**: Old history records are automatically deleted based on retention period

**Independent Test**: Insert old records, call cleanup, verify old records deleted and recent records remain

### Cleanup Function Implementation (FR-008)

- [ ] T035 [US3] Implement `cleanup_old_history()` internal function in `src/history.rs` returning `Result<i64, spi::Error>`
- [ ] T036 [US3] Read `WALRUS_HISTORY_RETENTION_DAYS` GUC value in `cleanup_old_history()`
- [ ] T037 [US3] Execute parameterized DELETE query: `DELETE FROM walrus.history WHERE timestamp < now() - $1 * interval '1 day'`
- [ ] T038 [US3] Return count of deleted records from `cleanup_old_history()`

### SQL-Callable Wrapper (FR-008)

- [ ] T039 [US3] Add `#[pg_schema] mod walrus` block to `src/lib.rs` or `src/history.rs`
- [ ] T040 [US3] Implement `cleanup_history()` as `#[pg_extern]` function returning `Result<i64, spi::Error>`
- [ ] T041 [US3] Have `cleanup_history()` call `cleanup_old_history()` internally

### Worker Integration (FR-009)

- [ ] T042 [US3] Call `cleanup_old_history()` at end of `process_checkpoint_stats()` in `src/worker.rs`
- [ ] T043 [US3] Wrap cleanup call in `BackgroundWorker::transaction()` with error handling

### Edge Cases

- [ ] T044 [US3] Handle retention_days = 0: all records deleted on each cleanup call
- [ ] T045 [US3] Handle empty history table: return 0 records deleted

### Tests for User Story 3

- [ ] T046 [US3] Write `#[pg_test]` in `src/history.rs`: `test_cleanup_history_deletes_old_records`
- [ ] T047 [US3] Write `#[pg_test]` in `src/history.rs`: `test_cleanup_history_preserves_recent_records`
- [ ] T048 [US3] Write `#[pg_test]` in `src/history.rs`: `test_cleanup_history_returns_count`
- [ ] T049 [US3] Write `#[pg_test]` in `src/history.rs`: `test_cleanup_history_retention_zero`
- [ ] T050 [US3] Create `tests/pg_regress/sql/cleanup.sql` with cleanup function SQL tests
- [ ] T051 [US3] Create `tests/pg_regress/expected/cleanup.out` with expected output

**Checkpoint**: User Story 3 complete - history table is automatically pruned

---

## Phase 6: User Story 4 - Compliance Audit Export (Priority: P3)

**Goal**: Compliance officer can export history for audit purposes

**Independent Test**: Export history with COPY TO, verify CSV output is valid

**Note**: This story requires no code implementation - it validates the schema supports standard PostgreSQL export tools.

### Tests for User Story 4

- [ ] T052 [US4] Create `tests/pg_regress/sql/export.sql` with COPY TO export test
- [ ] T053 [US4] Create `tests/pg_regress/expected/export.out` with expected output
- [ ] T054 [US4] Verify JSONB metadata column is preserved in export format

**Checkpoint**: User Story 4 complete - audit export workflows validated

---

## Phase 7: Edge Cases & Error Handling

**Purpose**: Implement edge cases from spec.md - these are REQUIREMENTS, not optional

### History Table Errors (Edge Cases from spec)

- [ ] T055 Write `#[pg_test]` in `src/history.rs`: `test_insert_fails_gracefully_on_error` - verify worker continues if insert fails (covers: table dropped, disk space exhausted, any INSERT error)
- [ ] T056 Implement table existence check in `insert_history_record()` before INSERT
- [ ] T057 Log warning if history table does not exist and continue worker operation

### Concurrent Access (Edge Cases from spec)

- [ ] T058 Write `#[pg_test]` in `src/history.rs`: `test_concurrent_insert_during_cleanup_preserves_new_records` - insert a record, start cleanup in same transaction, verify new record not deleted by concurrent cleanup
- [ ] T059 Document that standard PostgreSQL MVCC handles concurrent access (no additional implementation needed)

### Large Table Cleanup (Edge Cases from spec)

- [ ] T060 Verify timestamp index is used for DELETE via EXPLAIN in pg_regress test

### Performance Verification (Success Criteria from spec)

- [ ] T069 Write `#[pg_test]` in `src/history.rs`: `test_insert_completes_within_one_second` - verify insert timing meets SC-001 (< 1 second)

**Checkpoint**: All edge cases from spec implemented and tested

---

## Phase 8: Polish & Cross-Cutting Concerns

**Purpose**: Final validation and cleanup

- [ ] T061 Run `cargo pgrx test pg18` - verify all new tests pass
- [ ] T062 Run `cargo pgrx regress pg18 --postgresql-conf "shared_preload_libraries='pg_walrus'"` - verify pg_regress tests pass
- [ ] T063 Run multi-version tests: pg15, pg16, pg17, pg18
- [ ] T064 Verify `src/lib.rs` remains under 900 LOC (Constitution XV)
- [ ] T065 Verify `src/history.rs` remains under 900 LOC (Constitution XV)
- [ ] T066 Run `cargo clippy` and fix any warnings
- [ ] T067 Run `cargo fmt` to ensure consistent formatting
- [ ] T068 Run quickstart.md verification commands

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: No dependencies - can start immediately
- **Phase 2 (Foundational)**: Depends on Phase 1 - BLOCKS all user stories
- **Phase 3 (US1)**: Depends on Phase 2 - can start after foundational is complete
- **Phase 4 (US2)**: Depends on Phase 2 - can run in parallel with US1
- **Phase 5 (US3)**: Depends on Phase 2 - can run in parallel with US1/US2
- **Phase 6 (US4)**: Depends on Phase 2 - can run in parallel with US1/US2/US3
- **Phase 7 (Edge Cases)**: Depends on Phase 4, Phase 5 (needs core history functions)
- **Phase 8 (Polish)**: Depends on all previous phases

### User Story Dependencies

- **User Story 1 (P1)**: Query history - No dependencies on other stories
- **User Story 2 (P1)**: Automatic logging - No dependencies on other stories (but core of feature)
- **User Story 3 (P2)**: Automatic cleanup - No dependencies on US1/US2 (operates independently)
- **User Story 4 (P3)**: Audit export - No dependencies (validation only)

### Within Each User Story

- Schema/table must exist before any history operations
- GUC must be registered before cleanup can read retention days
- Internal functions before SQL-callable wrappers
- Worker integration after core functions are implemented

### Parallel Opportunities

**Phase 1**: All tasks can run in parallel (different files)
**Phase 2**: T004-T008 are sequential (single extension_sql! block); T009-T010 can run in parallel with T004-T008
**Phase 3**: T011-T015 can run in parallel (separate tests); T016-T017 can run in parallel
**Phase 4**: T018-T020 sequential; T027-T029 can run in parallel; T030-T034 can run in parallel
**Phase 5**: T035-T038 sequential; T039-T041 sequential; T046-T051 can run in parallel
**Phase 6**: All tasks can run in parallel
**Phase 7**: T055-T057 sequential; T058-T060, T069 can run in parallel
**Phase 8**: T061-T067 mostly sequential (test then fix)

---

## Parallel Example: User Story 2 Tests

```bash
# Launch all US2 tests together:
Task: "Write #[pg_test] test_insert_history_record_increase"
Task: "Write #[pg_test] test_insert_history_record_decrease"
Task: "Write #[pg_test] test_insert_history_record_capped"
Task: "Write #[pg_test] test_insert_history_record_with_metadata"
Task: "Write #[pg_test] test_insert_history_record_null_metadata"
```

---

## Implementation Strategy

### MVP First (User Stories 1 + 2)

1. Complete Phase 1: Setup
2. Complete Phase 2: Foundational (CRITICAL - blocks all stories)
3. Complete Phase 3: User Story 1 (queryable history table)
4. Complete Phase 4: User Story 2 (automatic logging)
5. **STOP and VALIDATE**: Test US1 + US2 independently
6. Deploy/demo if ready - feature is usable without cleanup

### Incremental Delivery

1. Complete Setup + Foundational -> Foundation ready
2. Add User Story 1 -> Test independently -> Table queryable
3. Add User Story 2 -> Test independently -> Events logged (MVP!)
4. Add User Story 3 -> Test independently -> Automatic cleanup
5. Add User Story 4 -> Test independently -> Audit export validated
6. Each story adds value without breaking previous stories

### Single Developer Strategy

1. Phase 1: Setup (T001-T003) - 5 minutes
2. Phase 2: Foundational (T004-T010) - 30 minutes
3. Phase 3: US1 (T011-T017) - 20 minutes
4. Phase 4: US2 (T018-T034) - 60 minutes
5. Phase 5: US3 (T035-T051) - 45 minutes
6. Phase 6: US4 (T052-T054) - 15 minutes
7. Phase 7: Edge Cases (T055-T060, T069) - 35 minutes
8. Phase 8: Polish (T061-T068) - 20 minutes

**Estimated Total**: ~4 hours

---

## Notes

- [P] tasks = different files, no dependencies
- [Story] label maps task to specific user story for traceability
- Each user story should be independently completable and testable
- Commit after each task or logical group
- Stop at any checkpoint to validate story independently
- Avoid: vague tasks, same file conflicts, cross-story dependencies that break independence
- All tests use `#[pg_test]` for PostgreSQL integration or pg_regress for SQL interface
- Background worker already has `pg_test` module with `postgresql_conf_options()` in `src/lib.rs`
