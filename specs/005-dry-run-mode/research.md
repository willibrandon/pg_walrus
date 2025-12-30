# Research: Dry-Run Mode

**Feature**: 005-dry-run-mode
**Date**: 2025-12-30
**Status**: Complete

## Research Topics

### 1. GUC Registration Pattern for Boolean Parameters

**Question**: How should `walrus.dry_run` be registered following existing pg_walrus patterns?

**Decision**: Use `GucRegistry::define_bool_guc()` with `GucContext::Sighup` and `GucFlags::default()`, matching the existing `WALRUS_ENABLE` and `WALRUS_SHRINK_ENABLE` patterns.

**Rationale**: The existing codebase (`src/guc.rs`) already has two boolean GUCs that serve as templates:
- `WALRUS_ENABLE` - controls whether sizing is active
- `WALRUS_SHRINK_ENABLE` - controls whether shrinking is active

Both use:
```rust
pub static WALRUS_DRY_RUN: GucSetting<bool> = GucSetting::<bool>::new(false);

GucRegistry::define_bool_guc(
    c"walrus.dry_run",
    c"Enable dry-run mode (log decisions without applying)",
    c"When enabled, pg_walrus logs sizing decisions but does not execute ALTER SYSTEM.",
    &WALRUS_DRY_RUN,
    GucContext::Sighup,
    GucFlags::default(),
);
```

**Alternatives Considered**:
- `GucContext::Userset` - Rejected: dry-run is a safety feature that should require elevated privileges or config reload
- `GucContext::Suset` - Considered: allows superuser to change anytime, but SIGHUP is consistent with other walrus GUCs

### 2. Dry-Run Decision Execution Path

**Question**: Where in the code should the dry-run check be placed?

**Decision**: Check `WALRUS_DRY_RUN.get()` at the point where `execute_alter_system()` would be called, and branch to log-only behavior.

**Rationale**: The decision logic in `worker.rs:process_checkpoint_stats()` has two decision points:
1. **GROW PATH** (line ~160): After calculating new size, before `execute_alter_system()`
2. **SHRINK PATH** (line ~270): After calculating shrink size, before `execute_alter_system()`

The dry-run check should:
1. Read `WALRUS_DRY_RUN.get()` once at the start of `process_checkpoint_stats()`
2. In both paths, replace `execute_alter_system()` + `send_sighup_to_postmaster()` with log-only code when dry-run is true
3. Continue to call `insert_history_record()` with modified action and metadata

**Alternatives Considered**:
- Check inside `execute_alter_system()` - Rejected: would still update shared memory state inconsistently
- Separate dry-run worker loop - Rejected: unnecessary duplication; single check is cleaner

### 3. History Record Format for Dry-Run Decisions

**Question**: How should dry-run decisions be recorded in `walrus.history`?

**Decision**: Use `action = 'dry_run'` with enhanced metadata containing `would_apply` and `dry_run: true` fields.

**Rationale**: The existing `insert_history_record()` function accepts:
- `action: &str` - currently 'increase', 'decrease', 'capped'
- `metadata: Option<JsonValue>` - algorithm details

For dry-run, the action column needs a new value that clearly distinguishes simulated decisions from actual ones. The metadata should indicate:
1. What action WOULD have been taken (`would_apply`: 'increase', 'decrease', 'capped')
2. That this was a simulation (`dry_run: true`)
3. All algorithm details (same as normal operation)

Example metadata structure:
```json
{
  "dry_run": true,
  "would_apply": "increase",
  "delta": 5,
  "multiplier": 6,
  "calculated_size_mb": 6144
}
```

**Alternatives Considered**:
- Add `is_dry_run: bool` column to history table - Rejected: schema change for a modifier; metadata is more flexible
- Prefix action with 'dry_run_' (e.g., 'dry_run_increase') - Rejected: single 'dry_run' action is cleaner for querying

### 4. Log Message Format

**Question**: What format should dry-run log messages use?

**Decision**: Use `pgrx::log!()` with `[DRY-RUN]` prefix and parenthetical reason, matching existing log patterns.

**Rationale**: Existing pg_walrus logs use this pattern:
```
LOG: pg_walrus: resizing max_wal_size from 1024 MB to 2048 MB
```

For dry-run, the format should be:
```
LOG: pg_walrus [DRY-RUN]: would change max_wal_size from 1024 MB to 2048 MB (threshold exceeded)
```

The `[DRY-RUN]` prefix:
- Appears immediately after `pg_walrus` for easy grep filtering
- Uses brackets for visual distinction
- Followed by `: would change` to indicate hypothetical action

The parenthetical reason:
- `(threshold exceeded)` for grow decisions
- `(sustained low activity)` for shrink decisions
- `(capped at walrus.max)` for capped decisions

**Alternatives Considered**:
- `[SIMULATION]` prefix - Rejected: 'dry-run' is more familiar terminology
- `LOG: pg_walrus [DRY-RUN] would change...` (no colon) - Rejected: colon maintains log format consistency

### 5. Shared Memory State Behavior

**Question**: Should shared memory state (quiet_intervals, prev_requested, total_adjustments) update during dry-run?

**Decision**: Yes - all state updates EXCEPT `total_adjustments` and `last_adjustment_time` should occur normally.

**Rationale**:
- `quiet_intervals` and `prev_requested` track algorithm state; these MUST update for the algorithm to function correctly
- If these don't update during dry-run, switching from dry-run to active mode would cause incorrect decisions
- `total_adjustments` and `last_adjustment_time` track actual changes; these should NOT increment for dry-run decisions

This ensures seamless transitions: enabling dry-run doesn't break algorithm state, and disabling it doesn't cause sudden incorrect decisions.

**Alternatives Considered**:
- Separate "dry-run state" - Rejected: unnecessary complexity; algorithm state should be consistent
- Don't update any state - Rejected: would break algorithm and cause incorrect decisions on mode switch

## Summary

All research topics resolved. No NEEDS CLARIFICATION markers remain. The implementation approach is:

1. **GUC**: Add `WALRUS_DRY_RUN` boolean GUC in `guc.rs` with SIGHUP context
2. **Worker Logic**: Check dry-run flag in `process_checkpoint_stats()`, branch to log-only path
3. **History**: Use `action = 'dry_run'` with `would_apply` and algorithm details in metadata
4. **Logging**: `LOG: pg_walrus [DRY-RUN]: would change max_wal_size from X MB to Y MB (reason)`
5. **State**: Update algorithm state normally; skip adjustment counters for dry-run decisions
