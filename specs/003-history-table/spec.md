# Feature Specification: History Table

**Feature Branch**: `003-history-table`
**Created**: 2025-12-30
**Status**: Draft
**Input**: User description: "Persistent audit trail of all sizing decisions for analysis and compliance."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Query Sizing Decision History (Priority: P1)

A DBA wants to understand the historical sizing decisions made by pg_walrus to analyze checkpoint patterns and WAL growth trends over time. They connect to the database and query the history table to see when and why max_wal_size was increased or decreased.

**Why this priority**: This is the core value proposition - without the ability to query history, the audit trail has no purpose. Every other feature depends on this foundational capability.

**Independent Test**: Can be fully tested by inserting history records and querying them back. Delivers immediate value for analysis workflows.

**Acceptance Scenarios**:

1. **Given** the extension is installed and history table exists, **When** a DBA runs `SELECT * FROM walrus.history ORDER BY timestamp DESC LIMIT 10`, **Then** they see the most recent 10 sizing decisions with all columns populated.

2. **Given** multiple sizing events have occurred, **When** a DBA filters by action type `SELECT * FROM walrus.history WHERE action = 'increase'`, **Then** only increase events are returned.

3. **Given** the history table has records, **When** a DBA queries for events within a date range, **Then** records matching the timestamp filter are returned efficiently using the timestamp index.

---

### User Story 2 - Automatic Event Logging (Priority: P1)

The background worker automatically logs every sizing decision to the history table without manual intervention. This includes increases, decreases, capped adjustments, and decisions where no change was made.

**Why this priority**: Equal priority with querying because without automatic logging, there is no history to query. These two stories together form the minimum viable feature.

**Independent Test**: Can be tested by triggering sizing events and verifying corresponding history records are created with correct values.

**Acceptance Scenarios**:

1. **Given** `walrus.enable` is on and forced checkpoints exceed threshold, **When** the worker increases max_wal_size, **Then** a history record with action='increase' is inserted with the old and new sizes.

2. **Given** shrinking conditions are met, **When** the worker decreases max_wal_size, **Then** a history record with action='decrease' is inserted.

3. **Given** a resize is requested but capped at `walrus.max`, **When** the adjustment completes, **Then** a history record with action='capped' is inserted showing the capped value.

4. **Given** forced checkpoints are below threshold, **When** no resize occurs, **Then** no history record is created for that interval (to avoid table bloat).

---

### User Story 3 - Automatic History Cleanup (Priority: P2)

Old history records are automatically deleted based on the configured retention period to prevent unbounded table growth and comply with data retention policies.

**Why this priority**: Important for production deployments but the feature is usable without it. Manual cleanup via DELETE statements is a fallback.

**Independent Test**: Can be tested by inserting old records and calling the cleanup function, then verifying old records are deleted while recent records remain.

**Acceptance Scenarios**:

1. **Given** `walrus.history_retention_days` is set to 7, **When** `walrus.cleanup_history()` is called, **Then** all records older than 7 days are deleted.

2. **Given** records exist both older and newer than the retention period, **When** cleanup runs, **Then** only records older than the retention period are removed.

3. **Given** the background worker is running, **When** the worker completes a monitoring cycle, **Then** it calls the cleanup function to purge expired records.

---

### User Story 4 - Compliance Audit Export (Priority: P3)

A compliance officer needs to export sizing history for audit purposes. They can query the history table and export results to their preferred format.

**Why this priority**: Value-added use case that leverages the core functionality. Standard PostgreSQL export tools (COPY, pg_dump) provide this capability without additional implementation.

**Independent Test**: Can be tested by querying history and using `COPY ... TO` to export data. Validates that the schema supports compliance workflows.

**Acceptance Scenarios**:

1. **Given** history records exist, **When** running `COPY (SELECT * FROM walrus.history WHERE timestamp >= '2025-01-01') TO '/tmp/audit.csv' WITH CSV HEADER`, **Then** a valid CSV file is produced.

2. **Given** the metadata JSONB column contains algorithm details, **When** exporting for audit, **Then** the JSONB is preserved in the export format.

---

### Edge Cases

- What happens when the history table is dropped or corrupted? The worker logs a warning and continues operating without history logging.
- How does the system handle concurrent inserts from multiple background workers (if somehow started)? Standard PostgreSQL SERIALIZABLE isolation or row-level locking prevents conflicts.
- What happens when disk space is exhausted during history insert? The worker logs an error and continues its primary sizing function.
- How does cleanup handle a very large history table? Uses indexed timestamp column for efficient DELETE with batching if needed.
- What happens if cleanup is called while inserts are happening? PostgreSQL MVCC ensures consistency - concurrent operations do not block each other.
- What happens when history_retention_days is set to 0? All records are eligible for cleanup - table is effectively cleared on each cleanup call.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST create a `walrus.history` table in the walrus schema during extension installation.
- **FR-002**: System MUST insert a history record for every max_wal_size increase event with action='increase'.
- **FR-003**: System MUST insert a history record for every max_wal_size decrease event with action='decrease'.
- **FR-004**: System MUST insert a history record when a resize is capped at walrus.max with action='capped'.
- **FR-005**: System MUST NOT insert history records when no sizing action is taken (delta below threshold and no shrink).
- **FR-006**: Each history record MUST capture: timestamp, action type, old_size_mb, new_size_mb, forced_checkpoints count, checkpoint_timeout_sec, optional reason text, and optional metadata JSONB.
- **FR-007**: System MUST provide a GUC parameter `walrus.history_retention_days` (integer, default: 7, range: 0-3650) to configure retention period.
- **FR-008**: System MUST provide a SQL function `walrus.cleanup_history()` that deletes records older than the retention period.
- **FR-009**: System MUST call `walrus.cleanup_history()` from the background worker after each monitoring cycle.
- **FR-010**: System MUST create an index on the timestamp column for efficient range queries and cleanup operations.
- **FR-011**: System MUST handle history table insert failures gracefully by logging a warning and continuing normal operation.
- **FR-012**: The metadata JSONB column MUST be optional (nullable) and store algorithm-specific details such as shrink_factor, quiet_intervals count, or growth multiplier.

### Key Entities

- **History Record**: Represents a single sizing decision event. Contains timestamp of decision, action type (increase/decrease/capped), size values before and after, checkpoint statistics at decision time, and optional diagnostic metadata.
- **Retention Configuration**: The `walrus.history_retention_days` GUC that determines how long history records are preserved before automatic cleanup.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: All sizing decisions (increase, decrease, capped) are recorded within 1 second of the decision.
- **SC-002**: History queries by timestamp range complete in under 100ms for tables with up to 1 million records.
- **SC-003**: Automatic cleanup removes expired records within one monitoring cycle after expiration.
- **SC-004**: The history table grows by at most one record per checkpoint_timeout interval under sustained load.
- **SC-005**: History logging adds less than 1ms overhead to each monitoring cycle.
- **SC-006**: DBAs can trace any max_wal_size change back to its cause via history records.

## Assumptions

- The walrus schema already exists or is created by the core extension (feature 001).
- The background worker has SPI access to insert records into the history table.
- PostgreSQL's standard MVCC provides sufficient concurrency control for history table access.
- The timestamp column uses TIMESTAMPTZ for timezone-aware storage.
- The BIGSERIAL primary key provides sufficient range for high-frequency environments.
- History inserts use a dedicated SPI transaction to isolate from worker's main transaction.
