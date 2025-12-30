# Feature Specification: SQL Observability Functions

**Feature Branch**: `004-sql-observability-functions`
**Created**: 2025-12-30
**Status**: Draft
**Input**: User description: "Expose extension state and controls via SQL functions for monitoring and management."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Monitor Extension Health (Priority: P1)

A database administrator wants to check the current operational status of pg_walrus to verify it is functioning correctly and understand its current configuration.

**Why this priority**: This is the foundational observability capability. Without knowing the extension's current state, administrators cannot make informed decisions about configuration changes or troubleshoot issues.

**Independent Test**: Can be fully tested by calling `SELECT walrus.status()` and verifying all expected fields are present with valid values. Delivers immediate value for monitoring dashboards and health checks.

**Acceptance Scenarios**:

1. **Given** pg_walrus is installed and running, **When** an administrator calls `SELECT walrus.status()`, **Then** the function returns a JSONB object containing all current configuration values and worker state.
2. **Given** pg_walrus is installed but disabled via `walrus.enable = false`, **When** an administrator calls `SELECT walrus.status()`, **Then** the status shows `enabled: false` and the worker state reflects the disabled configuration.
3. **Given** pg_walrus background worker is running, **When** `SELECT walrus.status()` is called, **Then** `worker_running` is `true` and `last_check_time` contains a valid timestamp.

---

### User Story 2 - View Adjustment History (Priority: P1)

A database administrator wants to review the history of all max_wal_size adjustments made by pg_walrus to understand sizing trends and validate that automatic adjustments are working as expected.

**Why this priority**: History visibility is critical for post-incident analysis, capacity planning, and validating that the extension behaves correctly over time.

**Independent Test**: Can be fully tested by calling `SELECT * FROM walrus.history()` after adjustments have occurred. Delivers value for audit trails and trend analysis.

**Acceptance Scenarios**:

1. **Given** pg_walrus has made sizing adjustments, **When** an administrator calls `SELECT * FROM walrus.history()`, **Then** the function returns rows containing timestamp, action, old_size_mb, new_size_mb, forced_checkpoints, and reason for each adjustment.
2. **Given** no sizing adjustments have occurred, **When** an administrator calls `SELECT * FROM walrus.history()`, **Then** the function returns an empty result set (zero rows).
3. **Given** multiple adjustments exist, **When** an administrator orders results by timestamp, **Then** the chronological sequence of adjustments is visible for trend analysis.

---

### User Story 3 - Preview Recommendations Before Applying (Priority: P2)

A database administrator wants to see what pg_walrus would recommend for max_wal_size without actually applying the change, enabling preview and validation of the algorithm's decisions.

**Why this priority**: Preview capability is important for building trust in the automation and for environments where administrators want to review changes before they occur.

**Independent Test**: Can be fully tested by calling `SELECT walrus.recommendation()` and verifying the returned recommendation matches expected algorithm behavior based on current state.

**Acceptance Scenarios**:

1. **Given** current checkpoint activity warrants a size increase, **When** an administrator calls `SELECT walrus.recommendation()`, **Then** the function returns a JSONB object with `action: "increase"`, the recommended size, and an explanation.
2. **Given** sustained low activity warrants a size decrease, **When** an administrator calls `SELECT walrus.recommendation()`, **Then** the function returns `action: "decrease"` with shrink calculations.
3. **Given** current size is optimal, **When** an administrator calls `SELECT walrus.recommendation()`, **Then** the function returns `action: "none"` indicating no change is recommended.

---

### User Story 4 - Trigger Immediate Analysis (Priority: P2)

A database administrator wants to manually trigger an analysis cycle to immediately assess the current checkpoint situation and optionally apply recommendations, useful after configuration changes or incident response.

**Why this priority**: Immediate analysis capability is valuable for proactive management but builds on the status and recommendation capabilities.

**Independent Test**: Can be fully tested by calling `SELECT walrus.analyze()` and verifying the analysis executes and returns appropriate results.

**Acceptance Scenarios**:

1. **Given** pg_walrus is enabled and running, **When** an administrator calls `SELECT walrus.analyze()`, **Then** the function performs an immediate analysis and returns a JSONB object with `analyzed: true`, the recommendation, and `applied: false`.
2. **Given** `walrus.enable = false`, **When** an administrator calls `SELECT walrus.analyze()`, **Then** the function returns `analyzed: false` with a reason indicating the extension is disabled.
3. **Given** an analysis determines a size change is needed, **When** the administrator calls `SELECT walrus.analyze(apply := true)`, **Then** the change is executed and the result shows `applied: true`.
4. **Given** an analysis determines no change is needed, **When** the administrator calls `SELECT walrus.analyze(apply := true)`, **Then** the result shows `applied: false` because no action was required.

---

### User Story 5 - Reset Extension State (Priority: P3)

A database administrator wants to reset pg_walrus state to a clean baseline after testing, configuration changes, or when troubleshooting unexpected behavior.

**Why this priority**: Reset is an administrative utility that is less frequently needed than monitoring and analysis capabilities.

**Independent Test**: Can be fully tested by calling `SELECT walrus.reset()` and verifying that history is cleared and internal counters are reset.

**Acceptance Scenarios**:

1. **Given** history records exist, **When** an administrator calls `SELECT walrus.reset()`, **Then** the function clears all history records and returns `true`.
2. **Given** reset succeeds, **When** the administrator subsequently calls `SELECT * FROM walrus.history()`, **Then** zero rows are returned.
3. **Given** reset is called, **When** the background worker's internal state is examined via `walrus.status()`, **Then** counters like `total_adjustments` and `quiet_intervals` reflect the reset state.

---

### Edge Cases

- What happens when `walrus.status()` is called before the background worker has completed its first cycle? Returns valid status with `null` for time-based fields that haven't been populated yet.
- How does `walrus.recommendation()` behave when checkpoint statistics are unavailable (null pointer from pgstat)? Returns `action: "error"` with an explanation that statistics are unavailable.
- What happens when `walrus.analyze()` is called while the background worker is mid-cycle? The SQL function operates independently; background worker continues unaffected.
- How does `walrus.history()` handle the case where the history table was dropped? Returns an appropriate error rather than crashing.
- What happens when `walrus.reset()` is called by a non-superuser? Depends on privilege configuration; if table permissions restrict access, appropriate permission error is returned.
- How does `walrus.status()` report when `max_wal_size` is at the configured `walrus.max` ceiling? Includes a field indicating the ceiling status.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST provide a `walrus.status()` function that returns current extension state as JSONB
- **FR-002**: The status JSONB MUST include: `enabled`, `current_max_wal_size_mb`, `configured_maximum_mb`, `threshold`, `checkpoint_timeout_sec`, `worker_running`
- **FR-003**: The status JSONB MUST include time-based fields: `last_check_time`, `last_adjustment_time`
- **FR-004**: The status JSONB MUST include counter fields: `total_adjustments`, `quiet_intervals`
- **FR-005**: The status JSONB MUST include shrink configuration: `shrink_enabled`, `shrink_factor`, `shrink_intervals`, `min_size_mb`
- **FR-006**: System MUST provide a `walrus.history()` function that returns the adjustment history as a set of records
- **FR-007**: The history function MUST return columns: `timestamp`, `action`, `old_size_mb`, `new_size_mb`, `forced_checkpoints`, `reason`
- **FR-008**: System MUST provide a `walrus.recommendation()` function that returns a recommendation without applying changes
- **FR-009**: The recommendation JSONB MUST include: `current_size_mb`, `recommended_size_mb`, `action` (one of "increase", "decrease", "none", "error"), `reason`, `confidence`
- **FR-010**: System MUST provide a `walrus.analyze(apply boolean DEFAULT false)` function that triggers immediate analysis with optional application of recommendations
- **FR-011**: The analyze function MUST return JSONB with: `analyzed` (boolean), `recommendation` (nested object), `applied` (boolean); `applied` is only `true` when `apply` parameter is `true` AND a change was executed
- **FR-012**: System MUST provide a `walrus.reset()` function that clears history table AND writes zeros to shared memory counters (quiet_intervals, total_adjustments, timestamps)
- **FR-013**: The reset function MUST return `true` on success, `false` on failure
- **FR-014**: All functions MUST be created in the `walrus` schema
- **FR-015**: All functions MUST use `#[pg_extern]` for proper SQL function export
- **FR-016**: JSONB-returning functions MUST use `pgrx::JsonB` wrapper type
- **FR-017**: The `walrus.history()` function MUST return results via `TableIterator` for proper set-returning function behavior
- **FR-018**: Status function MUST report whether current `max_wal_size` has reached the `walrus.max` ceiling
- **FR-019**: Recommendation function MUST calculate confidence as a percentage (0-100) based on data quality and sample size
- **FR-020**: `walrus.analyze()` and `walrus.reset()` MUST require superuser privileges; `walrus.status()`, `walrus.history()`, and `walrus.recommendation()` MUST be callable by any user
- **FR-021**: Extension MUST use PostgreSQL shared memory to store and expose ephemeral worker state (quiet_intervals, last_check_time, last_adjustment_time, total_adjustments, prev_requested) for real-time access by SQL functions

### Key Entities *(include if feature involves data)*

- **Extension State**: Runtime configuration values (GUC parameters) and ephemeral worker state stored in shared memory (quiet_intervals, last_check_time, last_adjustment_time, total_adjustments, prev_requested)
- **History Record**: Persisted record of a sizing decision with timestamp, action type, size values, checkpoint context, and optional metadata
- **Recommendation**: Computed suggestion containing current state analysis, proposed action, and confidence level

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: All five SQL functions (`walrus.status()`, `walrus.history()`, `walrus.recommendation()`, `walrus.analyze()`, `walrus.reset()`) are callable from SQL and return expected types
- **SC-002**: `walrus.status()` returns accurate real-time extension state within 100ms execution time
- **SC-003**: `walrus.history()` returns complete history data matching `walrus.history` table contents
- **SC-004**: `walrus.recommendation()` produces actionable recommendations consistent with the resize algorithm
- **SC-005**: `walrus.analyze()` completes within one checkpoint analysis cycle time
- **SC-006**: All functions handle error conditions gracefully without crashing PostgreSQL
- **SC-007**: Functions integrate with existing monitoring tools via standard SQL interfaces

## Clarifications

### Session 2025-12-30

- Q: What are the authorization requirements for each function? → A: Only `walrus.analyze()` and `walrus.reset()` require superuser; `walrus.status()`, `walrus.history()`, and `walrus.recommendation()` are public.
- Q: How should ephemeral worker state (quiet_intervals, timestamps) be shared with SQL functions? → A: Use PostgreSQL shared memory (shmem) to expose worker counters and timestamps.
- Q: Should walrus.analyze() automatically apply changes when warranted? → A: Never apply automatically; add optional `apply` boolean parameter (default false).
- Q: How should walrus.reset() handle worker state in shared memory? → A: Reset directly writes zeros to shared memory; worker sees reset state on next cycle.
- Q: Should prev_requested (checkpoint baseline) be in shmem for recommendation()? → A: Yes, store prev_requested in shmem so recommendation() can calculate delta and suggest grows.

## Assumptions

- The background worker state (quiet_intervals, last_check_time, last_adjustment_time, total_adjustments, prev_requested) will be exposed via PostgreSQL shared memory (shmem), enabling real-time visibility from SQL functions and full recommendation capability.
- The `walrus.analyze()` function runs its own analysis logic in the SQL session context (not via worker signaling); when `apply := true`, it executes ALTER SYSTEM directly.
- Confidence calculation for recommendations uses heuristics based on: number of observation intervals since last adjustment, stability of checkpoint patterns, and whether statistics are fresh.
- The `walrus.reset()` function resets both the history table (via DELETE) and directly writes zeros to shared memory counters (quiet_intervals, total_adjustments, timestamps); the worker sees the reset state on its next cycle.
- All functions require the extension to be installed; calling functions without `CREATE EXTENSION pg_walrus` will fail with appropriate errors.
