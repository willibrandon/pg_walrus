# Feature Specification: pg_walrus Core Extension (pgrx Rewrite)

**Feature Branch**: `001-pgrx-core-rewrite`
**Created**: 2025-12-29
**Status**: Draft
**Input**: User description: "Rewrite pg_walsizer as pg_walrus using Rust and the pgrx framework"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Automatic WAL Size Adjustment (Priority: P1)

A database administrator runs a PostgreSQL database with variable write workloads. During peak periods, the system experiences forced checkpoints that degrade performance. The administrator installs pg_walrus and enables it. The extension automatically monitors checkpoint activity and increases `max_wal_size` when forced checkpoints exceed the threshold, eliminating performance degradation without manual intervention.

**Why this priority**: This is the core value proposition of the extension. Without automatic WAL size adjustment, there is no reason to use pg_walrus.

**Independent Test**: Can be fully tested by generating WAL activity that triggers forced checkpoints and verifying that `max_wal_size` is automatically increased in `postgresql.auto.conf`.

**Acceptance Scenarios**:

1. **Given** a PostgreSQL instance with pg_walrus loaded and `max_wal_size` set to 1GB, **When** the workload generates enough WAL to trigger 3 forced checkpoints within a checkpoint timeout period, **Then** pg_walrus increases `max_wal_size` to 4GB (1GB * (3+1)) via ALTER SYSTEM and signals the postmaster to reload configuration.

2. **Given** a PostgreSQL instance with pg_walrus loaded, **When** forced checkpoints stay below the configured threshold (default: 2), **Then** pg_walrus takes no action and `max_wal_size` remains unchanged.

3. **Given** a PostgreSQL instance with pg_walrus loaded and a calculated new size exceeding `walrus.max`, **When** resize is triggered, **Then** pg_walrus sets `max_wal_size` to exactly `walrus.max` and logs a warning about the cap being reached.

---

### User Story 2 - Runtime Configuration Control (Priority: P2)

A database administrator needs to adjust pg_walrus behavior without restarting PostgreSQL. They modify GUC parameters (`walrus.enable`, `walrus.max`, `walrus.threshold`) via `ALTER SYSTEM` or `SET` commands and issue `pg_reload_conf()`. The extension immediately respects the new configuration values.

**Why this priority**: Configuration flexibility is essential for production environments where settings need tuning without downtime.

**Independent Test**: Can be tested by changing GUC values via SQL and verifying the extension respects them on the next checkpoint cycle.

**Acceptance Scenarios**:

1. **Given** pg_walrus is running with `walrus.enable = true`, **When** an administrator sets `walrus.enable = false` and reloads configuration, **Then** pg_walrus stops monitoring checkpoint activity and making adjustments until re-enabled.

2. **Given** pg_walrus is running with default settings, **When** an administrator sets `walrus.max = 8192` (8GB) and reloads configuration, **Then** pg_walrus uses the new maximum cap for all subsequent resize calculations.

3. **Given** pg_walrus is running with `walrus.threshold = 2`, **When** an administrator sets `walrus.threshold = 5` and reloads configuration, **Then** pg_walrus requires 5 forced checkpoints before triggering a resize.

---

### User Story 3 - Extension Lifecycle Management (Priority: P3)

A database administrator adds pg_walrus to `shared_preload_libraries` and restarts PostgreSQL. The extension initializes its background worker, registers GUC parameters, and begins monitoring. When PostgreSQL shuts down, the background worker terminates gracefully.

**Why this priority**: Proper lifecycle management ensures the extension integrates cleanly with PostgreSQL operations without causing startup/shutdown issues.

**Independent Test**: Can be tested by starting/stopping PostgreSQL and verifying the background worker appears in `pg_stat_activity` when running and terminates cleanly on shutdown.

**Acceptance Scenarios**:

1. **Given** pg_walrus is listed in `shared_preload_libraries`, **When** PostgreSQL starts, **Then** the pg_walrus background worker appears in `pg_stat_activity` with `backend_type = 'pg_walrus'` after recovery completes.

2. **Given** pg_walrus background worker is running, **When** PostgreSQL receives a SIGTERM for shutdown, **Then** the background worker logs a shutdown message and terminates without errors.

3. **Given** pg_walrus GUC parameters are not explicitly set, **When** PostgreSQL starts with pg_walrus loaded, **Then** `walrus.enable = true`, `walrus.max = 4096` (4GB), and `walrus.threshold = 2` are the effective defaults.

---

### User Story 4 - Multi-Version PostgreSQL Support (Priority: P2)

A database administrator operates PostgreSQL clusters running versions 15, 16, 17, and 18. They install pg_walrus compiled for each version. The extension functions identically across all versions, correctly accessing checkpoint statistics using the appropriate internal API for each PostgreSQL version.

**Why this priority**: Organizations commonly run multiple PostgreSQL versions, and the extension must work consistently across the supported range.

**Independent Test**: Can be tested by running the extension's test suite against each PostgreSQL version (15, 16, 17, 18) and verifying all tests pass.

**Acceptance Scenarios**:

1. **Given** pg_walrus is compiled for PostgreSQL 15 or 16, **When** the extension fetches checkpoint statistics, **Then** it accesses `stats->requested_checkpoints` to count forced checkpoints.

2. **Given** pg_walrus is compiled for PostgreSQL 17 or 18, **When** the extension fetches checkpoint statistics, **Then** it accesses `stats->num_requested` to count forced checkpoints.

3. **Given** pg_walrus is installed on any supported PostgreSQL version, **When** the administrator queries `SHOW walrus.enable`, **Then** the current value is returned correctly.

---

### Edge Cases

- What happens when `max_wal_size` is already at `walrus.max`? The extension skips the resize and continues monitoring without making changes.
- What happens when the extension triggers its own SIGHUP? The extension detects self-triggered SIGHUPs via atomic signal flags and skips processing that iteration to avoid redundant work.
- What happens when checkpoint statistics return a null pointer? The extension logs a warning and continues operating, retrying on the next cycle.
- What happens when `ALTER SYSTEM` fails due to permissions or disk issues? The extension logs a warning and continues operating without crashing.
- What happens when the calculated new size overflows `i32::MAX`? The extension caps the value at `i32::MAX` before applying the `walrus.max` cap.
- What happens when PostgreSQL is in recovery (standby)? The background worker waits until `BgWorkerStart_RecoveryFinished` before starting, so it only runs on primaries.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST register a background worker that starts after PostgreSQL recovery completes.
- **FR-002**: Background worker MUST wake at intervals matching `checkpoint_timeout` to check for forced checkpoints.
- **FR-003**: System MUST fetch checkpoint statistics via `pgstat_fetch_stat_checkpointer()` and track the delta since the last check.
- **FR-004**: System MUST calculate new `max_wal_size` as `current_size * (forced_checkpoints + 1)` when forced checkpoints exceed threshold.
- **FR-005**: System MUST cap calculated size at `walrus.max` to prevent unbounded growth.
- **FR-006**: System MUST apply changes via `ALTER SYSTEM SET max_wal_size` and signal the postmaster with SIGHUP.
- **FR-007**: System MUST detect and skip self-triggered SIGHUP signals to prevent redundant processing.
- **FR-008**: System MUST handle SIGTERM gracefully by logging and exiting the background worker.
- **FR-009**: System MUST register three GUC parameters: `walrus.enable` (bool), `walrus.max` (int with MB unit), `walrus.threshold` (int).
- **FR-010**: GUC parameters MUST be modifiable via SIGHUP (runtime reload) without server restart.
- **FR-011**: System MUST support PostgreSQL versions 15, 16, 17, and 18 with appropriate conditional compilation.
- **FR-012**: System MUST use `#[pg_guard]` on all FFI boundary functions for proper error handling.
- **FR-013**: System MUST use atomic types for signal handling flags to ensure thread safety.
- **FR-014**: Background worker MUST skip the first iteration after startup to establish a baseline checkpoint count.

### Key Entities

- **CheckpointStats**: Represents the delta of forced checkpoints between monitoring intervals. Key attributes: `forced_checkpoints` (count since last check).
- **GUC Parameters**: Configuration settings controlling extension behavior. Three parameters: enable flag, maximum size cap, and threshold count.
- **Background Worker State**: Maintains running state including previous checkpoint count, first-iteration flag, and atomic signal flags.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Forced checkpoint count decreases by at least 80% after pg_walrus adjusts `max_wal_size` on a workload that previously triggered forced checkpoints.
- **SC-002**: pg_walrus applies configuration changes within one `checkpoint_timeout` interval after detecting threshold breach.
- **SC-003**: pg_walrus passes all integration tests on PostgreSQL 15, 16, 17, and 18.
- **SC-004**: pg_walrus background worker starts within 5 seconds of PostgreSQL recovery completion.
- **SC-005**: pg_walrus respects GUC parameter changes within one monitoring cycle after `pg_reload_conf()`.
- **SC-006**: pg_walrus background worker shuts down cleanly (no error messages, no resource leaks) when PostgreSQL stops.
- **SC-007**: pg_walrus correctly logs all resize operations with before/after values for administrator visibility.
- **SC-008**: pg_walrus achieves functional parity with the original C implementation (pg_walsizer) for all core behaviors.

## Assumptions

- PostgreSQL is configured with `shared_preload_libraries` including pg_walrus (required for background workers).
- The extension runs on a primary server (not a standby) since `ALTER SYSTEM` is not valid on read-only instances.
- The DBA has sufficient permissions to modify `max_wal_size` via ALTER SYSTEM.
- The `checkpoint_timeout` GUC is set to a reasonable value (default: 5 minutes) and is not modified to extremely short intervals.
- The underlying WAL storage device has sufficient capacity to accommodate growth up to `walrus.max`.
- pgrx version 0.16 or compatible is used for building the extension.
