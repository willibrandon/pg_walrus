# Feature: Core Extension (pgrx)

**Status: COMPLETE** - 132 tests passing across PostgreSQL 15-18.

## What It Does
- Background worker monitors checkpoint activity every `checkpoint_timeout` interval
- Fetches stats via `pgstat_fetch_stat_checkpointer()`
- When forced checkpoints exceed threshold, increases `max_wal_size` using formula: `current * (forced_checkpoints + 1)`
- Applies changes via `ALTER SYSTEM` + `SIGHUP` to postmaster
- Caps at configurable maximum to prevent runaway growth

## GUC Parameters
- `walrus.enable` (bool, default: true) - Enable/disable auto-sizing
- `walrus.max` (int, default: 4GB) - Maximum allowed `max_wal_size`
- `walrus.threshold` (int, default: 2) - Forced checkpoints before resize

## Technical Details
- Supports PostgreSQL 15, 16, 17, 18
- Version-specific field names: `requested_checkpoints` (PG15-16) vs `num_requested` (PG17+)
- Uses `#[pg_guard]` for FFI safety
- Atomic signal handling for self-triggered SIGHUP detection

## Source Structure
- `src/lib.rs` - Entry point, _PG_init, GUC registration, tests
- `src/worker.rs` - Background worker main loop
- `src/stats.rs` - Checkpoint statistics access
- `src/config.rs` - ALTER SYSTEM implementation
- `src/guc.rs` - GUC parameter definitions

## Reference
- **Feature index**: `features/README.md`
