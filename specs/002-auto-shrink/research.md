# Research: Auto-Shrink Feature

**Date**: 2025-12-30
**Feature**: 002-auto-shrink
**Status**: Complete

## Research Tasks

### 1. Float GUC Parameter Support in pgrx

**Question**: How to implement `walrus.shrink_factor` as a floating-point GUC (0.0 < value < 1.0)?

**Finding**: pgrx fully supports `GucSetting<f64>` with `GucRegistry::define_float_guc()`.

**Pattern** (from `/Users/brandon/src/pgrx/pgrx-tests/src/tests/guc_tests.rs:84`):
```rust
static SHRINK_FACTOR: GucSetting<f64> = GucSetting::<f64>::new(0.75);

GucRegistry::define_float_guc(
    c"walrus.shrink_factor",
    c"Multiply by this factor when shrinking max_wal_size",
    c"Must be between 0.0 (exclusive) and 1.0 (exclusive). Lower values shrink more aggressively.",
    &SHRINK_FACTOR,
    0.01,        // min_value (use small positive, not 0.0)
    0.99,        // max_value (use < 1.0, not 1.0)
    GucContext::Sighup,
    GucFlags::default(),
);
```

**Decision**: Use `GucSetting::<f64>::new(0.75)` with turbofish syntax (required for Rust 2024 edition). Min/max bounds enforced at 0.01-0.99 to exclude exact boundaries.

**Alternatives Considered**:
- Using i32 percentage (e.g., 75 for 75%): Rejected - less intuitive for DBAs
- Using string with parsing: Rejected - pgrx has native f64 support

---

### 2. Shrink Calculation and Rounding

**Question**: How to calculate shrink size with correct rounding?

**Finding**: Shrink formula is `current_size * shrink_factor`, rounded up to nearest MB.

**Pattern**:
```rust
/// Calculate shrink target size.
/// Uses f64 multiplication then rounds up via ceiling to ensure we don't under-size.
pub fn calculate_shrink_size(current_size: i32, shrink_factor: f64, min_size: i32) -> i32 {
    let raw = (current_size as f64) * shrink_factor;
    let rounded = raw.ceil() as i32;
    rounded.max(min_size)
}
```

**Decision**: Use `f64::ceil()` for rounding up after multiplication, then clamp to `min_size`. No overflow concern since shrinking always produces smaller values.

**Alternatives Considered**:
- Round down: Rejected - could over-shrink unexpectedly
- Truncate: Rejected - same issue as round down
- Saturating arithmetic: Not needed - shrink always reduces size

---

### 3. Quiet Interval Counter Reset Semantics

**Question**: When exactly should the quiet interval counter reset?

**Finding**: Counter must reset in three scenarios:
1. **Activity detected**: When `delta >= threshold` (forced checkpoints occurred)
2. **Shrink executed**: After successfully shrinking max_wal_size
3. **Grow executed**: When grow logic triggers (existing code path)

**Pattern**:
```rust
// In process_checkpoint_stats:
if delta >= threshold {
    // Grow logic runs...
    quiet_intervals = 0;  // Reset after grow
} else {
    // No significant activity
    quiet_intervals += 1;

    // Check shrink condition
    if should_shrink(quiet_intervals, current_size, min_size) {
        execute_shrink(...);
        quiet_intervals = 0;  // Reset after shrink
    }
}
```

**Decision**: Counter increments only when `delta < threshold`. Resets on any resize (grow or shrink).

**Alternatives Considered**:
- Only reset on activity (not shrink): Rejected - would cause continuous shrinking
- Reset on grow but not shrink: Rejected - inconsistent behavior

---

### 4. Grow/Shrink Mutual Exclusivity

**Question**: How do grow and shrink interact in the same cycle?

**Finding**: Per spec FR-010: "System MUST evaluate shrink condition after evaluating grow condition (shrink happens only if grow did not trigger)."

**Pattern**:
```rust
fn process_checkpoint_stats(..., quiet_intervals: &mut i32) {
    // ... fetch delta ...

    if delta >= threshold {
        // GROW PATH
        *quiet_intervals = 0;  // Reset counter
        // Calculate and apply new size...
        execute_alter_system(new_size);
        return;  // Shrink not evaluated
    }

    // SHRINK PATH (only reached if grow did not trigger)
    *quiet_intervals += 1;

    if shrink_enable && *quiet_intervals >= shrink_intervals && current_size > min_size {
        let new_size = calculate_shrink_size(current_size, shrink_factor, min_size);
        if new_size < current_size {
            // Log and execute shrink
            execute_alter_system(new_size);
            *quiet_intervals = 0;  // Reset after shrink
        }
    }
}
```

**Decision**: Early return from grow path prevents shrink evaluation. Shrink only runs when `delta < threshold`.

**Alternatives Considered**:
- Separate shrink function: Rejected - logic is simple enough for inline
- Shrink in separate cycle: Rejected - unnecessary complexity

---

### 5. SIGHUP Suppression for Shrink

**Question**: Does shrink need separate SIGHUP suppression?

**Finding**: The existing `SUPPRESS_NEXT_SIGHUP` atomic flag in worker.rs handles this. The flag is set before sending SIGHUP and consumed on next iteration, regardless of whether the SIGHUP was triggered by grow or shrink.

**Pattern** (existing in worker.rs):
```rust
fn send_sighup_to_postmaster() {
    SUPPRESS_NEXT_SIGHUP.store(true, Ordering::SeqCst);
    unsafe { libc::kill(pg_sys::PostmasterPid, libc::SIGHUP); }
}
```

**Decision**: Reuse existing `send_sighup_to_postmaster()` function for shrink. No new flag needed.

**Alternatives Considered**:
- Separate flag for shrink: Rejected - single flag works for both cases
- No SIGHUP (rely on auto-reload): Rejected - ALTER SYSTEM requires SIGHUP

---

### 6. Test Strategy for Shrink

**Question**: How to test shrink functionality given background worker timing?

**Finding**: Three-tier testing approach per constitution VIII:

1. **`#[test]`**: Pure Rust tests for `calculate_shrink_size()` function
2. **`#[pg_test]`**: GUC defaults, boundary validation, quiet interval logic
3. **pg_regress**: SQL-level GUC syntax verification

**Pattern**:
```rust
// Tier 1: Pure Rust
#[test]
fn test_calculate_shrink_size() {
    assert_eq!(calculate_shrink_size(4096, 0.75, 1024), 3072);
    assert_eq!(calculate_shrink_size(1024, 0.75, 1024), 1024);  // Clamped
    assert_eq!(calculate_shrink_size(1000, 0.75, 1024), 1024);  // Below min
}

// Tier 2: pg_test
#[pg_test]
fn test_guc_shrink_factor_default() {
    let result = Spi::get_one::<&str>("SHOW walrus.shrink_factor").unwrap();
    assert_eq!(result, Some("0.75"));
}

// Tier 3: pg_regress (sql/shrink_gucs.sql)
SHOW walrus.shrink_enable;
SHOW walrus.shrink_factor;
SHOW walrus.shrink_intervals;
SHOW walrus.min_size;
```

**Decision**: Follow existing test patterns. Add shrink-specific tests to each tier.

**Alternatives Considered**:
- Integration tests with fake clock: Rejected - too complex for value
- Only pg_regress tests: Rejected - insufficient coverage

---

## Summary

All research tasks complete. No blockers identified. The implementation approach is:

1. Add 4 GUC parameters in `guc.rs` using pgrx's `define_float_guc` and `define_int_guc`
2. Add `quiet_intervals` counter to worker state in `worker.rs`
3. Add `calculate_shrink_size()` function with proper rounding
4. Extend `process_checkpoint_stats()` with shrink logic after grow check
5. Reuse existing `execute_alter_system()` and `send_sighup_to_postmaster()`
6. Add three-tier tests following existing patterns

**Next Phase**: Phase 1 - Design artifacts (data-model.md, quickstart.md)
