# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

pg_walrus is a Rust rewrite (using pgrx) of pg_walsizer - a PostgreSQL extension that automatically monitors and adjusts `max_wal_size` to prevent performance-degrading forced checkpoints. The name comes from WAL + Rust = Walrus.

**Current state**: The Rust implementation is complete with 132 tests passing across PostgreSQL 15-18.

## No Regression Policy

**ABSOLUTE PROHIBITION:**
- Recommending older Rust editions (e.g., "use edition 2021 instead of 2024")
- Recommending older language/framework versions to avoid compatibility issues
- Downgrading dependencies to avoid fixing code
- Any suggestion to use older tooling when newer versions have stricter requirements

**REQUIRED behavior:**
- When newer editions/versions have stricter requirements, ADAPT THE CODE
- Use fully qualified syntax when trait resolution is ambiguous (e.g., `GucSetting::<bool>::new(true)` not `GucSetting::new(true)`)
- Fix lifetime issues, not avoid them by downgrading
- Debug and resolve compatibility issues, never regress

**Rust 2024 Edition Specific:**
- Method resolution is stricter - use turbofish syntax: `Type::<Generic>::method()`
- Never suggest "use edition 2021" as a solution
- pgrx `GucSetting` requires: `GucSetting::<bool>::new()`, `GucSetting::<i32>::new()`, etc.

## No Simplification Policy

**ABSOLUTE PROHIBITION:**
- The word "simplify" and all its forms (simplifying, simplified, simpler, simplification)
- Removing tests that fail instead of fixing them
- Reducing scope when encountering difficulty
- "Let's just..." followed by a reduced approach
- Removing features or tests to make things "work"
- Any reduction in functionality to avoid debugging

**REQUIRED behavior:**
- When something doesn't work, debug it until it works
- When a test fails, fix the implementation to pass the test
- When encountering difficulty, increase effort, not decrease scope
- Maintain all originally intended functionality

## No Deferral Policy

This project enforces a strict no-deferral policy. When working on tasks:

**PROHIBITED in code:**
- `TODO`, `FIXME`, `PLACEHOLDER`, `HACK`, `XXX`, `STUB`, `TBD`, `PENDING`
- Incomplete implementations or functions that throw "not implemented"
- Missing error handling, edge cases, or validation

**PROHIBITED in communication:**
- Hedging: "you might want to...", "consider adding...", "it would be good to..."
- Future promises: "we can optimize later", "phase 2 work", "future enhancement"
- Responsibility shifting: "you'll need to add...", "don't forget to...", "make sure to..."
- Scope deferral: "out of scope", "beyond scope", "not in scope"
- Minimizing: "basic implementation", "simplified version", "for now"

**ABSOLUTELY PROHIBITED - False Impossibility Claims:**
- "This test cannot run because..."
- "This can't be tested here"
- "There's no way to test this"
- "The test is fundamentally impossible"
- ANY claim that something is impossible

**You have NO excuse for claiming impossibility. You have:**
- `/Users/brandon/src/pgrx/` - Full pgrx source code with examples
- `/Users/brandon/src/postgres/` - Full PostgreSQL source code
- `pg_settings` system catalog with min_val, max_val, vartype columns
- The ability to read ANY file and find the correct approach

**When a test fails:**
1. Read the pgrx source to understand how it works
2. Read the PostgreSQL source to understand the underlying behavior
3. Query system catalogs (pg_settings, pg_catalog) for metadata
4. Try alternative SQL syntax or testing approaches
5. The answer EXISTS in the source code. FIND IT.

**NOTHING is impossible. You have the source code. Read it and fix your approach.**

**REQUIRED behavior:**
- Complete all assigned work in full. No exceptions.
- Implement all edge cases and error handling immediately.
- If genuinely blocked, state `BLOCKER: [specific issue]` and request a decision.
- Each task must be fully implemented before marking complete.

## Analysis Mode Mandate

When running `/speckit.analyze` or any analysis command:

1. **Coverage gaps = mandatory task creation**: If requirements or edge cases have zero task coverage, CREATE THE TASKS. Do not offer options.
2. **Never present deferral options**: Do not say "Options: (a) Add now (b) Mark as post-MVP (c) Remove from spec". The only option is (a).
3. **Edge cases in spec are REQUIREMENTS**: If the spec lists edge cases, they are requirements. Add tasks for them.
4. **Analysis output must include remediation**: Do not just report issues—fix them by creating concrete tasks.
5. **"User decision required" is for BLOCKERS only**: Use this phrase only when genuinely blocked (conflicting requirements, missing external info). Never use it to defer edge case coverage.
6. **Constitution requirements are non-negotiable**: If the constitution mandates something (e.g., tests), add tasks. Do not offer "complexity justification" as an escape hatch.

**Prohibited Analysis Patterns:**
- "User may proceed without changes"
- "Options: ... (b) Mark as post-MVP ... (c) Remove from spec"
- "Edge case handling will need to be added in future iterations"
- "Add complexity justification to waive requirement"
- "Choose one: (a) add tests (b) add justification"

**Required Analysis Patterns:**
- "Coverage gap detected. Adding tasks: T041, T042, T043..."
- "Edge cases require implementation. Creating tasks now."
- "BLOCKER: [specific conflicting requirement]. Need decision: [specific question]"

## Test Failure Protocol

When a test fails, the ONLY acceptable responses are:

**ABSOLUTE PROHIBITION:**
- "I can make the test more lenient"
- "We could relax the assertion"
- "This test is too strict"
- "Let me adjust the test expectations"
- "For now, let's just skip this test case"
- "This is flaky"
- "This is tricky"
- Any suggestion to weaken, skip, or bypass tests

**ROOT CAUSE ANALYSIS REQUIRED:**
When a test fails, you MUST:
1. Identify the exact code causing the failure
2. Trace execution to find the root cause
3. Fix the implementation, never the test
4. Re-run tests to verify the fix
5. If the test uncovers a design flaw, state `BLOCKER: [specific design issue]` and request clarification

**Test Integrity Non-Negotiable:**
- Tests define the specification. Implementation must meet tests.
- A test failure indicates a gap between specification and implementation.
- Specification gaps must be resolved through code changes, never through relaxed tests.
- Tests are the contract. The contract never changes to accommodate weak implementations.

**When Tests Fail:**
- Ask: "What is the actual vs expected behavior?"
- Ask: "Which code path causes this difference?"
- Ask: "What must change in the implementation?"
- Do NOT ask: "Should we relax this test?"

**Red Flag Phrases (NEVER USE):**
- "We can relax this"
- "Make it more lenient"
- "For now, let's accept"
- "We can skip this case"
- "This test is overly strict"
- "Weaken the assertions"
- "Adjust expectations"
- "Be more lenient"

## Git Attribution Policy

Commit messages MUST NOT contain AI assistant attribution or co-authorship claims.

**ABSOLUTE PROHIBITION:**
- `Co-Authored-By: Claude` (any variant)
- `Co-Authored-By: Claude Code` (any variant)
- `Co-Authored-By: Anthropic` (any variant)
- Any AI/LLM co-authorship attribution
- Any Claude attribution in commit messages
- Any mention of AI assistance in commit metadata
- Generated with markers (e.g., "Generated with Claude Code")
- Robot emoji indicators of AI involvement

**Commit Message Format:**
- Focus on WHAT changed and WHY
- Use conventional commit format when appropriate
- No attribution to tools or assistants
- No emoji decorations unless project style requires them

**REQUIRED:**
- Commit messages describe the change, not who/what made it
- Focus on technical content and rationale
- Follow project's existing commit message conventions

## File Size Limits

**ABSOLUTE PROHIBITION:**
- Source code files exceeding 900 lines of code (LOC)
- Adding code to a file that would push it over 900 LOC

**REQUIRED behavior:**
- When a file approaches 900 LOC, proactively split into logical modules
- Extract related functionality into separate files before hitting the limit
- Use Rust's module system to organize code (e.g., `mod submodule;`)

**Measurement:**
- Count all lines including comments and blank lines
- Use `wc -l <file>` to check

**Splitting Strategies for pg_walrus:**
- Extract pure functions into utility modules
- Split tests into dedicated test files
- Move GUC definitions to dedicated `guc.rs`
- Extract statistics access to dedicated `stats.rs`
- Separate worker logic from initialization

## Build Commands

### Quick Reference
```bash
cargo pgrx run pg18                        # Build, install, and open psql
cargo pgrx test pg18                       # Run pgrx integration tests
cargo pgrx regress pg18                    # Run pg_regress SQL tests
cargo pgrx package --pg-config /usr/bin/pg_config  # Create distribution package
```

## Comprehensive cargo pgrx Reference

### Command Overview

| Command | Purpose | Default Mode |
|---------|---------|--------------|
| `init` | Initialize pgrx development environment | N/A |
| `new` | Create new extension project | N/A |
| `run` | Build, install, and launch psql | debug |
| `test` | Run `#[pg_test]` tests | debug |
| `regress` | Run pg_regress SQL tests | debug |
| `install` | Install extension to PostgreSQL | debug |
| `package` | Create distribution package | **release** |
| `schema` | Generate SQL schema files | debug |
| `start` | Start pgrx-managed PostgreSQL | N/A |
| `stop` | Stop pgrx-managed PostgreSQL | N/A |
| `status` | Check PostgreSQL instance status | N/A |
| `connect` | Connect to database via psql | N/A |
| `info` | Query pgrx environment | N/A |
| `get` | Get property from control file | N/A |
| `upgrade` | Upgrade pgrx dependency versions | N/A |
| `cross` | Cross-compilation utilities | N/A |

### cargo pgrx init

Initialize pgrx development environment (one-time setup).

```bash
# Initialize all PostgreSQL versions (downloads and compiles)
cargo pgrx init

# Initialize specific version only
cargo pgrx init --pg18 download

# Use existing PostgreSQL installation
cargo pgrx init --pg18 /usr/local/pgsql/bin/pg_config

# Advanced: Configure PostgreSQL compilation
cargo pgrx init --pg18 download --configure-flag="--with-openssl"

# Compile with Valgrind detection
cargo pgrx init --pg18 download --valgrind

# Multi-threaded compilation
cargo pgrx init -j 8
```

**Key Options:**
- `--pg13`, `--pg14`, `--pg15`, `--pg16`, `--pg17`, `--pg18`: Specify version path or `download`
- `--base-port PORT`: Base port for managed instances (default: 28800)
- `--base-testing-port PORT`: Base port for testing instances
- `--configure-flag FLAG`: Pass flags to PostgreSQL configure (repeatable)
- `--valgrind`: Enable Valgrind detection in compiled PostgreSQL
- `-j, --jobs N`: Parallel make jobs

**Environment Variables:**
- `PG13_PG_CONFIG` through `PG18_PG_CONFIG`: Paths to pg_config for each version
- `ICU_CFLAGS`, `ICU_LIBS`: Custom ICU library configuration
- `PKG_CONFIG_PATH`: Package config search path

### cargo pgrx new

Create a new extension project scaffold.

```bash
# Create new extension
cargo pgrx new my_extension

# Create extension with background worker template
cargo pgrx new my_extension --bgworker
```

**Generated Files:**
```
my_extension/
├── .cargo/config.toml     # Cargo build configuration
├── Cargo.toml             # Extension manifest
├── src/
│   ├── lib.rs             # Main extension code
│   └── bin/pgrx_embed.rs  # SQL generation binary
├── my_extension.control   # PostgreSQL control file
├── .gitignore
└── tests/pg_regress/
    ├── sql/setup.sql      # pg_regress setup
    └── expected/setup.out # Expected output
```

**Name Validation:** Extension name must match `[a-z0-9_]` pattern (lowercase alphanumeric and underscore only).

### cargo pgrx run

Build, install, and open interactive psql session.

```bash
# Run with default settings
cargo pgrx run pg18

# Run in release mode
cargo pgrx run pg18 --release

# Connect to specific database
cargo pgrx run pg18 my_database

# Use pgcli instead of psql
cargo pgrx run pg18 --pgcli

# Build and install only (don't launch psql)
cargo pgrx run pg18 --install-only

# Run under Valgrind
cargo pgrx run pg18 --valgrind
```

**Key Options:**
- `-r, --release`: Compile in release mode
- `--profile PROFILE`: Use named Cargo profile
- `--pgcli`: Use pgcli instead of psql
- `--install-only`: Install without launching psql
- `--valgrind`: Start PostgreSQL under Valgrind
- `-F, --features FEATURES`: Enable Cargo features

**Environment Variables:**
- `PG_VERSION`: Default PostgreSQL version
- `PGRX_PGCLI`: Set to enable pgcli by default

### cargo pgrx test

Run `#[pg_test]` integration tests.

```bash
# Run all tests for PostgreSQL 17
cargo pgrx test pg18

# Run specific test by name
cargo pgrx test pg18 test_guc_default

# Run tests for all configured versions
cargo pgrx test all

# Run in release mode
cargo pgrx test pg18 --release

# Skip schema regeneration (faster iteration)
cargo pgrx test pg18 --no-schema

# Run as different system user
cargo pgrx test pg18 --runas postgres

# Use custom data directory
cargo pgrx test pg18 --pgdata /tmp/pgrx-test-data
```

**Key Options:**
- `-r, --release`: Compile in release mode
- `--profile PROFILE`: Use named Cargo profile
- `-n, --no-schema`: Skip schema regeneration
- `--runas USER`: Run as specific system user (requires sudo)
- `--pgdata DIR`: Custom PostgreSQL data directory
- `-F, --features FEATURES`: Enable Cargo features
- `--no-default-features`: Disable default features
- `--all-features`: Enable all features

**Environment Variables:**
- `PG_VERSION`: Default PostgreSQL version
- `PGRX_BUILD_PROFILE`: Build profile name
- `PGRX_FEATURES`: Features to enable
- `PGRX_NO_DEFAULT_FEATURES`: Disable default features
- `PGRX_ALL_FEATURES`: Enable all features
- `PGRX_NO_SCHEMA`: Skip schema generation
- `CARGO_PGRX_TEST_RUNAS`: User to run tests as
- `CARGO_PGRX_TEST_PGDATA`: Custom data directory
- `RUST_LOG`: Logging level (debug, trace, info, warn)
- `PGRX_TEST_SKIP`: Skip all tests if set

### cargo pgrx regress

Run pg_regress SQL-based tests.

```bash
# Run all pg_regress tests (requires shared_preload_libraries for background worker extensions)
cargo pgrx regress pg18 --postgresql-conf "shared_preload_libraries='pg_walrus'"

# Run specific test file
cargo pgrx regress pg18 --postgresql-conf "shared_preload_libraries='pg_walrus'" guc_params

# Auto-accept new/changed test output
cargo pgrx regress pg18 --postgresql-conf "shared_preload_libraries='pg_walrus'" --auto

# Reset database before testing
cargo pgrx regress pg18 --postgresql-conf "shared_preload_libraries='pg_walrus'" --resetdb

# Use custom database name
cargo pgrx regress pg18 --dbname my_test_db

# Add PostgreSQL configuration
cargo pgrx regress pg18 --postgresql-conf log_min_messages=debug1

# Run in release mode
cargo pgrx regress pg18 --release
```

**Important for background worker extensions**: Unlike `cargo pgrx test` (which reads `postgresql_conf_options()` from the `pg_test` module), `cargo pgrx regress` does NOT automatically configure `shared_preload_libraries`. You must pass it explicitly via `--postgresql-conf` for extensions that register background workers.

**Key Options:**
- `-a, --auto`: Auto-accept new test output AND overwrite failed test output
- `--resetdb`: Recreate test database before running
- `--dbname NAME`: Override generated database name
- `--postgresql-conf KEY=VALUE`: Custom postgresql.conf settings (repeatable)
- `-r, --release`: Compile in release mode
- `-n, --no-schema`: Skip schema regeneration
- `--runas USER`: Run as specific system user

**Test File Structure:**
```
tests/pg_regress/
├── sql/
│   ├── setup.sql           # SPECIAL: Always runs first
│   ├── test_guc.sql        # Test files (alphabetical order)
│   └── test_worker.sql
├── expected/
│   ├── setup.out           # Expected output for setup.sql
│   ├── test_guc.out        # Expected output for test_guc.sql
│   └── test_worker.out
└── results/                # Generated during tests (gitignored)
```

**Important:** `setup.sql` is special - it always runs first regardless of alphabetical order.

### cargo pgrx install

Install extension to PostgreSQL instance.

```bash
# Install to default PostgreSQL (first in PATH)
cargo pgrx install

# Install to specific PostgreSQL
cargo pgrx install --pg-config /usr/local/pgsql/bin/pg_config

# Install in release mode
cargo pgrx install --release

# Install with sudo (for system PostgreSQL)
cargo pgrx install --sudo

# Build in test mode (includes pg_test feature)
cargo pgrx install --test
```

**Key Options:**
- `-c, --pg-config PATH`: Path to pg_config (default: first in $PATH)
- `-s, --sudo`: Use sudo for installation
- `-r, --release`: Compile in release mode
- `--test`: Build in test mode
- `--profile PROFILE`: Use named Cargo profile
- `-F, --features FEATURES`: Enable Cargo features

### cargo pgrx package

Create distribution package for deployment.

```bash
# Create package (defaults to release mode)
cargo pgrx package --pg-config /usr/local/pgsql/bin/pg_config

# Create debug package
cargo pgrx package --pg-config /path/to/pg_config --debug

# Custom output directory
cargo pgrx package --pg-config /path/to/pg_config --out-dir ./dist/

# Cross-compilation
cargo pgrx package --pg-config /path/to/pg_config --target x86_64-unknown-linux-gnu
```

**Key Options:**
- `-c, --pg-config PATH`: Path to pg_config (required)
- `--debug`: Compile in debug mode (default is **release**)
- `--out-dir DIR`: Output directory (default: `./target/[debug|release]/extname-pgXX/`)
- `--target TARGET`: Cross-compilation target

**Output Structure:**
```
target/release/pg_walrus-pg18/
├── usr/share/postgresql/17/extension/
│   ├── pg_walrus.control
│   └── pg_walrus--0.1.0.sql
└── usr/lib/postgresql/17/lib/
    └── pg_walrus.so
```

### cargo pgrx schema

Generate SQL schema files from extension source.

```bash
# Generate schema to stdout
cargo pgrx schema pg18

# Generate schema to file
cargo pgrx schema pg18 --out schema.sql

# Generate GraphViz DOT diagram
cargo pgrx schema pg18 --dot schema.dot

# Skip rebuild (use existing .so)
cargo pgrx schema pg18 --skip-build

# Generate in release mode
cargo pgrx schema pg18 --release
```

**Key Options:**
- `-o, --out FILE`: Output SQL file (default: stdout)
- `-d, --dot FILE`: Output GraphViz DOT file
- `--skip-build`: Don't rebuild extension
- `-r, --release`: Compile in release mode
- `-n, --no-schema`: Don't regenerate schema (use cached)

### cargo pgrx start / stop / status

Manage pgrx-managed PostgreSQL instances.

```bash
# Start PostgreSQL 17
cargo pgrx start pg18

# Start with custom configuration
cargo pgrx start pg18 --postgresql-conf log_statement=all

# Start under Valgrind
cargo pgrx start pg18 --valgrind

# Stop PostgreSQL 17
cargo pgrx stop pg18

# Stop all instances
cargo pgrx stop all

# Check status
cargo pgrx status pg18
cargo pgrx status all
```

**start Options:**
- `--postgresql-conf KEY=VALUE`: Runtime configuration (repeatable)
- `--valgrind`: Start under Valgrind

### cargo pgrx connect

Connect to database without building/installing extension.

```bash
# Connect to default database
cargo pgrx connect pg18

# Connect to specific database
cargo pgrx connect pg18 my_database

# Use pgcli instead of psql
cargo pgrx connect pg18 --pgcli
```

### cargo pgrx info

Query pgrx development environment.

```bash
# Get PostgreSQL installation path
cargo pgrx info path pg18
# Output: ~/.pgrx/17.2/pgrx-install

# Get pg_config path
cargo pgrx info pg-config pg18
# Output: ~/.pgrx/17.2/pgrx-install/bin/pg_config

# Get specific version number
cargo pgrx info version pg18
# Output: 17.2
```

### cargo pgrx get

Extract properties from extension control file.

```bash
# Get extension name
cargo pgrx get extname
# Output: pg_walrus

# Get git hash (for versioning)
cargo pgrx get git_hash
# Output: abc1234...

# Get any control file property
cargo pgrx get default_version
cargo pgrx get comment
```

### cargo pgrx upgrade

Upgrade pgrx dependency versions in Cargo.toml.

```bash
# Upgrade to latest stable
cargo pgrx upgrade

# Upgrade to specific version
cargo pgrx upgrade --to 0.16.1

# Include pre-release versions
cargo pgrx upgrade --include-prereleases

# Dry run (show changes without applying)
cargo pgrx upgrade --dry-run
```

**Key Options:**
- `--to VERSION`: Target version (default: latest stable)
- `--include-prereleases`: Include pre-release versions
- `-n, --dry-run`: Print changes without modifying Cargo.toml

### cargo pgrx cross

Cross-compilation utilities (experimental).

```bash
# Generate cross-compilation target bundle
cargo pgrx cross pgrx-target --pg-config /path/to/target/pg_config

# Specify output filename
cargo pgrx cross pgrx-target -o my-target.tgz

# Specify expected PostgreSQL version
cargo pgrx cross pgrx-target --pg-version 17
```

### Environment Variables Reference

**Global Variables:**
| Variable | Description |
|----------|-------------|
| `CARGO_PGRX` | Path to cargo-pgrx binary |
| `PGRX_HOME` | pgrx installation directory (default: `~/.pgrx/`) |
| `PGRX_BUILD_FLAGS` | Additional build flags for all commands |
| `RUST_LOG` | Logging level (trace, debug, info, warn, error) |

**Build Variables:**
| Variable | Description |
|----------|-------------|
| `PG_VERSION` | Default PostgreSQL version |
| `CARGO_TARGET_DIR` | Override cargo target directory |
| `CARGO_PKG_VERSION` | Extension version (auto-detected) |
| `PGRX_BUILD_PROFILE` | Build profile name |
| `PGRX_FEATURES` | Features to enable |
| `PGRX_NO_DEFAULT_FEATURES` | Disable default features |
| `PGRX_ALL_FEATURES` | Enable all features |
| `PGRX_NO_SCHEMA` | Skip schema generation |

**Test Variables:**
| Variable | Description |
|----------|-------------|
| `CARGO_PGRX_TEST_RUNAS` | User to run tests as |
| `CARGO_PGRX_TEST_PGDATA` | Custom test data directory |
| `PGRX_TEST_SKIP` | Skip all tests if set |
| `PGRX_REGRESS_TESTING` | Set during regress tests |

**PostgreSQL Configuration:**
| Variable | Description |
|----------|-------------|
| `PG13_PG_CONFIG` through `PG18_PG_CONFIG` | Paths to pg_config per version |
| `DBNAME` | Default database name |
| `PGRX_PGCLI` | Use pgcli instead of psql |

### Common Workflows

**Development Iteration:**
```bash
# Fast iteration during development
cargo pgrx run pg18                    # Build, install, test interactively
cargo pgrx test pg18 --no-schema       # Skip schema regen for faster tests
cargo pgrx regress pg18 --auto         # Auto-accept changed output
```

**Multi-Version Testing:**
```bash
# Test all supported versions
for v in pg15 pg16 pg17 pg18; do
    cargo pgrx test $v || exit 1
    cargo pgrx regress $v --postgresql-conf "shared_preload_libraries='pg_walrus'" || exit 1
done
```

**Release Build:**
```bash
# Create release packages for deployment
cargo pgrx package --pg-config $(which pg_config)
```

**Debugging:**
```bash
# Enable verbose logging
RUST_LOG=debug cargo pgrx test pg18

# Start PostgreSQL under Valgrind
cargo pgrx run pg18 --valgrind
```

### Cargo.toml Feature Configuration

Standard feature configuration for pgrx extensions:

```toml
[features]
default = ["pg18"]
pg13 = ["pgrx/pg13", "pgrx-tests/pg13"]
pg14 = ["pgrx/pg14", "pgrx-tests/pg14"]
pg15 = ["pgrx/pg15", "pgrx-tests/pg15"]
pg16 = ["pgrx/pg16", "pgrx-tests/pg16"]
pg17 = ["pgrx/pg17", "pgrx-tests/pg17"]
pg18 = ["pgrx/pg18", "pgrx-tests/pg18"]
pg_test = []

[lib]
crate-type = ["cdylib", "lib"]

# Required for proper panic handling in PostgreSQL
[profile.dev]
panic = "unwind"

[profile.release]
panic = "unwind"
opt-level = 3
lto = "fat"
codegen-units = 1
```

### Troubleshooting

**"extension must be loaded via shared_preload_libraries"**
- Background workers require loading at PostgreSQL startup
- Add to postgresql.conf: `shared_preload_libraries = 'pg_walrus'`
- For tests: Implement `pg_test` module with `postgresql_conf_options()`

**Tests fail to find background worker in pg_stat_activity**
- Ensure `pg_test` module exists at crate root (see Testing Strategy section)
- Verify `postgresql_conf_options()` returns correct shared_preload_libraries

**Schema generation fails**
- Try `--skip-build` if extension compiles but schema fails
- Check `RUST_LOG=debug` for detailed errors
- Verify `pg_module_magic!()` is present

**Permission denied during install**
- Use `--sudo` flag: `cargo pgrx install --sudo`
- Or run entire command with sudo (not recommended)

**Cross-compilation issues**
- Use `cargo pgrx cross pgrx-target` to generate target bundle
- Set `PGRX_PG_CONFIG_PATH` for cross-compilation pg_config

## Testing Strategy

pg_walrus uses three complementary testing approaches:

### 1. pgrx Integration Tests (`#[pg_test]`)

Tests that run inside PostgreSQL with full access to SPI, GUCs, and system catalogs.

```bash
cargo pgrx test pg18                    # Run all tests for PG17
cargo pgrx test pg18 test_guc_default   # Run specific test
```

**Use for**: GUC parameter verification, background worker visibility via `pg_stat_activity`, any test requiring database context.

**Module pattern**:
```rust
#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use pgrx::prelude::*;

    #[pg_test]
    fn test_guc_default() {
        let result = Spi::get_one::<&str>("SHOW walrus.enable").unwrap();
        assert_eq!(result, Some("on"));
    }

    #[pg_test(error = "invalid value")]
    fn test_guc_invalid() -> Result<(), spi::Error> {
        Spi::run("SET walrus.threshold = -1")
    }
}
```

**MANDATORY: Background worker testing requires `pg_test` module with `postgresql_conf_options()`**:

```rust
// MANDATORY - Must be at crate root (src/lib.rs)
// WITHOUT THIS MODULE, BACKGROUND WORKER TESTS WILL FAIL
#[cfg(test)]
pub mod pg_test {
    pub fn setup(_options: Vec<&str>) {}

    pub fn postgresql_conf_options() -> Vec<&'static str> {
        vec!["shared_preload_libraries='pg_walrus'"]
    }
}
```

**Why this is MANDATORY**:
- pgrx-tests calls `postgresql_conf_options()` to configure PostgreSQL BEFORE starting
- Without this module, `shared_preload_libraries` is NOT set in postgresql.auto.conf
- Background workers ONLY register when loaded via `shared_preload_libraries`
- Tests verifying background worker visibility (`pg_stat_activity`) WILL FAIL without this

**Failure mode without pg_test module**:
- PostgreSQL starts without the extension in shared_preload_libraries
- `_PG_init()` only called during CREATE EXTENSION (too late for bgworker)
- Background worker never spawns
- `SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE backend_type = 'pg_walrus')` returns FALSE

### 2. Pure Rust Unit Tests (`#[test]`)

Tests for pure Rust logic that does not require PostgreSQL.

```bash
cargo test --lib        # Run pure Rust tests only
```

**Use for**: Mathematical calculations, overflow handling, string formatting.

### 3. pg_regress SQL Tests

SQL-based tests using PostgreSQL's native pg_regress framework.

```bash
cargo pgrx regress pg18                 # Run all pg_regress tests
cargo pgrx regress pg18 guc_params      # Run specific test
cargo pgrx regress pg18 --auto          # Auto-accept new output
cargo pgrx regress pg18 --resetdb       # Reset database first
```

**Directory structure**:
```
tests/pg_regress/
├── sql/               # Test SQL scripts
│   ├── setup.sql      # Creates extension (runs first, special)
│   ├── guc_params.sql # GUC parameter tests
│   └── extension_info.sql
├── expected/          # Expected output files
│   ├── setup.out
│   ├── guc_params.out
│   └── extension_info.out
└── results/           # Generated during tests (gitignored)
```

**Use for**: SQL syntax verification, error message testing, GUC parameter SQL interface.

### When to Use Each Test Type

| Scenario | Test Type |
|----------|-----------|
| GUC parameter defaults (internal) | `#[pg_test]` |
| GUC parameter SQL syntax | pg_regress |
| Background worker visibility | `#[pg_test]` |
| Size calculation formula | `#[test]` |
| Overflow protection | `#[test]` |
| Error message format | pg_regress |
| Extension metadata | pg_regress |

### Multi-Version Testing

```bash
# Test all supported PostgreSQL versions
cargo pgrx test pg15 && cargo pgrx test pg16 && cargo pgrx test pg17 && cargo pgrx test pg18

# pg_regress all versions (requires --postgresql-conf for background worker extensions)
for v in pg15 pg16 pg17 pg18; do
    cargo pgrx regress $v --postgresql-conf "shared_preload_libraries='pg_walrus'" || exit 1
done
```

**Note**: pgrx-managed PostgreSQL instances (ports 28815-28818 in `~/.pgrx/data-XX`) are separate from any system PostgreSQL installations (e.g., Homebrew on port 5432). Each pgrx instance requires explicit `--postgresql-conf` configuration for `shared_preload_libraries`.

## Architecture

### Core Mechanism
The extension works by:
1. Running a background worker that wakes every `checkpoint_timeout` interval
2. Fetching checkpoint statistics via `pgstat_fetch_stat_checkpointer()`
3. Counting forced checkpoints since last check
4. If forced checkpoints exceed threshold, calculating new `max_wal_size` as: `current_size * (forced_checkpoints + 1)`
5. Applying changes via `ALTER SYSTEM` + `SIGHUP` to postmaster

### Source Structure
```
src/
├── lib.rs              # Entry point, _PG_init, GUC registration, pg_schema wrappers
├── worker.rs           # Background worker implementation
├── stats.rs            # Checkpoint statistics access (version-specific)
├── config.rs           # ALTER SYSTEM implementation
├── guc.rs              # GUC parameter definitions
├── history.rs          # History table operations (insert, cleanup)
├── shmem.rs            # Shared memory state (WalrusState, PgLwLock)
├── algorithm.rs        # Sizing algorithms (calculate_new_size, compute_recommendation)
├── functions.rs        # SQL function implementations (status, history, analyze, etc.)
└── tests.rs            # PostgreSQL integration tests (#[pg_test])
```

## GUC Parameters

### Core Parameters

| Parameter | Default | Description |
|-----------|---------|-------------|
| `walrus.enable` | true | Enable/disable auto-sizing |
| `walrus.max` | 4GB | Maximum allowed `max_wal_size` |
| `walrus.threshold` | 2 | Forced checkpoints before resize |

### Auto-Shrink Parameters

| Parameter | Default | Description |
|-----------|---------|-------------|
| `walrus.shrink_enable` | true | Enable/disable automatic shrinking |
| `walrus.shrink_factor` | 0.75 | Multiplier for shrink calculation (0.01-0.99) |
| `walrus.shrink_intervals` | 5 | Quiet intervals before shrinking (1-1000) |
| `walrus.min_size` | 1GB | Minimum floor for `max_wal_size` |

### History Parameters

| Parameter | Default | Description |
|-----------|---------|-------------|
| `walrus.history_retention_days` | 7 | Days to retain history records (0-3650) |

### Database Connection

| Parameter | Default | Description |
|-----------|---------|-------------|
| `walrus.database` | postgres | Database where history table is stored (postmaster context, requires restart) |

## PostgreSQL Version Compatibility

Supports PostgreSQL 15+ due to `pgstat_fetch_stat_checkpointer()` API. Version-specific handling needed for:
- PG 15-16: `stats->requested_checkpoints`
- PG 17+: `stats->num_requested`

## Key Technical Details

- Background worker uses `WaitLatch()` with `checkpoint_timeout` as the interval
- Self-triggered `SIGHUP` detection prevents re-processing own config changes
- Uses `ResourceOwner` for proper cleanup in transaction commands
- `AlterSystemSetConfigFile()` requires AST node construction (`AlterSystemStmt` -> `VariableSetStmt` -> `A_Const`)

## Reference Resources

**pgrx Repository**: `/Users/brandon/src/pgrx/`
- The pgrx framework source is cloned locally for reference
- Use this to look up API patterns, examples, and best practices
- Key directories:
  - `pgrx/` - Core framework code
  - `pgrx-examples/` - Example extensions
  - `pgrx-macros/` - Procedural macros
  - `pgrx-pg-sys/` - PostgreSQL bindings

**PostgreSQL Source**: `/Users/brandon/src/postgres/`
- The PostgreSQL source code is cloned locally for reference
- Use this to look up internal APIs, struct definitions, and implementation details
- Key directories for this extension:
  - `src/backend/postmaster/checkpointer.c` - Checkpointer process implementation
  - `src/backend/postmaster/bgworker.c` - Background worker infrastructure
  - `src/backend/utils/misc/guc.c` - GUC (Grand Unified Configuration) system
  - `src/include/pgstat.h` - Statistics collector definitions
  - `src/backend/commands/variable.c` - ALTER SYSTEM implementation

## Active Technologies
- Rust 1.83+ (latest stable, edition 2024) + pgrx 0.16.1, libc (FFI compatibility) (001-pgrx-core-rewrite)
- N/A (extension modifies postgresql.auto.conf via ALTER SYSTEM) (001-pgrx-core-rewrite)
- Rust 1.83+ (latest stable, edition 2024) + pgrx 0.16.1, libc 0.2 (002-auto-shrink)
- N/A (modifies postgresql.auto.conf via ALTER SYSTEM) (002-auto-shrink)
- PostgreSQL table (`walrus.history`) with BIGSERIAL primary key, TIMESTAMPTZ, JSONB (003-history-table)
- Rust 1.83+ (edition 2024) + pgrx 0.16.1 + pgrx 0.16.1, serde_json 1.x, libc 0.2 (004-sql-observability-functions)
- PostgreSQL shared memory (ephemeral), walrus.history table (persistent) (004-sql-observability-functions)
- PostgreSQL `walrus.history` table (existing from feature 004) (005-dry-run-mode)

## Recent Changes
- 001-pgrx-core-rewrite: Added Rust 1.83+ (latest stable, edition 2024) + pgrx 0.16.1, libc (FFI compatibility)
