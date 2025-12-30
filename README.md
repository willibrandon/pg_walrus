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

### Current
- Background worker monitoring checkpoint activity
- Automatic `max_wal_size` increases when forced checkpoints exceed threshold
- Configurable maximum cap to prevent runaway growth
- Live configuration updates via `ALTER SYSTEM` + `SIGHUP`
- **Auto-Shrink**: Automatically reduce `max_wal_size` after sustained periods of low checkpoint activity
- **History Table**: Full audit trail of all sizing adjustments in `walrus.history`

### Planned
- **SQL Functions**: Query status and recommendations via SQL
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
cargo pgrx init --pg18 download

# Or for all supported versions:
cargo pgrx init

# Clone and build pg_walrus
git clone https://github.com/willibrandon/pg_walrus.git
cd pg_walrus

# Run interactively with psql
cargo pgrx run pg18

# Run integration tests
cargo pgrx test pg18

# Run SQL regression tests (requires --postgresql-conf for background worker)
cargo pgrx regress pg18 --postgresql-conf "shared_preload_libraries='pg_walrus'"

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

### Core Parameters

| Parameter | Default | Description |
|-----------|---------|-------------|
| `walrus.enable` | `true` | Enable/disable automatic resizing |
| `walrus.max` | `4GB` | Maximum allowed `max_wal_size` |
| `walrus.threshold` | `2` | Forced checkpoints before resize |

### Auto-Shrink Parameters

| Parameter | Default | Description |
|-----------|---------|-------------|
| `walrus.shrink_enable` | `true` | Enable/disable automatic shrinking |
| `walrus.shrink_factor` | `0.75` | Multiplier for shrink calculation (0.01-0.99) |
| `walrus.shrink_intervals` | `5` | Quiet intervals before shrinking (1-1000) |
| `walrus.min_size` | `1GB` | Minimum floor for `max_wal_size` |

### History Parameters

| Parameter | Default | Description |
|-----------|---------|-------------|
| `walrus.history_retention_days` | `7` | Days to retain history records (0-3650) |

All parameters require `SIGHUP` to take effect (no restart needed).

### Database Connection

| Parameter | Default | Description |
|-----------|---------|-------------|
| `walrus.database` | `postgres` | Database where history table is stored (requires restart) |

**Note**: `walrus.database` has `postmaster` context and requires a PostgreSQL restart to change.

## History Table

The `walrus.history` table records all sizing decisions made by pg_walrus. The table is created in the `walrus` schema when you run `CREATE EXTENSION pg_walrus`.

### Schema

```sql
walrus.history (
    id BIGSERIAL PRIMARY KEY,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT now(),
    action TEXT NOT NULL,           -- 'increase', 'decrease', or 'capped'
    old_size_mb INTEGER NOT NULL,
    new_size_mb INTEGER NOT NULL,
    forced_checkpoints BIGINT NOT NULL,
    checkpoint_timeout_sec INTEGER NOT NULL,
    reason TEXT,
    metadata JSONB                  -- Action-specific details
)
```

### Querying History

```sql
-- Recent sizing decisions
SELECT timestamp, action, old_size_mb, new_size_mb, reason
FROM walrus.history
ORDER BY timestamp DESC
LIMIT 10;

-- Summary by action type
SELECT action, count(*), avg(new_size_mb - old_size_mb)::int AS avg_change
FROM walrus.history
GROUP BY action;

-- Export for compliance audit
COPY (SELECT * FROM walrus.history WHERE timestamp >= '2025-01-01' ORDER BY timestamp)
TO '/tmp/walrus_audit.csv' WITH CSV HEADER;
```

### Automatic Cleanup

Old history records are automatically deleted based on `walrus.history_retention_days`. You can also manually trigger cleanup:

```sql
-- Delete records older than retention period
SELECT walrus.cleanup_history();
-- Returns: number of deleted records
```

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
- PostgreSQL 18

Requires PostgreSQL 15+ due to `pgstat_fetch_stat_checkpointer()` API.

## Development

### Running Tests

pg_walrus uses pgrx-managed PostgreSQL instances for development and testing. These are separate from any system PostgreSQL installations.

```bash
# Integration tests (automatically configures shared_preload_libraries via pg_test module)
cargo pgrx test pg18

# SQL regression tests (requires explicit --postgresql-conf)
cargo pgrx regress pg18 --postgresql-conf "shared_preload_libraries='pg_walrus'"

# Test all supported versions
for v in pg15 pg16 pg17 pg18; do
    cargo pgrx test $v || exit 1
    cargo pgrx regress $v --postgresql-conf "shared_preload_libraries='pg_walrus'" || exit 1
done
```

**Note**: `cargo pgrx test` reads `shared_preload_libraries` from the `pg_test::postgresql_conf_options()` function in `src/lib.rs`. `cargo pgrx regress` does not—you must pass `--postgresql-conf` explicitly for background worker extensions.
