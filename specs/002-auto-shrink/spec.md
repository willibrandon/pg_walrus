# Feature Specification: Auto-Shrink

**Feature Branch**: `002-auto-shrink`
**Created**: 2025-12-30
**Status**: Draft
**Input**: Automatically decrease `max_wal_size` when workload decreases, preventing permanent storage growth.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Automatic Storage Reclamation After Workload Spike (Priority: P1)

A PostgreSQL database experiences periodic high-write workloads (e.g., nightly batch imports) that trigger the grow functionality to increase `max_wal_size`. After the workload completes, the database returns to normal low-write activity. Without shrinking, `max_wal_size` remains elevated indefinitely, wasting storage.

With auto-shrink enabled, the extension monitors consecutive "quiet intervals" (checkpoint intervals where forced checkpoints stay below the threshold). After the configured number of quiet intervals, it automatically reduces `max_wal_size` by the shrink factor, reclaiming storage while maintaining a floor to prevent over-shrinking.

**Why this priority**: Core value proposition - prevents permanent storage bloat from transient workload spikes

**Independent Test**: Simulate workload spike (grow max_wal_size), then verify shrink occurs after configured quiet intervals. Verify via SHOW max_wal_size and log messages.

**Acceptance Scenarios**:

1. **Given** max_wal_size was grown to 4GB and 5 consecutive checkpoint intervals pass with 0 forced checkpoints, **When** the background worker evaluates the shrink condition, **Then** max_wal_size is reduced by shrink_factor (default: 75%) to 3GB
2. **Given** max_wal_size is at 2GB with shrink_intervals=3 and 2 quiet intervals have passed, **When** the 3rd quiet interval completes, **Then** max_wal_size is reduced to 1.5GB
3. **Given** max_wal_size is at 1GB (exactly min_size), **When** quiet intervals exceed shrink_intervals threshold, **Then** max_wal_size remains at 1GB (no shrink below floor)
4. **Given** max_wal_size is at 1.2GB and min_size is 1GB with shrink_factor=0.75, **When** shrink triggers, **Then** max_wal_size is reduced to 1GB (clamped to min_size, not 0.9GB)

---

### User Story 2 - Shrink Respects Minimum Floor (Priority: P1)

A DBA configures `walrus.min_size` to ensure `max_wal_size` never drops below a safe baseline for their workload. The extension enforces this floor regardless of how many quiet intervals occur.

**Why this priority**: Safety feature - prevents over-shrinking that could cause performance problems

**Independent Test**: Set min_size above shrink target, verify shrink clamps at min_size rather than going below.

**Acceptance Scenarios**:

1. **Given** min_size=2GB and max_wal_size=2.5GB with shrink_factor=0.75, **When** shrink triggers, **Then** max_wal_size becomes 2GB (clamped to min_size, not 1.875GB)
2. **Given** min_size=1GB and max_wal_size=1GB, **When** shrink_intervals pass, **Then** no ALTER SYSTEM is executed (already at floor)
3. **Given** min_size=3GB is changed to 4GB while max_wal_size=3.5GB, **When** next shrink check occurs, **Then** no shrink happens (current size below new floor)

---

### User Story 3 - Quiet Interval Counter Resets on Activity (Priority: P1)

When forced checkpoints occur (indicating renewed write activity), the quiet interval counter resets to zero. This prevents inappropriate shrinking during fluctuating workloads.

**Why this priority**: Correctness - ensures shrink only happens after sustained quiet periods

**Independent Test**: Accumulate quiet intervals, trigger forced checkpoint, verify counter resets.

**Acceptance Scenarios**:

1. **Given** 4 quiet intervals have accumulated (shrink_intervals=5), **When** a forced checkpoint occurs in interval 5, **Then** the quiet interval counter resets to 0
2. **Given** quiet_intervals=0 and threshold=2 and 1 forced checkpoint occurs, **When** the interval completes, **Then** quiet_intervals remains 0 (activity below threshold still counts as "not quiet")
3. **Given** quiet_intervals=3 and threshold=2 and 3 forced checkpoints occur, **When** the interval completes, **Then** quiet_intervals resets to 0

---

### User Story 4 - Disable Shrinking While Keeping Grow Enabled (Priority: P2)

A DBA wants automatic growth but prefers manual control over shrinking. The `walrus.shrink_enable` GUC allows independent control.

**Why this priority**: Operational flexibility - some DBAs prefer conservative auto-sizing

**Independent Test**: Set shrink_enable=false, verify shrink never occurs regardless of quiet intervals.

**Acceptance Scenarios**:

1. **Given** shrink_enable=false and 10 quiet intervals pass, **When** the background worker runs, **Then** no shrink occurs
2. **Given** shrink_enable=true and 5 quiet intervals pass, **When** shrink_enable is changed to false via ALTER SYSTEM + pg_reload_conf(), **Then** next cycle does not shrink
3. **Given** shrink_enable=false and walrus.enable=true, **When** forced checkpoints exceed threshold, **Then** grow still functions normally

---

### User Story 5 - Configure Shrink Aggressiveness (Priority: P2)

A DBA adjusts shrink_factor and shrink_intervals to match their workload patterns. Lower shrink_factor (e.g., 0.5) shrinks more aggressively; higher shrink_intervals requires longer quiet periods before shrinking.

**Why this priority**: Tunability - different workloads need different shrink characteristics

**Independent Test**: Configure non-default values, verify shrink behavior matches configuration.

**Acceptance Scenarios**:

1. **Given** shrink_factor=0.5 and max_wal_size=4GB, **When** shrink triggers, **Then** max_wal_size becomes 2GB
2. **Given** shrink_intervals=10 and 9 quiet intervals pass, **When** interval 10 completes quietly, **Then** shrink triggers
3. **Given** shrink_intervals=10 and 9 quiet intervals pass, **When** interval 10 has forced checkpoints, **Then** counter resets and no shrink occurs

---

### User Story 6 - Logging Shrink Events (Priority: P2)

Shrink events are logged at the same level as grow events, allowing DBAs to audit max_wal_size changes over time.

**Why this priority**: Observability - DBAs need visibility into automatic adjustments

**Independent Test**: Trigger shrink, verify LOG message appears with old and new size.

**Acceptance Scenarios**:

1. **Given** shrink triggers from 4GB to 3GB, **When** the shrink completes, **Then** a LOG message shows "pg_walrus: shrinking max_wal_size from 4096 MB to 3072 MB"
2. **Given** shrink is clamped by min_size, **When** shrink completes, **Then** log shows final clamped value, not calculated value
3. **Given** shrink would occur but ALTER SYSTEM fails, **When** the error is caught, **Then** a WARNING is logged

---

### Edge Cases

- **Shrink factor rounding**: When shrink calculation produces a non-integer MB value, round up to nearest MB to avoid under-sizing
- **Shrink factor boundaries**: shrink_factor must be > 0.0 and < 1.0 (values >= 1.0 would grow, = 0.0 would shrink to zero)
- **min_size exceeds current max_wal_size**: If DBA sets min_size higher than current max_wal_size, no shrink (and potentially a warning)
- **Concurrent configuration changes**: shrink_enable changed mid-cycle via SIGHUP should take effect on next iteration
- **PostgreSQL restart resets counter**: Quiet interval counter is in-memory state; restart resets to 0 (conservative behavior)
- **Shrink during SIGHUP suppression**: Self-triggered SIGHUP from grow should not reset quiet interval counter inappropriately
- **Shrink to exactly min_size**: When calculated shrink value is below min_size, clamp to min_size exactly
- **Integer overflow in shrink calculation**: shrink_factor * current_size should use safe floating-point arithmetic

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST track consecutive checkpoint intervals where forced checkpoints are below `walrus.threshold` ("quiet intervals")
- **FR-002**: System MUST reset the quiet interval counter to zero when forced checkpoints >= threshold in any interval
- **FR-003**: System MUST shrink `max_wal_size` when quiet_intervals >= `walrus.shrink_intervals` AND current max_wal_size > `walrus.min_size`
- **FR-004**: System MUST calculate new size as: `max(current_size * walrus.shrink_factor, walrus.min_size)` rounded up to nearest MB
- **FR-005**: System MUST NOT shrink below `walrus.min_size` under any circumstances
- **FR-006**: System MUST reset quiet interval counter after executing a shrink
- **FR-007**: System MUST log shrink events at LOG level with format: "pg_walrus: shrinking max_wal_size from X MB to Y MB"
- **FR-008**: System MUST execute shrink via ALTER SYSTEM + SIGHUP to postmaster (same mechanism as grow)
- **FR-009**: System MUST suppress self-triggered SIGHUP handling for shrink events (same as grow)
- **FR-010**: System MUST evaluate shrink condition after evaluating grow condition (shrink happens only if grow did not trigger)
- **FR-011**: System MUST skip shrink evaluation entirely when `walrus.shrink_enable` is false
- **FR-012**: System MUST handle SIGHUP to reload shrink-related GUC values at runtime
- **FR-013**: System MUST use saturating/checked arithmetic to prevent overflow in shrink calculations

### GUC Requirements

- **GR-001**: `walrus.shrink_enable` (bool, default: true) - Enable/disable automatic shrinking; context: SIGHUP
- **GR-002**: `walrus.shrink_factor` (real, default: 0.75) - Multiplier when shrinking; context: SIGHUP; valid range: (0.0, 1.0) exclusive
- **GR-003**: `walrus.shrink_intervals` (int, default: 5) - Quiet intervals before shrinking; context: SIGHUP; valid range: [1, 1000]
- **GR-004**: `walrus.min_size` (int, default: 1024 MB = 1GB) - Minimum max_wal_size floor; context: SIGHUP; valid range: [2, i32::MAX] MB; unit: MB

### Key Entities

- **Quiet Interval Counter**: In-memory i32 tracking consecutive intervals with forced checkpoints < threshold; reset on shrink, grow, or activity
- **Shrink Decision State**: Boolean determined per-cycle: shrink_enable AND quiet_intervals >= shrink_intervals AND current_size > min_size
- **GUC Settings**: Four new static GucSetting variables for shrink parameters (registered alongside existing grow parameters)

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: After workload spike subsides, max_wal_size returns to near-baseline within (shrink_intervals * checkpoint_timeout) time
- **SC-002**: max_wal_size never drops below walrus.min_size regardless of quiet interval count
- **SC-003**: Storage utilization for WAL stabilizes at efficient levels rather than growing monotonically
- **SC-004**: All shrink events are logged, allowing post-hoc audit via PostgreSQL logs
- **SC-005**: DBAs can disable shrink independently of grow, maintaining operational flexibility
- **SC-006**: Shrink calculations complete within the same cycle time as current grow logic (no noticeable latency impact)
- **SC-007**: Extension passes all existing tests plus new shrink-specific tests across PostgreSQL 15-18

## Assumptions

- Quiet interval counter is ephemeral (in-memory); PostgreSQL restart resets it to zero. This is acceptable because a restart typically indicates operational changes that warrant a fresh baseline.
- shrink_factor is applied as floating-point multiplication, then result is rounded up to nearest integer MB.
- The grow and shrink logic are mutually exclusive per cycle: if grow triggers, shrink is not evaluated; if grow does not trigger, shrink may be evaluated.
- The existing ALTER SYSTEM + SIGHUP mechanism is reused without modification for shrink operations.
- Log messages use the same "pg_walrus:" prefix as existing messages for consistent filtering.
