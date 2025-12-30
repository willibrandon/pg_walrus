<!--
SYNC IMPACT REPORT
==================
Version change: 0.0.0 → 1.0.0
Bump rationale: Initial constitution creation (MAJOR)

Added sections:
- I. No Task Deferral (NON-NEGOTIABLE)
- II. PostgreSQL Extension Safety
- III. pgrx Idioms
- IV. Test Discipline
- V. Observability
- Additional Constraints
- Development Workflow
- Governance

Removed sections: None (initial creation)

Templates status:
- ✅ plan-template.md - Constitution Check section exists
- ✅ spec-template.md - Compatible structure
- ✅ tasks-template.md - Compatible structure

Follow-up: None
-->

# pg_walrus Constitution

## Core Principles

### I. No Task Deferral (NON-NEGOTIABLE)

Every task assigned MUST be completed in full. No exceptions.

**PROHIBITED in code:**
- Code markers: `TODO`, `FIXME`, `PLACEHOLDER`, `HACK`, `XXX`, `STUB`, `TBD`, `PENDING`
- Incomplete implementations or functions that panic with "not implemented"
- Missing error handling, edge cases, or validation
- Partial implementations that require follow-up work

**PROHIBITED in communication:**
- Hedging: "you might want to...", "consider adding...", "it would be good to..."
- Future promises: "we can optimize later", "phase 2 work", "future enhancement"
- Responsibility shifting: "you'll need to add...", "don't forget to...", "make sure to..."
- Scope deferral: "out of scope", "beyond scope", "not in scope"
- Minimizing: "basic implementation", "simplified version", "for now"

**REQUIRED behavior:**
- Complete all assigned work in full before marking tasks complete
- Implement all edge cases and error handling immediately
- If genuinely blocked, state `BLOCKER: [specific issue]` and request a decision
- Each task MUST be fully implemented before moving to the next

**Rationale**: Deferred work creates technical debt, misleads progress tracking, and shifts burden to users. Complete work or explicitly escalate blockers.

### II. PostgreSQL Extension Safety

All code MUST respect PostgreSQL's safety requirements for extensions.

**Memory Safety:**
- Use pgrx memory contexts correctly (`PgMemoryContexts`)
- Never hold Rust references across PostgreSQL callbacks
- Use `PgBox` for PostgreSQL-allocated memory that Rust manages
- Properly handle `AllocatedByRust` vs `AllocatedByPostgres` semantics

**Error Handling:**
- Use `pgrx::error!()` and `pgrx::warning!()` for PostgreSQL-compatible error reporting
- Never panic in background worker code—use `PgTryBuilder` for error recovery
- Handle `SIGTERM` and `SIGHUP` signals appropriately in background workers
- Use `WaitLatch` with proper timeout handling

**Transaction Safety:**
- Wrap SPI calls in appropriate transaction contexts
- Use `Spi::connect()` for database queries
- Never hold transaction-scoped resources across `WaitLatch` calls

**Rationale**: PostgreSQL extensions run in the same process as the database. Crashes or memory corruption can bring down the entire database cluster.

### III. pgrx Idioms

Follow established pgrx patterns for PostgreSQL extension development.

**GUC Registration:**
- Use `GucRegistry::define_*_guc()` for all configuration parameters
- Include proper `GucContext` (e.g., `SIGHUP` for runtime-changeable parameters)
- Provide descriptive help text for each GUC

**Background Workers:**
- Use `BackgroundWorkerBuilder` for worker registration
- Implement proper startup, main loop, and shutdown sequences
- Handle `SignalWakeFlags` for `SIGHUP` and `SIGTERM`
- Use `BackgroundWorker::wait_latch()` with appropriate conditions

**Version Compatibility:**
- Use `#[cfg(feature = "pgXX")]` for version-specific code paths
- Test against all supported PostgreSQL versions (15, 16, 17)
- Document version-specific behavior differences

**Rationale**: Consistent patterns reduce bugs, improve maintainability, and ensure compatibility across PostgreSQL versions.

### IV. Test Discipline

Tests MUST be written for all functionality.

**Required Tests:**
- Unit tests for pure Rust logic (`#[test]`)
- pgrx integration tests for PostgreSQL functionality (`#[pg_test]`)
- Tests for each GUC parameter behavior
- Tests for background worker start/stop lifecycle
- Tests for error conditions and edge cases

**Test Execution:**
- All tests MUST pass before merging
- Use `cargo pgrx test pgXX` for each supported PostgreSQL version
- Failed tests block implementation—fix the test or the code, no skipping

**Rationale**: PostgreSQL extensions are difficult to debug in production. Comprehensive testing catches issues before deployment.

### V. Observability

All significant operations MUST be observable.

**Logging:**
- Use `pgrx::log!()`, `pgrx::warning!()`, `pgrx::error!()` appropriately
- Log configuration changes (SIGHUP handling)
- Log sizing decisions with before/after values
- Include sufficient context for debugging (checkpoint counts, sizes, thresholds)

**Metrics (planned):**
- Expose statistics via SQL functions
- Track sizing history in a queryable format
- Support standard monitoring integrations

**Rationale**: DBAs need visibility into extension behavior for troubleshooting and capacity planning.

## Additional Constraints

### Technology Stack

| Component | Requirement |
|-----------|-------------|
| Language | Rust (latest stable) |
| Framework | pgrx 0.12+ |
| PostgreSQL | 15, 16, 17 (18 when available) |
| Build | cargo-pgrx |
| Testing | cargo pgrx test |

### Performance Requirements

- Background worker MUST NOT block PostgreSQL operations
- Configuration reload MUST complete within 1 second
- Memory overhead MUST be minimal (< 1MB per worker)

### Compatibility Requirements

- Extension MUST load via `shared_preload_libraries`
- GUC changes MUST take effect on SIGHUP (no restart required)
- MUST work alongside common extensions (pg_stat_statements, etc.)

## Development Workflow

### Code Review Requirements

- All PRs MUST pass CI (cargo check, clippy, fmt, test)
- Constitution compliance MUST be verified
- No prohibited code markers or deferral language

### Quality Gates

1. **Pre-commit**: `cargo fmt --check && cargo clippy -- -D warnings`
2. **Pre-merge**: All tests pass on all supported PostgreSQL versions
3. **Pre-release**: Manual testing on production-like workload

### Documentation Requirements

- Public functions MUST have rustdoc comments
- GUC parameters MUST be documented with examples
- README MUST reflect current functionality

## Governance

This constitution supersedes all other practices. Amendments require:

1. Written proposal with rationale
2. Impact assessment on existing code
3. Migration plan if breaking changes
4. Update to this document with version increment

**Version Policy:**
- MAJOR: Principle removal or redefinition
- MINOR: New principle or section added
- PATCH: Clarifications or typo fixes

**Compliance Review:**
- All PRs MUST verify constitution compliance
- Violations require explicit justification in Complexity Tracking
- Unjustified violations block merge

**Guidance File**: See `CLAUDE.md` for runtime development guidance.

**Version**: 1.0.0 | **Ratified**: 2025-12-29 | **Last Amended**: 2025-12-29
