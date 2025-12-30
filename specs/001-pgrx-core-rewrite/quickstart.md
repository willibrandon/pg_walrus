# Quickstart: pg_walrus

**Date**: 2025-12-29
**Feature**: 001-pgrx-core-rewrite

## Prerequisites

- PostgreSQL 15, 16, 17, or 18
- Rust 1.83+ (for building from source)
- cargo-pgrx 0.16.1+ installed (`cargo install cargo-pgrx`)

## Building from Source

```bash
# Clone the repository
git clone https://github.com/your-org/pg_walrus.git
cd pg_walrus

# Build for your PostgreSQL version
cargo pgrx build --features pg17 --release

# Package the extension
cargo pgrx package --pg-config $(which pg_config)
```

## Installation

### Option 1: Using cargo-pgrx (Development)

```bash
# Install directly to PostgreSQL
cargo pgrx install --features pg17
```

### Option 2: Manual Installation (Production)

```bash
# Copy extension files to PostgreSQL
cp target/release/pg_walrus-pg17/usr/share/postgresql/17/extension/* \
   $(pg_config --sharedir)/extension/

cp target/release/pg_walrus-pg17/usr/lib/postgresql/17/lib/* \
   $(pg_config --pkglibdir)/
```

## Configuration

### 1. Enable Shared Preload

Edit `postgresql.conf`:

```ini
shared_preload_libraries = 'pg_walrus'
```

Or via SQL (requires restart):

```sql
ALTER SYSTEM SET shared_preload_libraries = 'pg_walrus';
```

### 2. Restart PostgreSQL

```bash
pg_ctl restart -D $PGDATA
```

### 3. Verify Installation

```sql
-- Check extension is loaded
SELECT * FROM pg_stat_activity WHERE backend_type = 'pg_walrus';

-- View configuration
SELECT name, setting, unit FROM pg_settings WHERE name LIKE 'walrus.%';
```

## Basic Usage

### Default Behavior

With default settings, pg_walrus:
- Monitors checkpoint activity every `checkpoint_timeout` (default: 5 minutes)
- Increases `max_wal_size` when 2+ forced checkpoints occur
- Caps growth at 4GB

### Customizing Behavior

```sql
-- Set maximum to 16GB
ALTER SYSTEM SET walrus.max = '16GB';

-- Only resize after 5 forced checkpoints
ALTER SYSTEM SET walrus.threshold = 5;

-- Apply changes
SELECT pg_reload_conf();
```

### Temporarily Disabling

```sql
-- Disable automatic sizing
ALTER SYSTEM SET walrus.enable = false;
SELECT pg_reload_conf();

-- Re-enable
ALTER SYSTEM SET walrus.enable = true;
SELECT pg_reload_conf();
```

## Monitoring

### Check Current Status

```sql
-- Current max_wal_size
SHOW max_wal_size;

-- pg_walrus configuration
SELECT name, setting, unit
FROM pg_settings
WHERE name LIKE 'walrus.%';

-- Background worker status
SELECT pid, backend_type, state
FROM pg_stat_activity
WHERE backend_type = 'pg_walrus';
```

### Watch Resize Events

```bash
# Follow PostgreSQL logs for pg_walrus messages
tail -f $PGDATA/log/postgresql-*.log | grep -i walrus
```

Expected log messages:
```
LOG:  pg_walrus worker started
LOG:  pg_walrus: detected 3 forced checkpoints over 300 seconds
LOG:  pg_walrus: resizing max_wal_size from 1024 MB to 4096 MB
```

## Testing Locally

### Generate WAL Activity

```sql
-- Create test table
CREATE TABLE wal_test (id serial, data text);

-- Generate WAL (adjust iterations based on your max_wal_size)
INSERT INTO wal_test (data)
SELECT repeat('x', 1000)
FROM generate_series(1, 1000000);
```

### Force Checkpoints (for testing)

```sql
-- Force a checkpoint
CHECKPOINT;

-- Check checkpoint statistics
SELECT * FROM pg_stat_checkpointer;
```

## Troubleshooting

### Extension Not Loading

```sql
-- Verify shared_preload_libraries
SHOW shared_preload_libraries;
-- Should include: pg_walrus
```

If missing, check:
1. `postgresql.conf` has correct entry
2. PostgreSQL was restarted (not just reloaded)
3. Extension files are in correct directory

### Worker Not Running

```sql
SELECT * FROM pg_stat_activity WHERE backend_type = 'pg_walrus';
```

If no rows:
1. Check PostgreSQL logs for startup errors
2. Verify extension is in `shared_preload_libraries`
3. Confirm PostgreSQL is running as primary (not standby)

### max_wal_size Not Changing

Check logs and verify:
1. `walrus.enable = true`
2. Forced checkpoints exceed `walrus.threshold`
3. Current `max_wal_size` is below `walrus.max`

```sql
-- View recent checkpoint activity
SELECT * FROM pg_stat_checkpointer;

-- Check if at maximum
SELECT setting::int = (SELECT setting::int FROM pg_settings WHERE name = 'walrus.max')
FROM pg_settings WHERE name = 'max_wal_size';
```

## Uninstalling

```sql
-- Remove from shared_preload_libraries
ALTER SYSTEM RESET shared_preload_libraries;
-- Then restart PostgreSQL

-- Remove extension files (optional)
rm $(pg_config --sharedir)/extension/pg_walrus*
rm $(pg_config --pkglibdir)/pg_walrus*
```

## Technical Notes

### CheckPointTimeout Access

pg_walrus uses PostgreSQL's `checkpoint_timeout` GUC to determine the background worker's wake interval. This variable is accessed via an extern C declaration rather than through pgrx's standard `pg_sys` bindings.

**Why?** pgrx's bindgen does not include `postmaster/bgwriter.h` in its header list, so `pg_sys::CheckPointTimeout` is not available. The variable is declared directly:

```rust
extern "C" {
    static CheckPointTimeout: std::ffi::c_int;
}
```

This is safe because PostgreSQL exports `CheckPointTimeout` with `PGDLLIMPORT` and guarantees it is initialized before extension code runs. See `research.md R8` for full details.
