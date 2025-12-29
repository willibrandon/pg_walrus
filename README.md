# pg_walrus

**WAL + Rust = Walrus**

A PostgreSQL extension that automatically monitors and adjusts `max_wal_size` to prevent performance-degrading forced checkpoints. pg_walrus is a Rust rewrite of [pg_walsizer](https://github.com/pgedge/pg_walsizer) using the [pgrx](https://github.com/pgcentralfoundation/pgrx) framework.

## The Problem

PostgreSQL's `max_wal_size` determines how much WAL data can accumulate between checkpoints. When this limit is exceeded before `checkpoint_timeout`, PostgreSQL forces a checkpoint—which can dramatically reduce OLTP performance (sometimes by an order of magnitude).

The default `max_wal_size` is only 1GB, and most active systems need more. But how much more? This extension removes the guesswork.

## How It Works

pg_walrus runs a background worker that:

1. Wakes every `checkpoint_timeout` interval
2. Checks how many forced checkpoints occurred
3. If forced checkpoints exceed the threshold, calculates a new `max_wal_size`
4. Applies the change via `ALTER SYSTEM` and signals PostgreSQL to reload

```
LOG:  detected 4 forced checkpoints over 60 seconds
LOG:  WAL request threshold (2) met, resizing max_wal_size
LOG:  current max_wal_size is 512, should be 2560
LOG:  received SIGHUP, reloading configuration files
LOG:  parameter "max_wal_size" changed to "2560"
```

## Features

### Current (from pg_walsizer)
- Background worker monitoring checkpoint activity
- Automatic `max_wal_size` increases when forced checkpoints exceed threshold
- Configurable maximum cap to prevent runaway growth
- Live configuration updates via `ALTER SYSTEM` + `SIGHUP`

### Planned (pg_walrus enhancements)
- **Auto-Shrink**: Automatically reduce size when workload decreases
- **SQL Functions**: Query status, history, and recommendations via SQL
- **History Table**: Full audit trail of all adjustments
- **Dry-Run Mode**: Test behavior without making changes
- **Rate Limiting**: Prevent thrashing on unstable workloads
- **NOTIFY Events**: Real-time notifications on adjustments
- **Prometheus Metrics**: Standard monitoring integration
- **Smart Algorithms**: Multiple sizing strategies (adaptive, percentile)

## Installation

### Prerequisites

- Rust toolchain (rustc, cargo, rustfmt) from https://rustup.rs
- libclang 11+ (for bindgen)
- PostgreSQL build dependencies

See the [pgrx system requirements](https://github.com/pgcentralfoundation/pgrx#system-requirements) for platform-specific details.

### Building from Source (Rust)

```bash
# Install cargo-pgrx
cargo install --locked cargo-pgrx

# Initialize pgrx (downloads and compiles Postgres versions to ~/.pgrx/)
# For a single version:
cargo pgrx init --pg17 download

# Or for all supported versions:
cargo pgrx init

# Clone and build pg_walrus
git clone https://github.com/willibrandon/pg_walrus.git
cd pg_walrus

# Run interactively with psql
cargo pgrx run pg17

# Run tests
cargo pgrx test pg17

# Create installation package
cargo pgrx package
```

### Installing to System Postgres

```bash
# Install to Postgres found via pg_config on $PATH
cargo pgrx install --release

# Or with sudo if needed
cargo pgrx install --release --sudo
```

### Original C Version (pg_walsizer)

```bash
cd pg_walsizer
make
sudo make install
```

### Enable the Extension

Add to `postgresql.conf` (or use `ALTER SYSTEM`):

```sql
-- Add to existing shared_preload_libraries
ALTER SYSTEM SET shared_preload_libraries = 'pg_stat_statements, pg_walrus';
```

Or in `postgresql.conf`:

```ini
shared_preload_libraries = 'pg_walrus'  # add to existing list if needed
```

Restart PostgreSQL:

```bash
pg_ctl restart -D $PGDATA
```

## Configuration

| Parameter | Default | Description |
|-----------|---------|-------------|
| `walrus.enable` | `true` | Enable/disable automatic resizing |
| `walrus.max` | `4GB` | Maximum allowed `max_wal_size` |
| `walrus.threshold` | `2` | Forced checkpoints before resize |

All parameters require `SIGHUP` to take effect (no restart needed).

### Example

```sql
-- Set maximum to 16GB
ALTER SYSTEM SET walrus.max = '16GB';

-- Increase threshold for batch-heavy workloads
ALTER SYSTEM SET walrus.threshold = 5;

-- Reload configuration
SELECT pg_reload_conf();
```

## PostgreSQL Version Support

- PostgreSQL 15
- PostgreSQL 16
- PostgreSQL 17
- PostgreSQL 18 (when available)

Requires PostgreSQL 15+ due to `pgstat_fetch_stat_checkpointer()` API.

## Project Status

| Component | Status |
|-----------|--------|
| Original C extension (pg_walsizer) | Complete |
| Rust conversion design | Complete |
| Rust implementation | In Progress |
| Enhanced features | Planned |

## Documentation

| Document | Description |
|----------|-------------|
| [CONVERSION_PROPOSAL.md](CONVERSION_PROPOSAL.md) | Technical design for C to Rust conversion |
| [ENHANCEMENTS_PROPOSAL.md](ENHANCEMENTS_PROPOSAL.md) | New features and improvements |
| [features/](features/) | Speckit feature documents |
| [pg_walsizer/README.md](pg_walsizer/README.md) | Original pg_walsizer documentation |

## License

PostgreSQL License (same as PostgreSQL itself)

## Links

- **pg_walrus**: https://github.com/willibrandon/pg_walrus
- **Original pg_walsizer**: https://github.com/pgedge/pg_walsizer
- **pgrx Framework**: https://github.com/pgcentralfoundation/pgrx
