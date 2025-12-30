# Feature Specification: Dry-Run Mode

**Feature Branch**: `005-dry-run-mode`
**Created**: 2025-12-30
**Status**: Draft
**Input**: User description: "Test extension behavior without making actual configuration changes"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Validate Extension Before Production Enablement (Priority: P1)

A database administrator deploying pg_walrus for the first time wants to observe what sizing decisions the extension would make before allowing it to modify `max_wal_size` in production. They enable dry-run mode to watch the logs and history table for several days to validate the algorithm behaves as expected under their workload.

**Why this priority**: This is the core value proposition—safe validation before committing to automatic configuration changes. Production databases require careful vetting of any automated tuning.

**Independent Test**: Enable `walrus.dry_run = true` alongside `walrus.enable = true`, generate checkpoint activity exceeding threshold, and verify no `ALTER SYSTEM` is executed while appropriate log messages and history records appear.

**Acceptance Scenarios**:

1. **Given** `walrus.enable = true` and `walrus.dry_run = true` and checkpoint activity exceeds threshold, **When** the background worker processes the cycle, **Then** a log message appears indicating what change WOULD be made and no `ALTER SYSTEM` is executed
2. **Given** `walrus.enable = true` and `walrus.dry_run = true`, **When** a sizing decision would occur, **Then** a history record is inserted with `action = 'dry_run'`
3. **Given** `walrus.dry_run = true`, **When** the worker would normally send SIGHUP to postmaster, **Then** no SIGHUP is sent

---

### User Story 2 - Tune Algorithm Parameters Safely (Priority: P2)

A DBA wants to experiment with different `walrus.threshold` and `walrus.shrink_factor` values to find optimal settings for their workload. Using dry-run mode, they can change parameters and observe what decisions would be made without actually modifying `max_wal_size`.

**Why this priority**: Parameter tuning is a common operational task that dry-run mode enables safely, but depends on the core dry-run functionality.

**Independent Test**: Change `walrus.threshold` from 2 to 5, reload configuration, and observe that dry-run decisions reflect the new threshold without affecting actual `max_wal_size`.

**Acceptance Scenarios**:

1. **Given** `walrus.dry_run = true` and `walrus.threshold = 5`, **When** 4 forced checkpoints occur, **Then** no dry-run decision is logged (below threshold)
2. **Given** `walrus.dry_run = true` and `walrus.threshold = 5`, **When** 6 forced checkpoints occur, **Then** a dry-run decision IS logged showing what change would occur

---

### User Story 3 - Audit Decision History for Compliance (Priority: P3)

A DBA in a regulated environment needs a complete audit trail of what pg_walrus would have done, even in dry-run mode. The history table records all simulated decisions with full metadata for compliance review.

**Why this priority**: Audit trail is a secondary benefit that builds on top of the dry-run logging capability.

**Independent Test**: Query `walrus.history` after dry-run decisions and verify records contain complete metadata including `dry_run: true` marker.

**Acceptance Scenarios**:

1. **Given** dry-run mode is enabled and a grow decision would occur, **When** querying `walrus.history`, **Then** a record exists with `action = 'dry_run'` and `metadata->'would_apply'` = 'increase'
2. **Given** dry-run mode is enabled and a shrink decision would occur, **When** querying `walrus.history`, **Then** a record exists with `action = 'dry_run'` and `metadata->'would_apply'` = 'decrease'

---

### Edge Cases

- What happens when dry-run mode is enabled mid-cycle (changed via `ALTER SYSTEM` + `pg_reload_conf()`)?
  - The change takes effect on the next iteration; in-progress decisions complete with their original mode
- What happens when `walrus.enable = false` but `walrus.dry_run = true`?
  - No decisions are made (enable must be true for any processing); dry_run is a modifier, not a standalone mode
- What happens when history table does not exist but dry-run logging is enabled?
  - Log message appears in PostgreSQL log; history insert is skipped gracefully (existing behavior from history module)
- What happens when calculated size would exceed `walrus.max` in dry-run mode?
  - Log shows capped decision; history records `action = 'dry_run'` with `metadata->'would_apply' = 'capped'`

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST provide a `walrus.dry_run` GUC parameter of type boolean with default value `false`
- **FR-002**: System MUST allow `walrus.dry_run` to be changed at runtime via `ALTER SYSTEM` with `SIGHUP` reload (GucContext::Sighup)
- **FR-003**: When `walrus.dry_run = true` and a grow decision would occur, system MUST log: `LOG: pg_walrus [DRY-RUN]: would change max_wal_size from X MB to Y MB (threshold exceeded)`
- **FR-004**: When `walrus.dry_run = true` and a shrink decision would occur, system MUST log: `LOG: pg_walrus [DRY-RUN]: would change max_wal_size from X MB to Y MB (sustained low activity)`
- **FR-005**: When `walrus.dry_run = true` and a capped decision would occur, system MUST log: `LOG: pg_walrus [DRY-RUN]: would change max_wal_size from X MB to Y MB (capped at walrus.max)`
- **FR-006**: When `walrus.dry_run = true`, system MUST NOT execute `ALTER SYSTEM SET max_wal_size`
- **FR-007**: When `walrus.dry_run = true`, system MUST NOT send SIGHUP to the postmaster
- **FR-008**: When `walrus.dry_run = true` and a decision would occur, system MUST insert a history record with `action = 'dry_run'`
- **FR-009**: Dry-run history records MUST include `metadata->'dry_run' = true`
- **FR-010**: Dry-run history records MUST include `metadata->'would_apply'` with value 'increase', 'decrease', or 'capped'
- **FR-011**: Dry-run history records MUST include all algorithm metadata (delta, multiplier, calculated_size_mb, shrink_factor, quiet_intervals as applicable)
- **FR-012**: When `walrus.dry_run = false` (default), system MUST execute sizing decisions normally (no change to existing behavior)
- **FR-013**: The `walrus.dry_run` GUC MUST be visible in `pg_settings` with appropriate short_desc and long_desc

### Key Entities

- **walrus.dry_run GUC**: Boolean configuration parameter controlling dry-run mode behavior
- **Dry-run log entry**: PostgreSQL LOG message with `[DRY-RUN]` prefix indicating simulated decision
- **Dry-run history record**: Row in `walrus.history` with `action = 'dry_run'` and metadata indicating what would have happened

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Administrators can enable dry-run mode and observe simulated decisions without any actual configuration changes occurring
- **SC-002**: All dry-run decisions are logged with clear indication that no change was made
- **SC-003**: History table provides complete audit trail distinguishing dry-run decisions from actual decisions
- **SC-004**: Transitioning from dry-run to active mode requires only changing `walrus.dry_run = false` and reloading configuration
- **SC-005**: Dry-run mode has no impact on PostgreSQL performance beyond normal pg_walrus overhead

## Assumptions

- The history table feature (004) is available; dry-run mode gracefully handles missing history table
- Log messages follow existing pg_walrus log format conventions with appropriate log level (LOG for decisions)
- The `[DRY-RUN]` prefix provides clear visual distinction in log output
- Shared memory state (quiet_intervals, prev_requested) continues to update normally in dry-run mode so that transitions between modes are seamless
- Dry-run mode does not reset the quiet_intervals counter differently than normal mode; the algorithm state progresses identically
