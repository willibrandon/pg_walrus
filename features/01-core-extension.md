# Feature: Core Extension (pgrx Rewrite)

Rewrite pg_walsizer as pg_walrus using Rust and the pgrx framework.

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

## Technical Constraints
- Support PostgreSQL 15, 16, 17, 18
- Handle version-specific field names: `requested_checkpoints` (PG15-16) vs `num_requested` (PG17+)
- Use `#[pg_guard]` for FFI safety
- Atomic signal handling for self-triggered SIGHUP detection

## Module Structure
- `lib.rs` - Entry point, _PG_init, GUC registration
- `worker.rs` - Background worker main loop
- `stats.rs` - Checkpoint statistics access
- `config.rs` - ALTER SYSTEM implementation
- `version_compat.rs` - PG version handling

## Reference
- **pg_walsizer source**: `pg_walsizer/walsizer.c` - Use as reference for background worker, GUC registration, and core logic
- **pg_walsizer header**: `pg_walsizer/walsizer.h`
- **Conversion design**: `CONVERSION_PROPOSAL.md` - C-to-Rust API mappings
- **Feature index**: `features/README.md`
