# Research: Rate Limiting

**Feature**: 006-rate-limiting
**Date**: 2025-12-30

## Research Tasks

### 1. Shared Memory Extension Pattern

**Task**: Best practices for extending existing pgrx shared memory structs

**Decision**: Add new fields directly to `WalrusState` struct in `shmem.rs`

**Rationale**:
- pgrx `PgLwLock<T>` wraps the entire struct, so adding fields is safe as long as T remains `Copy + Clone + Default`
- Existing pattern in pg_walrus uses primitive types (i32, i64) which satisfy these bounds
- Adding three new i64/i32 fields maintains the same safety guarantees

**Alternatives Considered**:
- Separate `RateLimiterState` struct with its own `PgLwLock`: Rejected because it would require coordinating two locks and increase complexity
- Store state in a separate table: Rejected because shared memory provides faster access and state is ephemeral (acceptable to lose on restart per FR-016)

**Source**: `/Users/brandon/src/pgrx/pgrx/src/lwlock.rs` - `PgLwLock` implementation shows it works with any `PGRXSharedMemory` type

### 2. Rolling Window Implementation

**Task**: How to implement a rolling one-hour window for `max_changes_per_hour`

**Decision**: Track `hour_window_start` timestamp and reset counter when `now - hour_window_start >= 3600`

**Rationale**:
- Rolling window based on first adjustment in window (not fixed hourly boundaries) matches user expectation from FR-007
- Checking window expiry on each rate limit check is O(1) and adds negligible overhead
- Using Unix timestamps (i64) provides sufficient precision for second-level granularity

**Alternatives Considered**:
- Fixed hourly boundaries (reset at :00): Rejected because spec says "rolling one-hour window" and "1 hour from first change in window"
- Circular buffer of timestamps: Rejected because it requires tracking N timestamps instead of just start time and count

**Source**: Spec FR-007: "System MUST reset the hourly counter when 60 minutes have elapsed since the hour window started"

### 3. Rate Limit Check Order in Worker Loop

**Task**: Where to insert rate limit checks in the existing worker flow

**Decision**: Insert rate limit check immediately after determining an adjustment is needed, before the dry-run check

**Rationale**:
- FR-014 explicitly requires: "Rate limiting checks MUST occur before dry-run checks"
- This ensures dry-run mode accurately reflects what would happen with rate limiting active
- The check occurs in both GROW PATH (after line 109 `if delta >= threshold`) and SHRINK PATH (after shrink conditions pass)

**Implementation Location**:
- GROW PATH: After `if delta >= threshold {` block opens, before calculating new size
- SHRINK PATH: After all shrink conditions pass, before calculating shrink target

**Source**: `src/worker.rs` lines 109-272 (GROW) and 273-412 (SHRINK)

### 4. History Table Schema for 'skipped' Action

**Task**: Verify history table can accept 'skipped' action type

**Decision**: Modify CHECK constraint to include 'skipped' action type

**Rationale**:
- Current CHECK constraint: `action IN ('increase', 'decrease', 'capped', 'dry_run')`
- Must add 'skipped' to allow FR-013: "record skipped adjustments in the history table"
- SQL migration required: `ALTER TABLE walrus.history DROP CONSTRAINT ... ADD CONSTRAINT ...`

**Source**: `src/lib.rs` lines 36-37 show current constraint definition

### 5. GUC Parameter Ranges

**Task**: Validate GUC parameter ranges for `cooldown_sec` and `max_changes_per_hour`

**Decision**:
- `cooldown_sec`: Range 0-86400 (0 disables cooldown, 86400 = 24 hours max)
- `max_changes_per_hour`: Range 0-1000 (0 blocks all adjustments, 1000 is practical upper limit)

**Rationale**:
- FR-017 specifies these ranges explicitly
- Edge case: `cooldown_sec = 0` disables cooldown per spec
- Edge case: `max_changes_per_hour = 0` blocks all automatic adjustments per spec
- Upper bounds prevent configuration errors (no one needs 1000+ changes/hour)

**Source**: FR-017, edge cases in spec

### 6. walrus.force_adjust() Rate Limit Bypass

**Task**: Verify `walrus.force_adjust()` exists and determine bypass implementation

**Finding**: `walrus.force_adjust()` does NOT exist in current codebase. The spec assumes it from feature 004.

**Decision**:
- Check spec assumption: Feature 004 spec lists `walrus.analyze(apply := true)` as the manual adjustment mechanism
- FR-015 requirement: "Manual adjustments via `walrus.force_adjust()` MUST bypass rate limiting"
- Resolution: Either (a) create `walrus.force_adjust()` function, or (b) interpret `walrus.analyze(apply := true)` as the manual intervention that bypasses rate limiting

**Analysis of `walrus.analyze(apply := true)`**:
- Exists in `functions.rs` lines 266-344
- Already performs manual adjustment when `apply = true`
- Superuser-only, suitable for operator intervention
- Does NOT currently record rate limiting state

**Recommended Action**:
- Interpret FR-015 as applying to `walrus.analyze(apply := true)` since this is the existing manual adjustment mechanism
- Update spec assumption to clarify this mapping
- Ensure `walrus.analyze(apply := true)` bypasses rate limiting and still updates rate limit counters appropriately

**Source**: `src/functions.rs` analyze() implementation, spec FR-015

### 7. Timestamp Source for Rate Limiting

**Task**: Determine best timestamp source for cooldown calculations

**Decision**: Use existing `shmem::now_unix()` function which calls `std::time::SystemTime::now()`

**Rationale**:
- Already used throughout codebase for `last_check_time` and `last_adjustment_time`
- Provides Unix timestamp in seconds, sufficient for cooldown granularity
- Edge case (clock skew): Spec states "safe failure mode" if clock jumps backward

**Note on Monotonic Time**:
- Spec mentions "monotonic timestamps derived from PostgreSQL's internal clock"
- However, existing code uses `SystemTime::now()` which is wall-clock time
- For rate limiting purposes, wall-clock is acceptable because:
  - Worst case: clock jumps backward → cooldown appears longer → safe (no adjustment thrashing)
  - PostgreSQL NTP discipline typically prevents large jumps
- Decision: Use existing `now_unix()` for consistency

**Source**: `src/shmem.rs` lines 111-117

### 8. walrus.status() Extension

**Task**: Determine fields to add to `walrus.status()` JSONB output

**Decision**: Add the following fields per FR-012:
- `cooldown_active`: boolean (computed: `last_change_time + cooldown_sec > now`)
- `cooldown_remaining_sec`: integer or null (computed: max(0, last_change_time + cooldown_sec - now))
- `last_change_time`: ISO 8601 timestamp or null (renamed from `last_adjustment_time` for clarity, or keep both)
- `changes_this_hour`: integer
- `hourly_window_start`: ISO 8601 timestamp or null
- `hourly_limit_reached`: boolean (computed: `changes_this_hour >= max_changes_per_hour`)

**Rationale**:
- User Story 4 acceptance scenarios require these specific fields
- Using ISO 8601 timestamps consistent with existing `last_check_time` and `last_adjustment_time` fields
- Computing `cooldown_active` and `cooldown_remaining_sec` on read provides real-time status

**Source**: FR-012, User Story 4

## Summary

All research tasks completed. No NEEDS CLARIFICATION items remain. Key findings:

1. Extend `WalrusState` struct with 3 new fields (last_change_time, changes_this_hour, hour_window_start)
2. Rate limit checks go before dry-run checks in worker loop
3. History table needs schema migration to add 'skipped' action type
4. `walrus.analyze(apply := true)` is the manual adjustment mechanism that bypasses rate limiting
5. Use existing `now_unix()` for timestamps
6. Add 6 new fields to `walrus.status()` output
