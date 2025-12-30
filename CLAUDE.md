# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

pg_walrus is a Rust rewrite (using pgrx) of pg_walsizer - a PostgreSQL extension that automatically monitors and adjusts `max_wal_size` to prevent performance-degrading forced checkpoints. The name comes from WAL + Rust = Walrus.

**Current state**: The repository contains the original C implementation (pg_walsizer) and design documents for the Rust conversion. The Rust implementation has not yet been created.

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
cargo pgrx test pg17                       # Run pgrx integration tests
cargo pgrx regress pg17                    # Run pg_regress SQL tests
cargo pgrx package --pg-config /usr/bin/pg_config  # Create package
```

## Testing Strategy

pg_walrus uses three complementary testing approaches:

### 1. pgrx Integration Tests (`#[pg_test]`)

Tests that run inside PostgreSQL with full access to SPI, GUCs, and system catalogs.

```bash
cargo pgrx test pg17                    # Run all tests for PG17
cargo pgrx test pg17 test_guc_default   # Run specific test
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
cargo pgrx regress pg17                 # Run all pg_regress tests
cargo pgrx regress pg17 guc_params      # Run specific test
cargo pgrx regress pg17 --auto          # Auto-accept new output
cargo pgrx regress pg17 --resetdb       # Reset database first
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

# pg_regress all versions
cargo pgrx regress pg15 && cargo pgrx regress pg16 && cargo pgrx regress pg17 && cargo pgrx regress pg18
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

## Recent Changes
- 001-pgrx-core-rewrite: Added Rust 1.83+ (latest stable, edition 2024) + pgrx 0.16.1, libc (FFI compatibility)
