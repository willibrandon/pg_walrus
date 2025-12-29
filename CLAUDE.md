# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

pg_walrus is a Rust rewrite (using pgrx) of pg_walsizer - a PostgreSQL extension that automatically monitors and adjusts `max_wal_size` to prevent performance-degrading forced checkpoints. The name comes from WAL + Rust = Walrus.

**Current state**: The repository contains the original C implementation (pg_walsizer) and design documents for the Rust conversion. The Rust implementation has not yet been created.

## Build Commands

### Original C Extension (pg_walsizer)
```bash
cd pg_walsizer
make                    # Build extension
sudo make install       # Install to PostgreSQL
```

### Future Rust Extension (pg_walrus)
```bash
cargo pgrx build --features pg17           # Build for PG17
cargo pgrx test pg17                       # Run tests
cargo pgrx package --pg-config /usr/bin/pg_config  # Create package
```

## Architecture

### Core Mechanism
The extension works by:
1. Running a background worker that wakes every `checkpoint_timeout` interval
2. Fetching checkpoint statistics via `pgstat_fetch_stat_checkpointer()`
3. Counting forced checkpoints since last check
4. If forced checkpoints exceed threshold, calculating new `max_wal_size` as: `current_size * (forced_checkpoints + 1)`
5. Applying changes via `ALTER SYSTEM` + `SIGHUP` to postmaster

### Key Files (C Implementation)
- `pg_walsizer/walsizer.c` - Background worker and main logic (~290 lines)
- `pg_walsizer/walsizer.h` - Header with `PG_MODULE_MAGIC` export

### Planned Rust Structure
```
src/
├── lib.rs              # Entry point, _PG_init, GUC registration
├── worker.rs           # Background worker implementation
├── stats.rs            # Checkpoint statistics access (version-specific)
├── config.rs           # ALTER SYSTEM implementation
└── version_compat.rs   # PG version handling (#[cfg] blocks)
```

## GUC Parameters

| Parameter | Default | Description |
|-----------|---------|-------------|
| `walsizer.enable` / `walrus.enable` | true | Enable/disable auto-sizing |
| `walsizer.max` / `walrus.max` | 4GB | Maximum allowed `max_wal_size` |
| `walsizer.threshold` / `walrus.threshold` | 2 | Forced checkpoints before resize |

## PostgreSQL Version Compatibility

Supports PostgreSQL 15+ due to `pgstat_fetch_stat_checkpointer()` API. Version-specific handling needed for:
- PG 15-16: `stats->requested_checkpoints`
- PG 17+: `stats->num_requested`

## Key Technical Details

- Background worker uses `WaitLatch()` with `checkpoint_timeout` as the interval
- Self-triggered `SIGHUP` detection prevents re-processing own config changes
- Uses `ResourceOwner` for proper cleanup in transaction commands
- `AlterSystemSetConfigFile()` requires AST node construction (`AlterSystemStmt` -> `VariableSetStmt` -> `A_Const`)
