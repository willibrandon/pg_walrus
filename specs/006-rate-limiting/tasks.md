# Tasks: Rate Limiting

**Input**: Design documents from `/specs/006-rate-limiting/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, quickstart.md

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1, US2, US3, US4)
- Include exact file paths in descriptions

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Add rate limiting state to shared memory and update SQL schema

- [X] T001 [P] Add `changes_this_hour` (i32) and `hour_window_start` (i64) fields to `WalrusState` struct in `src/shmem.rs`
- [X] T002 [P] Update `reset_state()` function in `src/shmem.rs` to reset new rate limiting fields to 0 per FR-019
- [X] T003 Add 'skipped' to CHECK constraint in `walrus.history` table definition in `src/lib.rs` (lines 37)

**Checkpoint**: ✅ Rate limiting state storage and schema ready for feature implementation

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core GUC parameters and rate limiting check function that ALL user stories depend on

**CRITICAL**: No user story work can begin until this phase is complete

- [X] T004 [P] Add `WALRUS_COOLDOWN_SEC` GucSetting (i32, default: 300) in `src/guc.rs`
- [X] T005 [P] Add `WALRUS_MAX_CHANGES_PER_HOUR` GucSetting (i32, default: 4) in `src/guc.rs`
- [X] T006 Register `walrus.cooldown_sec` GUC with GucRegistry in `src/guc.rs` (range 0-86400, GucContext::Sighup)
- [X] T007 Register `walrus.max_changes_per_hour` GUC with GucRegistry in `src/guc.rs` (range 0-1000, GucContext::Sighup)
- [X] T008 Create `check_rate_limit()` function in `src/worker.rs` that checks cooldown and hourly limit, returns `Option<(String, serde_json::Value)>` with block reason and metadata if blocked

**Checkpoint**: ✅ Foundation ready - GUCs registered, rate limit check function available for user stories

---

## Phase 3: User Story 1 - Prevent Thrashing During Workload Spikes (Priority: P1) 🎯 MVP

**Goal**: Enforce cooldown period between adjustments to prevent rapid successive changes

**Independent Test**: Simulate rapid checkpoint threshold exceedances and verify only first adjustment applies

### Implementation for User Story 1

- [X] T009 [US1] Implement cooldown check logic in `check_rate_limit()`: compare `last_adjustment_time + cooldown_sec` vs `now_unix()` in `src/worker.rs`
- [X] T010 [US1] Insert rate limit check after grow threshold detection (after `if delta >= threshold`) in GROW PATH of `src/worker.rs`
- [X] T011 [US1] Insert rate limit check after shrink conditions pass in SHRINK PATH of `src/worker.rs`
- [X] T012 [US1] Add LOG level message when adjustment blocked by cooldown: "pg_walrus: adjustment blocked - cooldown active (N seconds remaining)" in `src/worker.rs`
- [X] T013 [US1] When cooldown blocks adjustment, call `history::insert_history_record()` with action='skipped', reason='cooldown active', metadata containing `blocked_by` and `cooldown_remaining_sec` in `src/worker.rs`

**Checkpoint**: ✅ Cooldown period enforcement complete - adjustments rate-limited by minimum interval

---

## Phase 4: User Story 2 - Limit Maximum Adjustments Per Hour (Priority: P2)

**Goal**: Enforce maximum adjustments per rolling one-hour window as secondary safety net

**Independent Test**: Simulate multiple adjustments across cooldown windows within an hour and verify hourly counter blocks beyond limit

### Implementation for User Story 2

- [X] T014 [US2] Implement rolling window expiry check in `check_rate_limit()`: if `now_unix() - hour_window_start >= 3600`, reset `changes_this_hour = 1` and `hour_window_start = now` in `src/worker.rs`
- [X] T015 [US2] Implement hourly limit check in `check_rate_limit()`: compare `changes_this_hour >= max_changes_per_hour` (only if `max_changes_per_hour > 0`) in `src/worker.rs`
- [X] T016 [US2] Update shared memory state on successful adjustment: `changes_this_hour += 1`, update `hour_window_start` if expired in `src/worker.rs`
- [X] T017 [US2] Add LOG level message when adjustment blocked by hourly limit: "pg_walrus: adjustment blocked - hourly limit reached (N of M)" in `src/worker.rs`
- [X] T018 [US2] When hourly limit blocks adjustment, call `history::insert_history_record()` with action='skipped', reason='hourly limit reached', metadata containing `blocked_by` and `changes_this_hour` in `src/worker.rs`

**Checkpoint**: ✅ Hourly limit enforcement complete - adjustments capped per rolling hour window

---

## Phase 5: User Story 3 - Configure Rate Limiting Parameters (Priority: P3)

**Goal**: Allow DBAs to tune rate limiting behavior via GUC parameters

**Independent Test**: Modify GUC parameters at runtime and verify new values affect rate limiting behavior immediately

### Implementation for User Story 3

- [X] T019 [P] [US3] Add `#[pg_test]` test `test_guc_cooldown_sec_default` verifying SHOW walrus.cooldown_sec returns '300' in `src/guc.rs` or `src/tests.rs`
- [X] T020 [P] [US3] Add `#[pg_test]` test `test_guc_cooldown_sec_range` verifying min_val=0, max_val=86400 from pg_settings in `src/guc.rs` or `src/tests.rs`
- [X] T021 [P] [US3] Add `#[pg_test]` test `test_guc_max_changes_per_hour_default` verifying SHOW walrus.max_changes_per_hour returns '4' in `src/guc.rs` or `src/tests.rs`
- [X] T022 [P] [US3] Add `#[pg_test]` test `test_guc_max_changes_per_hour_range` verifying min_val=0, max_val=1000 from pg_settings in `src/guc.rs` or `src/tests.rs`
- [X] T023 [US3] Add pg_regress test `rate_limiting.sql` verifying ALTER SYSTEM SET walrus.cooldown_sec and walrus.max_changes_per_hour work in `tests/pg_regress/sql/`
- [X] T024 [US3] Create expected output file `rate_limiting.out` for pg_regress test in `tests/pg_regress/expected/`

**Checkpoint**: ✅ GUC parameters fully tested - configuration interface validated

---

## Phase 6: User Story 4 - Rate Limiting Observability (Priority: P3)

**Goal**: Expose rate limiting state via walrus.status() and record skipped adjustments in history

**Independent Test**: Trigger rate-limited scenarios and verify walrus.status() returns accurate metrics

### Implementation for User Story 4

- [X] T025 [US4] Add `cooldown_sec` field (GUC value) to walrus.status() output in `src/functions.rs`
- [X] T026 [US4] Add `max_changes_per_hour` field (GUC value) to walrus.status() output in `src/functions.rs`
- [X] T027 [US4] Add `cooldown_active` computed field (boolean) to walrus.status() output in `src/functions.rs`
- [X] T028 [US4] Add `cooldown_remaining_sec` computed field (integer, 0 if not active) to walrus.status() output in `src/functions.rs`
- [X] T029 [US4] Add `changes_this_hour` field (from shmem) to walrus.status() output in `src/functions.rs`
- [X] T030 [US4] Add `hourly_window_start` field (ISO 8601 timestamp, null if 0) to walrus.status() output in `src/functions.rs`
- [X] T031 [US4] Add `hourly_limit_reached` computed field (boolean) to walrus.status() output in `src/functions.rs`
- [X] T032 [US4] Ensure `walrus.analyze(apply := true)` bypasses rate limiting (does not check or update rate limit state) per FR-015 in `src/functions.rs`
- [X] T033 [P] [US4] Add `#[pg_test]` test `test_status_rate_limiting_fields` verifying all 7 new fields present in walrus.status() output in `src/tests.rs`
- [X] T034 [P] [US4] Add `#[pg_test]` test `test_history_skipped_action` verifying action='skipped' can be inserted and queried in `src/history.rs`

**Checkpoint**: ✅ Rate limiting fully observable via status() and history

---

## Phase 7: Edge Cases & Integration

**Purpose**: Handle all edge cases from spec.md

- [X] T035 [P] Handle `cooldown_sec = 0` edge case: skip cooldown check entirely in `check_rate_limit()` in `src/worker.rs`
- [X] T036 [P] Handle `max_changes_per_hour = 0` edge case: block all automatic adjustments in `check_rate_limit()` in `src/worker.rs`
- [X] T037 Verify cooldown check order: cooldown checked BEFORE hourly limit per edge case spec in `src/worker.rs`
- [X] T038 Verify rate limit check occurs BEFORE dry-run check per FR-014 in `src/worker.rs`
- [X] T039 [P] Add `#[pg_test]` test `test_cooldown_zero_disables_cooldown` verifying cooldown_sec=0 allows immediate adjustments in `src/tests.rs`
- [X] T040 [P] Add `#[pg_test]` test `test_max_changes_zero_blocks_all` verifying max_changes_per_hour=0 blocks all automatic adjustments in `src/tests.rs`
- [X] T041 [P] Add `#[pg_test]` test `test_reset_clears_rate_limit_state` verifying walrus.reset() clears changes_this_hour and hour_window_start in `src/tests.rs`
- [X] T046 [P] Add `#[pg_test]` test `test_restart_clears_rate_limit_state` verifying changes_this_hour and hour_window_start are 0 after fresh extension load in `src/tests.rs`
- [X] T047 [P] Add `#[pg_test]` test `test_cooldown_boundary_allows_adjustment` verifying adjustment proceeds when last_adjustment_time + cooldown_sec == now (strict inequality) in `src/tests.rs`
- [X] T048 [P] Add `#[pg_test]` test `test_dry_run_counts_for_rate_limiting` verifying dry-run adjustment increments changes_this_hour in `src/tests.rs`
- [X] T049 [P] Add `#[pg_test]` test `test_cooldown_checked_before_hourly` verifying cooldown is checked first and hourly counter not incremented when cooldown blocks in `src/tests.rs`
- [X] T050 [P] Add `#[pg_test]` test `test_clock_skew_extends_cooldown_safely` verifying backward clock jump extends cooldown rather than allowing premature adjustment in `src/tests.rs`

**Checkpoint**: ✅ All edge cases handled - feature complete

---

## Phase 8: Polish & Cross-Cutting Concerns

**Purpose**: Final validation and documentation

- [X] T042 Run `cargo pgrx test pg15 pg16 pg17 pg18` to verify all tests pass across PostgreSQL versions
- [X] T043 Run `cargo pgrx regress pg15 pg16 pg17 pg18 --postgresql-conf "shared_preload_libraries='pg_walrus'"` to verify pg_regress tests pass
- [X] T044 Validate quickstart.md examples work correctly with actual implementation
- [X] T045 Verify file sizes remain under 900 LOC limit after modifications

**Final Results:**
- ✅ 121 pgrx tests pass on PG15, PG16, PG17, PG18
- ✅ 11 pg_regress tests pass on PG15, PG16, PG17, PG18

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Phase 1 completion - BLOCKS all user stories
- **User Stories (Phase 3-6)**: All depend on Foundational phase completion
  - User Story 1 (Phase 3): Must complete before Phase 4 (US2 depends on cooldown state updates)
  - User Story 2 (Phase 4): Extends check_rate_limit() from US1
  - User Story 3 (Phase 5): Can run in parallel with US1/US2 (tests GUCs)
  - User Story 4 (Phase 6): Can run in parallel with US1/US2 (adds observability)
- **Edge Cases (Phase 7)**: Depends on US1 and US2 completion
- **Polish (Phase 8)**: Depends on all user stories and edge cases complete

### User Story Dependencies

- **User Story 1 (P1)**: Can start after Foundational (Phase 2)
- **User Story 2 (P2)**: Depends on US1 completion (shares check_rate_limit function)
- **User Story 3 (P3)**: Can start after Foundational (Phase 2) - tests GUCs only
- **User Story 4 (P3)**: Can start after Foundational (Phase 2) - observability only

### Within Each User Story

- Worker integration before logging
- Logging before history recording
- Implementation before tests

### Parallel Opportunities

- T001, T002, T003 can run in parallel (different files)
- T004, T005 can run in parallel (same file, independent definitions)
- T019-T022 can run in parallel (independent test functions)
- T025-T031 can be grouped (all modify functions.rs status())
- T033, T034 can run in parallel (independent tests)
- T035, T036, T039-T041, T046-T050 can run in parallel after dependencies met

---

## Parallel Example: Phase 1 Setup

```bash
# Launch all setup tasks together (different files):
Task: "Add changes_this_hour and hour_window_start fields to WalrusState in src/shmem.rs"
Task: "Update reset_state() to reset new fields in src/shmem.rs"
Task: "Add 'skipped' to CHECK constraint in src/lib.rs"
```

---

## Parallel Example: User Story 3 Tests

```bash
# Launch all GUC tests together (independent tests):
Task: "test_guc_cooldown_sec_default in src/tests.rs"
Task: "test_guc_cooldown_sec_range in src/tests.rs"
Task: "test_guc_max_changes_per_hour_default in src/tests.rs"
Task: "test_guc_max_changes_per_hour_range in src/tests.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (shmem fields, schema)
2. Complete Phase 2: Foundational (GUCs, check_rate_limit skeleton)
3. Complete Phase 3: User Story 1 (cooldown enforcement)
4. **STOP and VALIDATE**: Test cooldown blocking works correctly
5. Manual testing with `walrus.status()` to verify cooldown_active

### Incremental Delivery

1. Complete Setup + Foundational → Infrastructure ready
2. Add User Story 1 → Cooldown enforcement → MVP!
3. Add User Story 2 → Hourly limit enforcement
4. Add User Story 3 → GUC parameter tests
5. Add User Story 4 → Full observability
6. Complete Edge Cases → Production ready

### File Modification Summary

| File | Changes |
|------|---------|
| `src/shmem.rs` | Add 2 fields to WalrusState, update reset_state() |
| `src/guc.rs` | Add 2 GucSettings, register 2 GUCs |
| `src/worker.rs` | Add check_rate_limit(), integrate into grow/shrink paths |
| `src/functions.rs` | Extend status() with 7 new fields, ensure analyze() bypass |
| `src/lib.rs` | Add 'skipped' to CHECK constraint |
| `src/tests.rs` | Add rate limiting tests |
| `src/history.rs` | (No changes - already supports action='skipped' with current infrastructure) |
| `tests/pg_regress/sql/rate_limiting.sql` | New file - GUC SQL tests |
| `tests/pg_regress/expected/rate_limiting.out` | New file - expected output |

---

## Notes

- Reuse existing `last_adjustment_time` field for cooldown calculations (per data-model.md)
- Use existing `now_unix()` function for timestamp generation
- Use existing `insert_history_record()` for skipped action recording
- All rate limiting state is ephemeral (lost on PostgreSQL restart) per FR-016
- Manual adjustments via `walrus.analyze(apply := true)` bypass rate limiting per FR-015
