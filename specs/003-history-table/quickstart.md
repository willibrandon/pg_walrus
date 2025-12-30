# Quickstart: History Table Feature

**Feature**: 003-history-table
**Date**: 2025-12-30

## Prerequisites

- pg_walrus extension installed and loaded via `shared_preload_libraries`
- PostgreSQL 15, 16, 17, or 18
- Rust 1.83+ with pgrx 0.16.1

## Build & Test

```bash
# Build and run tests for PostgreSQL 18
cargo pgrx test pg18

# Run pg_regress SQL tests
cargo pgrx regress pg18 --postgresql-conf "shared_preload_libraries='pg_walrus'"

# Run on all supported versions
for v in pg15 pg16 pg17 pg18; do
    cargo pgrx test $v || exit 1
done
```

## Development Setup

```bash
# Start interactive PostgreSQL session
cargo pgrx run pg18

# Inside psql:
CREATE EXTENSION pg_walrus;

# Verify history table exists
\d walrus.history

# Check GUC is registered
SHOW walrus.history_retention_days;
```

## Key Files

| File | Purpose |
|------|---------|
| `src/lib.rs` | Extension entry point, extension_sql! for schema/table |
| `src/guc.rs` | Add WALRUS_HISTORY_RETENTION_DAYS |
| `src/history.rs` | NEW: History insert and cleanup functions |
| `src/worker.rs` | Integrate history logging into worker loop |
| `tests/pg_regress/sql/history.sql` | SQL interface tests |
| `tests/pg_regress/sql/cleanup.sql` | Cleanup function tests |

## Implementation Checklist

1. **Schema & Table Creation**
   - [ ] Add `extension_sql!` block to `lib.rs` with bootstrap positioning
   - [ ] Create `walrus` schema
   - [ ] Create `walrus.history` table with all columns
   - [ ] Create timestamp index

2. **GUC Registration**
   - [ ] Add `WALRUS_HISTORY_RETENTION_DAYS` static to `guc.rs`
   - [ ] Register in `register_gucs()` with range 0-3650

3. **History Module**
   - [ ] Create `src/history.rs`
   - [ ] Implement `insert_history_record()` with SPI
   - [ ] Implement `cleanup_old_history()` with SPI
   - [ ] Add module declaration to `lib.rs`

4. **SQL Function**
   - [ ] Create `walrus` schema module with `#[pg_schema]`
   - [ ] Implement `cleanup_history()` as `#[pg_extern]`

5. **Worker Integration**
   - [ ] Import history module in `worker.rs`
   - [ ] Call `insert_history_record()` after resize (grow path)
   - [ ] Call `insert_history_record()` after shrink (shrink path)
   - [ ] Call `cleanup_old_history()` at end of monitoring cycle

6. **Tests**
   - [ ] `#[pg_test]` for table existence
   - [ ] `#[pg_test]` for GUC default value
   - [ ] `#[pg_test]` for insert and query
   - [ ] `#[pg_test]` for cleanup function
   - [ ] pg_regress for SQL interface
   - [ ] Multi-version test pass (pg15, pg16, pg17, pg18)

## Verification Commands

```sql
-- After implementation, verify:

-- 1. Table exists
SELECT * FROM walrus.history LIMIT 1;

-- 2. GUC is accessible
SHOW walrus.history_retention_days;

-- 3. Cleanup function works
SELECT walrus.cleanup_history();

-- 4. Index exists
SELECT indexname FROM pg_indexes WHERE tablename = 'history' AND schemaname = 'walrus';

-- 5. Worker is logging (wait for resize event or trigger manually)
SELECT count(*) FROM walrus.history;
```

## Common Issues

**"relation walrus.history does not exist"**
- Ensure `CREATE EXTENSION pg_walrus;` was run
- Check extension_sql! has `bootstrap` positioning

**"function walrus.cleanup_history() does not exist"**
- Ensure `#[pg_schema] mod walrus` is defined
- Verify function has `#[pg_extern]` attribute

**"permission denied for schema walrus"**
- Run as superuser or grant permissions on walrus schema

**Tests fail with "shared_preload_libraries" error**
- Ensure `pg_test` module has `postgresql_conf_options()` returning the library

## Next Steps

After implementation:
1. Run `/speckit.tasks` to generate tasks.md
2. Implement tasks in order
3. Run full test suite
4. Create PR
