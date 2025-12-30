# Feature: SQL Observability Functions

Expose extension state and controls via SQL functions for monitoring and management.

## Functions

### walrus_status() -> JSONB
Returns current extension state:
- enabled, current_max_wal_size_mb, configured_maximum_mb
- threshold, checkpoint_timeout_sec, worker_running
- last_check_time, last_adjustment_time, total_adjustments
- shrink_enabled, quiet_intervals

### walrus_history() -> SETOF RECORD
Returns adjustment history table:
- timestamp, action, old_size_mb, new_size_mb
- forced_checkpoints, reason

### walrus_recommendation() -> JSONB
Returns current recommendation without applying:
- current_size_mb, recommended_size_mb
- action (increase/decrease/none), reason, confidence

### walrus_analyze() -> JSONB
Triggers immediate analysis cycle:
- analyzed (bool), recommendation, applied (bool)

### walrus_reset() -> BOOL
Resets extension state (clears history, counters)

## Technical Notes
- Use `#[pg_extern]` macro for function export
- Return JSONB using `pgrx::JsonB` wrapper
- Table-returning functions use `TableIterator`

## Dependencies
- Requires core extension (feature 01)
- walrus_history() requires history table (feature 04)

## Reference
- **Core implementation**: `src/worker.rs`, `src/stats.rs` - Reference for internal state to expose
- **Feature index**: `features/README.md`
