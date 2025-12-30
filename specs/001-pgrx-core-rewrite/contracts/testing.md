# Testing Contract: pg_walrus

**Date**: 2025-12-29
**Feature**: 001-pgrx-core-rewrite
**Reference**: pgrx testing framework from `/Users/brandon/src/pgrx/`

## Overview

pg_walrus uses three complementary testing approaches:

| Test Type | Framework | Use Case | Execution |
|-----------|-----------|----------|-----------|
| `#[pg_test]` | pgrx-tests | Tests requiring PostgreSQL (SPI, GUCs, worker visibility) | `cargo pgrx test pgXX` |
| `#[test]` | Rust standard | Pure Rust logic (calculations, overflow handling) | `cargo test` |
| pg_regress | PostgreSQL | SQL-based verification (GUC syntax, extension loading) | `cargo pgrx regress pgXX` |

## Test Categories

### 1. PostgreSQL Integration Tests (`#[pg_test]`)

Tests that require a running PostgreSQL instance. Each test:
- Runs inside a PostgreSQL transaction
- Has access to SPI, GUCs, and system catalogs
- Automatically rolls back on completion (test isolation)
- Can verify background worker visibility via `pg_stat_activity`

**When to use**:
- Verifying GUC parameter registration and defaults
- Testing SQL-callable functions
- Checking background worker in `pg_stat_activity`
- Any test requiring database context

**Module organization**:

```rust
#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use pgrx::prelude::*;

    #[pg_test]
    fn test_guc_default() {
        let result = Spi::get_one::<&str>("SHOW walrus.enable")
            .expect("query failed");
        assert_eq!(result, Some("on"));
    }

    #[pg_test(error = "invalid value for parameter")]
    fn test_guc_invalid_value() -> Result<(), spi::Error> {
        Spi::run("SET walrus.threshold = -1")
    }
}
```

**Expected error pattern**:

```rust
#[pg_test(error = "syntax error")]
fn test_expected_sql_error() -> Result<(), spi::Error> {
    Spi::run("THIS IS NOT VALID SQL")
}
```

### 2. Pure Rust Unit Tests (`#[test]`)

Tests for pure Rust logic that does not require PostgreSQL.

**When to use**:
- Mathematical calculations (new_size = current * (delta + 1))
- Overflow handling (saturating arithmetic)
- String formatting
- Data structure validation

**Example**:

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_new_size_calculation() {
        // current_size * (delta + 1)
        assert_eq!(calculate_new_size(1024, 3), 4096);
        assert_eq!(calculate_new_size(2048, 1), 4096);
    }

    #[test]
    fn test_overflow_protection() {
        // i32::MAX / 2 * 3 would overflow, but saturating_mul caps it
        let result = calculate_new_size(i32::MAX / 2, 2);
        assert_eq!(result, i32::MAX);
    }
}
```

### 3. SQL Regression Tests (pg_regress)

SQL-based tests using PostgreSQL's native `pg_regress` framework.

**When to use**:
- Verifying extension SQL interface
- Testing GUC parameter behavior via SQL commands
- End-to-end scenarios that are easier to express in SQL
- Testing output format and error messages

**Directory structure**:

```text
tests/pg_regress/
├── sql/               # Test input scripts
│   ├── setup.sql      # Runs first when DB is created (special)
│   ├── guc_params.sql # GUC parameter tests
│   └── extension.sql  # Extension behavior tests
├── expected/          # Expected output files
│   ├── setup.out
│   ├── guc_params.out
│   └── extension.out
└── results/           # Generated during test run (gitignored)
```

**setup.sql** (required):

```sql
-- This file runs once when the regression database is created
-- Must create the extension before other tests
CREATE EXTENSION pg_walrus;
```

**Test file pattern**:

```sql
-- tests/pg_regress/sql/guc_params.sql

-- Test default values
SHOW walrus.enable;
SHOW walrus.max;
SHOW walrus.threshold;

-- Test setting valid values
SET walrus.enable = false;
SHOW walrus.enable;

SET walrus.threshold = 5;
SHOW walrus.threshold;

-- Test boundary conditions
SET walrus.threshold = 1;    -- minimum
SET walrus.threshold = 1000; -- maximum
```

## Test Infrastructure

### Background Worker Testing Configuration

For testing extensions with background workers, pgrx-tests supports `shared_preload_libraries` via the `pg_test` module:

```rust
#[cfg(test)]
pub mod pg_test {
    /// Called once at test framework initialization
    pub fn setup(_options: Vec<&str>) {
        // Optional: one-time setup code
    }

    /// PostgreSQL configuration for tests
    pub fn postgresql_conf_options() -> Vec<&'static str> {
        vec!["shared_preload_libraries='pg_walrus'"]
    }
}
```

### Test Execution Order (pgrx-tests)

```text
1. pgrx-tests writes postgresql.auto.conf with shared_preload_libraries='pg_walrus'
2. PostgreSQL starts with pg_walrus loaded
3. _PG_init() called → GUCs registered, bgworker registered
4. Recovery finishes → bgworker spawns
5. CREATE EXTENSION pg_walrus → SQL objects created
6. #[pg_test] tests run → can query pg_stat_activity
7. Each test wraps in transaction → rolls back after
8. PostgreSQL stopped after tests complete
```

### Test Execution Order (pg_regress)

```text
1. cargo pgrx regress starts PostgreSQL instance
2. Creates database: pg_walrus_regress
3. Runs setup.sql (creates extension)
4. Runs test files in alphabetical order
5. Compares output to expected/*.out files
6. Reports pass/fail based on diff
```

## Test Commands

### Running Tests

```bash
# Run all pgrx tests for PostgreSQL 17
cargo pgrx test pg17

# Run specific pgrx test
cargo pgrx test pg17 test_guc_default

# Run pure Rust tests only
cargo test --lib

# Run pg_regress tests for PostgreSQL 17
cargo pgrx regress pg17

# Run specific pg_regress test
cargo pgrx regress pg17 guc_params

# Run pg_regress with auto-accept new output
cargo pgrx regress pg17 --auto

# Reset pg_regress database
cargo pgrx regress pg17 --resetdb

# Run pg_regress with custom PostgreSQL config
cargo pgrx regress pg17 --postgresql-conf shared_preload_libraries=pg_walrus
```

### Multi-Version Testing

```bash
# Test all supported PostgreSQL versions
cargo pgrx test pg15
cargo pgrx test pg16
cargo pgrx test pg17
cargo pgrx test pg18

# pg_regress all versions
cargo pgrx regress pg15
cargo pgrx regress pg16
cargo pgrx regress pg17
cargo pgrx regress pg18
```

### Environment Variables

| Variable | Purpose | Example |
|----------|---------|---------|
| `CARGO_PGRX_TEST_PGDATA` | Custom PGDATA for tests | `/tmp/pgrx-test-data` |
| `CARGO_PGRX_TEST_RUNAS` | Run PostgreSQL as user | `postgres` |
| `PGRX_TEST_SKIP` | Skip all tests | `1` |
| `RUST_BACKTRACE` | Enable backtraces | `1` |

## pg_walrus Test Requirements

### GUC Parameter Tests (Constitution VIII)

| Test | Type | Verification |
|------|------|--------------|
| walrus.enable default | `#[pg_test]` | `SHOW walrus.enable` returns `'on'` |
| walrus.max default | `#[pg_test]` | `SHOW walrus.max` returns `'4096'` |
| walrus.threshold default | `#[pg_test]` | `SHOW walrus.threshold` returns `'2'` |
| walrus.enable toggle | pg_regress | `SET walrus.enable = false` works |
| walrus.threshold range | pg_regress | Valid 1-1000, error outside range |

### Background Worker Tests (Constitution VIII)

| Test | Type | Verification |
|------|------|--------------|
| Worker visibility | `#[pg_test]` | `pg_stat_activity.backend_type = 'pg_walrus'` |
| Worker running after recovery | `#[pg_test]` | EXISTS query returns true |

### Pure Rust Tests (Constitution VIII)

| Test | Type | Verification |
|------|------|--------------|
| Size calculation formula | `#[test]` | `current * (delta + 1)` |
| Overflow protection | `#[test]` | `saturating_mul` caps at `i32::MAX` |

### pg_regress Tests

| Test File | Purpose |
|-----------|---------|
| `setup.sql` | Create extension, verify loads |
| `guc_params.sql` | GUC defaults, valid ranges, invalid values |
| `extension_info.sql` | Extension metadata, version |

## Test File Naming Conventions

### pgrx Tests

- Test functions: `test_[feature]_[scenario]`
- Examples: `test_guc_default`, `test_background_worker_running`

### pg_regress Tests

- SQL files: `[feature].sql`
- Expected output: `[feature].out`
- Examples: `guc_params.sql`, `extension_info.sql`
- Special: `setup.sql` (runs first on DB creation)

## Test Isolation and Cleanup

### #[pg_test] Isolation

Each `#[pg_test]` function runs in a transaction that automatically rolls back:

```rust
#[pg_test]
fn test_create_temp_table() {
    Spi::run("CREATE TABLE test_table (id int)").unwrap();
    // Table exists within this test
    let count = Spi::get_one::<i64>("SELECT count(*) FROM test_table").unwrap();
    assert_eq!(count, Some(0));
    // Table automatically cleaned up when test ends (rollback)
}
```

### pg_regress Cleanup

pg_regress tests persist changes. Each test should either:
1. Use `DROP IF EXISTS ... ; CREATE ...` pattern
2. Clean up at end: `DROP TABLE test_table;`
3. Use `--resetdb` flag to start fresh

**Example resilient test**:

```sql
-- Clean start
DROP TABLE IF EXISTS walrus_test;

-- Test code
CREATE TABLE walrus_test (id int);
INSERT INTO walrus_test VALUES (1);
SELECT * FROM walrus_test;

-- Clean finish
DROP TABLE walrus_test;
```

## Creating New pg_regress Tests

1. Create SQL test file:
   ```bash
   echo "SHOW walrus.enable;" > tests/pg_regress/sql/new_test.sql
   ```

2. Run regress to generate expected output:
   ```bash
   cargo pgrx regress pg17
   ```

3. Accept the output when prompted (or use `--auto`)

4. Verify test passes:
   ```bash
   cargo pgrx regress pg17 new_test
   ```

## Debugging Test Failures

### pgrx Test Failures

1. Check PostgreSQL logs in output
2. Run with backtrace: `RUST_BACKTRACE=1 cargo pgrx test pg17`
3. Run specific test: `cargo pgrx test pg17 test_name`

### pg_regress Failures

1. Check `tests/pg_regress/regression.diffs` for diff output
2. Check `tests/pg_regress/results/test_name.out` for actual output
3. Compare with `tests/pg_regress/expected/test_name.out`

## Constitution Compliance

### VIII. Test Discipline

> Tests must be present. For any feature:
> - Core functionality requires integration tests
> - Edge cases require unit tests
> - Complex flows may require pg_regress tests

**pg_walrus compliance**:

| Requirement | Test Type | Task Reference |
|-------------|-----------|----------------|
| GUC registration | `#[pg_test]` | T037, T038, T039 |
| Background worker | `#[pg_test]` | T040, T041 |
| Size calculation | `#[test]` | T042 |
| Overflow handling | `#[test]` | T043 |
| GUC SQL interface | pg_regress | T044, T045 |
| Extension loading | pg_regress | T046 |

## Constraints

1. **No test deferral**: All specified tests must be implemented in the current phase
2. **No test weakening**: When tests fail, fix the implementation, not the test
3. **Full coverage**: Edge cases in spec.md require corresponding tests
4. **Multi-version**: Tests must pass on PostgreSQL 15, 16, 17, and 18
5. **Clean isolation**: Tests must not depend on state from other tests
