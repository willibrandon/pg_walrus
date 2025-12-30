# Feature Specification: Rate Limiting

**Feature Branch**: `006-rate-limiting`
**Created**: 2025-12-30
**Status**: Draft
**Input**: User description: "Prevent thrashing on unstable workloads by enforcing minimum time between adjustments"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Prevent Thrashing During Workload Spikes (Priority: P1)

A database administrator manages a PostgreSQL instance with unpredictable batch workloads that cause rapid checkpoint activity fluctuations. Without rate limiting, pg_walrus makes multiple rapid adjustments in quick succession, causing config churn and unnecessary SIGHUP signals to PostgreSQL.

**Why this priority**: This is the core problem the feature solves. Preventing thrashing during unstable workloads is the primary value proposition and affects operational stability.

**Independent Test**: Can be fully tested by simulating rapid checkpoint threshold exceedances within a short time window and verifying that only the first adjustment is applied while subsequent ones are rate-limited.

**Acceptance Scenarios**:

1. **Given** pg_walrus detects threshold exceedance at T=0 and applies an adjustment, **When** another threshold exceedance is detected at T=60 seconds (within the 300-second default cooldown), **Then** the second adjustment is skipped and a "cooldown active" message is logged.
2. **Given** the cooldown period of 300 seconds has elapsed since the last adjustment, **When** a new threshold exceedance is detected, **Then** the adjustment is applied normally.
3. **Given** pg_walrus is in a cooldown period, **When** `walrus.status()` is called, **Then** the status shows `cooldown_active: true` and `cooldown_remaining_sec` with seconds until cooldown expires.

---

### User Story 2 - Limit Maximum Adjustments Per Hour (Priority: P2)

A system administrator wants to ensure that even if workloads cause repeated threshold exceedances, the system never makes more than a configured number of adjustments per hour to maintain stability and prevent excessive configuration changes.

**Why this priority**: The hourly limit provides a secondary safety net when the cooldown alone is insufficient, particularly during sustained workload instability.

**Independent Test**: Can be fully tested by simulating multiple adjustment triggers spread across the cooldown windows within an hour and verifying the hourly counter correctly blocks adjustments beyond the limit.

**Acceptance Scenarios**:

1. **Given** `walrus.max_changes_per_hour` is set to 4 and 4 adjustments have been made in the current hour, **When** a fifth threshold exceedance is detected, **Then** the adjustment is skipped and "hourly limit reached" is logged.
2. **Given** the first adjustment in the current window was made 59 minutes ago and 4 total adjustments have been made, **When** the 60-minute mark passes, **Then** the hourly counter resets and new adjustments are allowed.
3. **Given** both cooldown and hourly limit would allow an adjustment, **When** a threshold exceedance is detected, **Then** the adjustment proceeds and both counters are updated (cooldown starts, hourly count increments).

---

### User Story 3 - Configure Rate Limiting Parameters (Priority: P3)

A DBA wants to tune the rate limiting behavior to match their specific workload patterns, allowing more frequent adjustments for highly variable workloads or stricter limits for stable environments.

**Why this priority**: Configurability enables the feature to work across diverse PostgreSQL deployments with different operational requirements.

**Independent Test**: Can be fully tested by modifying GUC parameters at runtime and verifying the new values immediately affect rate limiting behavior.

**Acceptance Scenarios**:

1. **Given** `walrus.cooldown_sec` is changed from 300 to 600 via `ALTER SYSTEM`, **When** `pg_reload_conf()` is called, **Then** subsequent rate limit checks use the new 600-second cooldown.
2. **Given** `walrus.max_changes_per_hour` is changed from 4 to 2, **When** a threshold exceedance occurs after 2 adjustments in the hour, **Then** the third adjustment is blocked by the new limit.
3. **Given** a user queries `SHOW walrus.cooldown_sec`, **Then** the current configured value is returned.

---

### User Story 4 - Rate Limiting Observability (Priority: P3)

An operator wants to monitor rate limiting behavior through existing observability functions to understand when adjustments are being blocked and why.

**Why this priority**: Visibility into rate limiting state is essential for troubleshooting and capacity planning, but relies on existing observability infrastructure (feature 004).

**Independent Test**: Can be fully tested by triggering rate-limited scenarios and verifying `walrus.status()` returns accurate rate limiting metrics.

**Acceptance Scenarios**:

1. **Given** an adjustment was blocked due to cooldown, **When** `walrus.status()` is called, **Then** the result includes `cooldown_active: true`, `last_adjustment_time`, and `cooldown_remaining_sec`.
2. **Given** an adjustment was blocked due to hourly limit, **When** `walrus.status()` is called, **Then** the result includes `changes_this_hour` and `hourly_window_start`.
3. **Given** rate limiting blocked an adjustment, **When** `walrus.history()` is queried, **Then** a record with `action='skipped'` and `reason='cooldown active'` or `reason='hourly limit reached'` exists.

---

### Edge Cases

- **What happens when PostgreSQL restarts during a cooldown period?**
  - Rate limiting state is stored in shared memory, which is lost on restart. After restart, the cooldown is effectively reset, allowing immediate adjustments. The hourly counter also resets. This is acceptable because restarts are disruptive events that reset checkpoint baselines anyway.

- **What happens if the cooldown expires exactly when the worker wakes?**
  - The comparison uses strict inequality (`last_change + cooldown > now` means skip). If cooldown has elapsed exactly to the second, the adjustment proceeds.

- **What happens if `cooldown_sec` is set to 0?**
  - Cooldown is disabled; only the hourly limit applies. This is a valid configuration for operators who prefer only hourly limits.

- **What happens if `max_changes_per_hour` is set to 0?**
  - All adjustments are blocked indefinitely until the parameter is changed. This effectively disables automatic adjustments while still allowing manual ones via `walrus.analyze(apply := true)` (which bypasses rate limiting).

- **What happens if dry-run mode is enabled alongside rate limiting?**
  - Dry-run decisions still respect rate limiting (a dry-run is "counted" as an adjustment for rate limiting purposes). This ensures dry-run accurately reflects what would happen in production.

- **What happens if both cooldown and hourly limit are simultaneously triggered?**
  - Cooldown is checked first. If cooldown blocks the adjustment, the hourly counter is not incremented (since no adjustment was attempted). The log message reflects which limit was hit first.

- **What happens when system clock jumps backward (NTP adjustment)?**
  - The implementation uses wall-clock timestamps via `std::time::SystemTime::now()` (consistent with existing `last_adjustment_time` tracking). If the system clock jumps backward, cooldown may appear to extend, but this is a safe failure mode (prevents adjustment thrashing rather than allowing premature adjustments).

- **What happens when `walrus.reset()` is called?**
  - Reset clears all rate limiting state: `last_adjustment_time`, `changes_this_hour`, and `hour_window_start` are all reset to zero. This provides a clean slate for the extension, consistent with reset behavior for other monitoring counters.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST enforce a minimum time interval (cooldown period) between consecutive max_wal_size adjustments.
- **FR-002**: System MUST track the timestamp of the last successful adjustment in shared memory for cooldown calculation.
- **FR-003**: System MUST skip adjustments and log "cooldown active" when the cooldown period has not elapsed since the last adjustment.
- **FR-004**: System MUST limit the maximum number of adjustments within a rolling one-hour window.
- **FR-005**: System MUST track the count of adjustments in the current hour and the start time of that hour window in shared memory.
- **FR-006**: System MUST skip adjustments and log "hourly limit reached" when the hourly limit has been reached.
- **FR-007**: System MUST reset the hourly counter when 60 minutes have elapsed since the hour window started.
- **FR-008**: System MUST provide a GUC parameter `walrus.cooldown_sec` (integer, default: 300) controlling the minimum seconds between adjustments.
- **FR-009**: System MUST provide a GUC parameter `walrus.max_changes_per_hour` (integer, default: 4) controlling the maximum adjustments per hour.
- **FR-010**: Both GUC parameters MUST be configurable at runtime via `ALTER SYSTEM` and `pg_reload_conf()` (GucContext::Sighup).
- **FR-011**: System MUST apply rate limiting to both grow and shrink adjustment paths in the worker loop.
- **FR-012**: System MUST expose rate limiting state (cooldown active, remaining cooldown, hourly count, window start) via `walrus.status()` function.
- **FR-013**: System MUST record skipped adjustments in the history table with action='skipped' and appropriate reason.
- **FR-014**: Rate limiting checks MUST occur before dry-run checks, so dry-run mode accurately reflects rate limiting behavior.
- **FR-015**: Manual adjustments via `walrus.analyze(apply := true)` MUST bypass rate limiting to allow operator intervention.
- **FR-016**: Rate limiting state MUST persist in shared memory across worker wake cycles but MAY be lost on PostgreSQL restart.
- **FR-017**: System MUST validate GUC parameters: `cooldown_sec` range 0-86400 (0 to 24 hours), `max_changes_per_hour` range 0-1000.
- **FR-018**: System MUST log at LOG level when an adjustment is blocked by rate limiting, including which limit was triggered.
- **FR-019**: `walrus.reset()` MUST clear all rate limiting state (last_adjustment_time, changes_this_hour, hour_window_start) to zero.

### Key Entities

- **RateLimiter State**: Stored in shared memory alongside existing WalrusState. Contains:
  - `last_adjustment_time`: Existing field reused for cooldown calculation (i64, 0 = never adjusted)
  - `changes_this_hour`: Count of adjustments in current hour window (i32) - NEW
  - `hour_window_start`: Unix timestamp when current hour window began (i64, 0 = no adjustments yet) - NEW

- **GUC Parameters**: Two new runtime configuration parameters:
  - `walrus.cooldown_sec`: Minimum seconds between adjustments (integer)
  - `walrus.max_changes_per_hour`: Maximum adjustments per rolling hour (integer)

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: When cooldown is active, adjustment attempts are blocked 100% of the time and logged appropriately.
- **SC-002**: When hourly limit is reached, adjustment attempts are blocked 100% of the time and logged appropriately.
- **SC-003**: Rate limiting state is accurately reflected in `walrus.status()` output within one worker cycle of state changes.
- **SC-004**: GUC parameter changes take effect within one worker cycle after `pg_reload_conf()`.
- **SC-005**: Skipped adjustments appear in history table with correct action and reason within the same transaction.
- **SC-006**: With default parameters (300s cooldown, 4/hour limit), the system makes at most 4 adjustments per hour regardless of workload volatility.
- **SC-007**: Manual operator intervention via `walrus.analyze(apply := true)` succeeds regardless of rate limiting state.
- **SC-008**: Zero data races or shared memory corruption when rate limiting state is read by SQL functions while the worker updates it.

## Clarifications

### Session 2025-12-30

- Q: When `walrus.reset()` is called, how should it interact with rate limiting state? → A: Reset clears rate limiting state (cooldown and hourly counters reset to zero)

## Assumptions

- The existing shared memory infrastructure (`shmem::WalrusState`, `PgLwLock`) can be extended to include rate limiting fields without breaking existing functionality.
- The existing history table infrastructure (feature 003) supports the new action type `'skipped'`.
- The existing `walrus.status()` function (feature 004) can be extended to include rate limiting metrics.
- PostgreSQL's internal clock (used via `std::time::SystemTime::now()`) provides sufficient accuracy for second-granularity cooldown tracking.
- The `walrus.analyze(apply := true)` function exists from feature 004 and provides manual adjustment capability.
