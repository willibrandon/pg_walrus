# pg_walrus Enhancement Proposal

## New Features for the Rust Rewrite

This document outlines enhancements that leverage Rust's capabilities to significantly improve pg_walrus beyond the original pg_walsizer C implementation.

> **pg_walrus** = WAL + Rust

---

## Table of Contents

1. [Current Limitations](#1-current-limitations)
2. [Tier 1: High-Value Enhancements](#2-tier-1-high-value-enhancements)
3. [Tier 2: Medium-Value Enhancements](#3-tier-2-medium-value-enhancements)
4. [Tier 3: Advanced Features](#4-tier-3-advanced-features)
5. [Implementation Complexity Analysis](#5-implementation-complexity-analysis)
6. [Recommended Enhancement Package](#6-recommended-enhancement-package)

---

## 1. Current Limitations

The existing C implementation has several limitations that could be addressed:

| Limitation | Impact | Difficulty to Fix in C |
|------------|--------|------------------------|
| **No decrease logic** | WAL storage grows permanently | Medium |
| **No observability** | Cannot query extension state via SQL | Medium |
| **Simple algorithm** | May overshoot or undershoot optimal size | High |
| **No history tracking** | Cannot audit past decisions | High |
| **No dry-run mode** | Cannot test without making changes | Low |
| **No rate limiting** | Could thrash on unstable workloads | Medium |
| **No notifications** | Must monitor logs manually | Medium |
| **Integer overflow possible** | Edge case crashes | Low |
| **No workload patterns** | Reacts same to batch jobs vs steady load | High |

---

## 2. Tier 1: High-Value Enhancements

### 2.1 Automatic Size Decrease (Auto-Shrink)

**Problem**: Current implementation only increases `max_wal_size`, leading to permanent storage growth even after workload decreases.

**Solution**: Implement gradual decrease when forced checkpoints stay below threshold for sustained periods.

```rust
/// Configuration for auto-shrink behavior
pub static WALRUS_SHRINK_ENABLE: GucSetting<bool> = GucSetting::new(true);
pub static WALRUS_SHRINK_FACTOR: GucSetting<f64> = GucSetting::new(0.75); // Shrink to 75%
pub static WALRUS_SHRINK_INTERVALS: GucSetting<i32> = GucSetting::new(5);  // After 5 quiet intervals

/// State tracking for shrink decisions
struct ShrinkState {
    quiet_intervals: u32,           // Consecutive intervals below threshold
    last_forced_checkpoints: i64,   // For trend analysis
    high_water_mark: i32,           // Peak size reached
}

impl ShrinkState {
    fn should_shrink(&self, current_size: i32, min_size: i32) -> Option<i32> {
        if !WALRUS_SHRINK_ENABLE.get() {
            return None;
        }

        let intervals_required = WALRUS_SHRINK_INTERVALS.get() as u32;
        if self.quiet_intervals < intervals_required {
            return None;
        }

        let factor = WALRUS_SHRINK_FACTOR.get();
        let new_size = ((current_size as f64) * factor) as i32;

        // Don't shrink below minimum or PostgreSQL default
        let floor = std::cmp::max(min_size, 1024); // At least 1GB
        if new_size < floor {
            return None;
        }

        // Don't shrink if already at or below target
        if new_size >= current_size {
            return None;
        }

        Some(new_size)
    }
}
```

**New GUCs:**
- `walrus.shrink_enable` (bool, default: true) - Enable automatic shrinking
- `walrus.shrink_factor` (real, default: 0.75) - Multiply by this factor when shrinking
- `walrus.shrink_intervals` (int, default: 5) - Quiet intervals before shrinking
- `walrus.min_size` (int, default: 1024MB) - Never shrink below this

**Value**: Prevents permanent storage growth, saves disk space

---

### 2.2 SQL Observability Functions

**Problem**: No way to query extension state without parsing logs.

**Solution**: Expose SQL functions for monitoring and management.

```rust
/// Get current extension status as JSON
#[pg_extern]
fn walrus_status() -> pgrx::JsonB {
    let status = json!({
        "enabled": WALRUS_ENABLE.get(),
        "current_max_wal_size_mb": get_max_wal_size_mb(),
        "configured_maximum_mb": WALRUS_MAX.get(),
        "threshold": WALRUS_THRESHOLD.get(),
        "checkpoint_timeout_sec": get_checkpoint_timeout(),
        "worker_running": is_worker_running(),
        "last_check_time": get_last_check_time(),
        "last_adjustment_time": get_last_adjustment_time(),
        "total_adjustments": get_total_adjustments(),
        "shrink_enabled": WALRUS_SHRINK_ENABLE.get(),
        "quiet_intervals": get_quiet_interval_count(),
    });
    pgrx::JsonB(status)
}

/// Get adjustment history
#[pg_extern]
fn walrus_history() -> TableIterator<'static, (
    name!(timestamp, TimestampWithTimeZone),
    name!(action, String),
    name!(old_size_mb, i32),
    name!(new_size_mb, i32),
    name!(forced_checkpoints, i64),
    name!(reason, String),
)> {
    let history = get_adjustment_history();
    TableIterator::new(history.into_iter())
}

/// Get current recommendation without applying
#[pg_extern]
fn walrus_recommendation() -> pgrx::JsonB {
    let recommendation = calculate_recommendation();
    pgrx::JsonB(json!({
        "current_size_mb": recommendation.current,
        "recommended_size_mb": recommendation.recommended,
        "action": recommendation.action, // "increase", "decrease", "none"
        "reason": recommendation.reason,
        "confidence": recommendation.confidence,
    }))
}

/// Manually trigger an analysis cycle
#[pg_extern]
fn walrus_analyze() -> pgrx::JsonB {
    // Trigger immediate analysis
    let result = perform_analysis_cycle();
    pgrx::JsonB(json!({
        "analyzed": true,
        "recommendation": result.recommendation,
        "applied": result.applied,
    }))
}

/// Reset extension state (clear history, counters)
#[pg_extern]
fn walrus_reset() -> bool {
    reset_extension_state();
    true
}
```

**Value**: Enables monitoring dashboards, alerting, debugging

---

### 2.3 Event History Table

**Problem**: No audit trail of what changes were made and why.

**Solution**: Log all decisions to a table for analysis.

```rust
extension_sql!(
    r#"
    CREATE TABLE IF NOT EXISTS walrus.history (
        id BIGSERIAL PRIMARY KEY,
        timestamp TIMESTAMPTZ NOT NULL DEFAULT now(),
        action TEXT NOT NULL,  -- 'increase', 'decrease', 'no_change', 'capped'
        old_size_mb INTEGER NOT NULL,
        new_size_mb INTEGER NOT NULL,
        forced_checkpoints BIGINT NOT NULL,
        checkpoint_timeout_sec INTEGER NOT NULL,
        reason TEXT,
        metadata JSONB
    );

    CREATE INDEX ON walrus.history (timestamp);

    -- Automatic cleanup of old history (keep 30 days by default)
    CREATE OR REPLACE FUNCTION walrus.cleanup_history() RETURNS void AS $$
    DELETE FROM walrus.history
    WHERE timestamp < now() - interval '30 days';
    $$ LANGUAGE SQL;
    "#,
    name = "history_table"
);

/// Log an adjustment to the history table
fn log_adjustment(event: &AdjustmentEvent) -> Result<(), spi::Error> {
    Spi::connect(|client| {
        client.update(
            "INSERT INTO walrus.history
             (action, old_size_mb, new_size_mb, forced_checkpoints,
              checkpoint_timeout_sec, reason, metadata)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
            None,
            Some(vec![
                (PgBuiltInOids::TEXTOID.oid(), event.action.clone().into_datum()),
                (PgBuiltInOids::INT4OID.oid(), event.old_size.into_datum()),
                (PgBuiltInOids::INT4OID.oid(), event.new_size.into_datum()),
                (PgBuiltInOids::INT8OID.oid(), event.forced_checkpoints.into_datum()),
                (PgBuiltInOids::INT4OID.oid(), event.timeout.into_datum()),
                (PgBuiltInOids::TEXTOID.oid(), event.reason.clone().into_datum()),
                (PgBuiltInOids::JSONBOID.oid(), JsonB(event.metadata.clone()).into_datum()),
            ]),
        )?;
        Ok(())
    })
}
```

**New GUCs:**
- `walrus.history_retention_days` (int, default: 30) - Days to keep history

**Value**: Audit compliance, trend analysis, debugging

---

### 2.4 Dry-Run Mode

**Problem**: Cannot test behavior without making actual changes.

**Solution**: Mode that logs recommendations without applying them.

```rust
pub static WALRUS_DRY_RUN: GucSetting<bool> = GucSetting::new(false);

fn apply_new_wal_size(new_size: i32, reason: &str) -> Result<bool, WalrusError> {
    let current_size = get_max_wal_size_mb();

    if WALRUS_DRY_RUN.get() {
        log!(
            "pg_walrus [DRY-RUN]: would change max_wal_size from {} MB to {} MB ({})",
            current_size,
            new_size,
            reason
        );

        // Log to history with dry-run flag
        log_adjustment(&AdjustmentEvent {
            action: "dry_run".to_string(),
            old_size: current_size,
            new_size,
            reason: reason.to_string(),
            metadata: json!({"dry_run": true}),
            ..Default::default()
        })?;

        return Ok(false); // Indicates no change was made
    }

    // Actually apply the change
    alter_system_max_wal_size(new_size)?;
    signal_config_reload()?;

    log_adjustment(&AdjustmentEvent {
        action: if new_size > current_size { "increase" } else { "decrease" }.to_string(),
        old_size: current_size,
        new_size,
        reason: reason.to_string(),
        metadata: json!({"applied": true}),
        ..Default::default()
    })?;

    Ok(true)
}
```

**Value**: Safe testing in production, validation before enabling

---

## 3. Tier 2: Medium-Value Enhancements

### 3.1 Smarter Sizing Algorithms

**Problem**: Simple multiplication may overshoot or undershoot optimal size.

**Solution**: Multiple algorithm options with adaptive learning.

```rust
#[derive(PostgresGucEnum, Clone, Copy, PartialEq, Debug)]
pub enum SizingAlgorithm {
    /// Original: new_size = current * (checkpoints + 1)
    Multiplicative,
    /// Add fixed increment per checkpoint: new_size = current + (checkpoints * increment)
    Additive,
    /// Exponential moving average of WAL generation rate
    Adaptive,
    /// Target specific percentile of historical usage
    Percentile,
}

pub static WALRUS_ALGORITHM: GucSetting<SizingAlgorithm> =
    GucSetting::new(SizingAlgorithm::Multiplicative);

/// Adaptive algorithm with exponential moving average
struct AdaptiveState {
    ema_wal_rate: f64,           // Exponential moving average of WAL MB/sec
    ema_alpha: f64,              // Smoothing factor (0.1 = slow adaptation)
    samples: Vec<WalSample>,     // Recent samples for percentile calc
}

impl AdaptiveState {
    fn calculate_optimal_size(&mut self, new_sample: WalSample) -> i32 {
        // Update EMA
        let wal_rate = new_sample.wal_generated_mb as f64 / new_sample.duration_sec as f64;
        self.ema_wal_rate = self.ema_alpha * wal_rate + (1.0 - self.ema_alpha) * self.ema_wal_rate;

        // Calculate size to hold checkpoint_timeout worth of WAL with 50% headroom
        let timeout = get_checkpoint_timeout() as f64;
        let estimated_wal = self.ema_wal_rate * timeout;
        let with_headroom = estimated_wal * 1.5;

        with_headroom.ceil() as i32
    }
}

/// Percentile-based sizing
fn calculate_percentile_size(samples: &[WalSample], percentile: f64) -> i32 {
    if samples.is_empty() {
        return get_max_wal_size_mb();
    }

    let mut sizes: Vec<f64> = samples.iter()
        .map(|s| s.wal_generated_mb as f64)
        .collect();
    sizes.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let idx = ((percentile / 100.0) * (sizes.len() - 1) as f64) as usize;
    (sizes[idx] * 1.2).ceil() as i32 // 20% headroom on percentile
}
```

**New GUCs:**
- `walrus.algorithm` (enum, default: multiplicative) - Sizing algorithm
- `walrus.ema_alpha` (real, default: 0.2) - EMA smoothing factor
- `walrus.target_percentile` (real, default: 95) - For percentile algorithm

**Value**: Better sizing accuracy, less overshoot, workload adaptation

---

### 3.2 Rate Limiting / Cooldown

**Problem**: Unstable workloads could cause thrashing (rapid size changes).

**Solution**: Enforce minimum time between adjustments.

```rust
pub static WALRUS_COOLDOWN_SEC: GucSetting<i32> = GucSetting::new(300); // 5 minutes
pub static WALRUS_MAX_CHANGES_PER_HOUR: GucSetting<i32> = GucSetting::new(4);

struct RateLimiter {
    last_change: Option<Instant>,
    changes_this_hour: u32,
    hour_start: Instant,
}

impl RateLimiter {
    fn can_change(&mut self) -> bool {
        let now = Instant::now();

        // Reset hourly counter if needed
        if now.duration_since(self.hour_start) > Duration::from_secs(3600) {
            self.changes_this_hour = 0;
            self.hour_start = now;
        }

        // Check hourly limit
        if self.changes_this_hour >= WALRUS_MAX_CHANGES_PER_HOUR.get() as u32 {
            debug1!("pg_walrus: hourly change limit reached");
            return false;
        }

        // Check cooldown
        if let Some(last) = self.last_change {
            let cooldown = Duration::from_secs(WALRUS_COOLDOWN_SEC.get() as u64);
            if now.duration_since(last) < cooldown {
                debug1!("pg_walrus: cooldown period active");
                return false;
            }
        }

        true
    }

    fn record_change(&mut self) {
        self.last_change = Some(Instant::now());
        self.changes_this_hour += 1;
    }
}
```

**Value**: Prevents thrashing, improves stability

---

### 3.3 NOTIFY/LISTEN Integration

**Problem**: Must poll logs or tables to detect changes.

**Solution**: Send PostgreSQL notifications on events.

```rust
pub static WALRUS_NOTIFY_CHANNEL: GucSetting<Option<&'static CStr>> =
    GucSetting::new(Some(c"walrus_events"));

fn notify_adjustment(event: &AdjustmentEvent) -> Result<(), spi::Error> {
    let channel = match WALRUS_NOTIFY_CHANNEL.get() {
        Some(c) => c.to_str().unwrap_or("walrus_events"),
        None => return Ok(()), // Notifications disabled
    };

    let payload = serde_json::to_string(&json!({
        "event": "adjustment",
        "action": event.action,
        "old_size_mb": event.old_size,
        "new_size_mb": event.new_size,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    })).unwrap_or_default();

    Spi::connect(|client| {
        client.update(
            &format!("NOTIFY {}, '{}'", channel, payload.replace('\'', "''")),
            None,
            None,
        )?;
        Ok(())
    })
}

// Client-side usage:
// LISTEN walrus_events;
// -- Then receive: {"event":"adjustment","action":"increase","old_size_mb":1024,...}
```

**Value**: Real-time alerting, integration with monitoring systems

---

### 3.4 Prometheus Metrics Export

**Problem**: No standard metrics format for monitoring integration.

**Solution**: Expose metrics in Prometheus format.

```rust
/// Metrics state (updated atomically)
static METRICS: Lazy<Metrics> = Lazy::new(|| Metrics::default());

#[derive(Default)]
struct Metrics {
    total_increases: AtomicU64,
    total_decreases: AtomicU64,
    total_analysis_cycles: AtomicU64,
    last_forced_checkpoints: AtomicI64,
    current_recommended_size: AtomicI32,
}

#[pg_extern]
fn walrus_metrics_prometheus() -> String {
    format!(
        r#"# HELP walrus_max_wal_size_bytes Current max_wal_size setting
# TYPE walrus_max_wal_size_bytes gauge
walrus_max_wal_size_bytes {{}} {}

# HELP walrus_max_allowed_bytes Maximum allowed by walrus.max
# TYPE walrus_max_allowed_bytes gauge
walrus_max_allowed_bytes {{}} {}

# HELP walrus_increases_total Total number of size increases
# TYPE walrus_increases_total counter
walrus_increases_total {{}} {}

# HELP walrus_decreases_total Total number of size decreases
# TYPE walrus_decreases_total counter
walrus_decreases_total {{}} {}

# HELP walrus_analysis_cycles_total Total analysis cycles run
# TYPE walrus_analysis_cycles_total counter
walrus_analysis_cycles_total {{}} {}

# HELP walrus_forced_checkpoints Last observed forced checkpoint count
# TYPE walrus_forced_checkpoints gauge
walrus_forced_checkpoints {{}} {}

# HELP walrus_enabled Whether auto-sizing is enabled
# TYPE walrus_enabled gauge
walrus_enabled {{}} {}
"#,
        get_max_wal_size_mb() as i64 * 1024 * 1024,
        WALRUS_MAX.get() as i64 * 1024 * 1024,
        METRICS.total_increases.load(Ordering::Relaxed),
        METRICS.total_decreases.load(Ordering::Relaxed),
        METRICS.total_analysis_cycles.load(Ordering::Relaxed),
        METRICS.last_forced_checkpoints.load(Ordering::Relaxed),
        if WALRUS_ENABLE.get() { 1 } else { 0 },
    )
}
```

**Value**: Standard monitoring integration, dashboards, alerting

---

## 4. Tier 3: Advanced Features

### 4.1 Time-Based Profiles

**Problem**: Different workloads at different times (OLTP during day, batch at night).

**Solution**: Schedule-aware configuration.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TimeProfile {
    name: String,
    start_hour: u8,      // 0-23
    end_hour: u8,        // 0-23
    days: Vec<Weekday>,  // Mon-Sun
    max_size: i32,
    threshold: i32,
    shrink_enable: bool,
}

pub static WALRUS_PROFILES: GucSetting<Option<&'static CStr>> = GucSetting::new(None);

// Example configuration (JSON in postgresql.conf):
// walrus.profiles = '[
//   {"name":"business_hours","start_hour":9,"end_hour":18,"days":["Mon","Tue","Wed","Thu","Fri"],"max_size":8192,"threshold":2,"shrink_enable":false},
//   {"name":"batch_window","start_hour":22,"end_hour":6,"days":["Mon","Tue","Wed","Thu","Fri"],"max_size":32768,"threshold":5,"shrink_enable":false},
//   {"name":"weekend","start_hour":0,"end_hour":24,"days":["Sat","Sun"],"max_size":4096,"threshold":3,"shrink_enable":true}
// ]'

fn get_active_profile() -> Option<TimeProfile> {
    let profiles_json = WALRUS_PROFILES.get()?;
    let profiles: Vec<TimeProfile> = serde_json::from_str(
        profiles_json.to_str().ok()?
    ).ok()?;

    let now = chrono::Local::now();
    let current_hour = now.hour() as u8;
    let current_day = now.weekday();

    profiles.into_iter().find(|p| {
        p.days.contains(&current_day) &&
        is_hour_in_range(current_hour, p.start_hour, p.end_hour)
    })
}
```

**Value**: Optimized behavior for different workload patterns

---

### 4.2 WAL Generation Rate Tracking

**Problem**: Only reacts to forced checkpoints, not proactive sizing.

**Solution**: Monitor actual WAL generation rate for predictive sizing.

```rust
/// Track WAL generation between checks
struct WalRateTracker {
    last_lsn: Option<pg_sys::XLogRecPtr>,
    last_time: Option<Instant>,
    rate_samples: VecDeque<f64>,  // MB/sec samples
    max_samples: usize,
}

impl WalRateTracker {
    fn update(&mut self) -> Option<f64> {
        let current_lsn = unsafe { pg_sys::GetXLogWriteRecPtr() };
        let now = Instant::now();

        let rate = if let (Some(last_lsn), Some(last_time)) = (self.last_lsn, self.last_time) {
            let bytes_written = (current_lsn - last_lsn) as f64;
            let seconds = now.duration_since(last_time).as_secs_f64();
            if seconds > 0.0 {
                let mb_per_sec = bytes_written / (1024.0 * 1024.0) / seconds;
                self.rate_samples.push_back(mb_per_sec);
                if self.rate_samples.len() > self.max_samples {
                    self.rate_samples.pop_front();
                }
                Some(mb_per_sec)
            } else {
                None
            }
        } else {
            None
        };

        self.last_lsn = Some(current_lsn);
        self.last_time = Some(now);
        rate
    }

    fn predict_size_needed(&self, seconds_ahead: f64) -> i32 {
        if self.rate_samples.is_empty() {
            return get_max_wal_size_mb();
        }

        // Use 95th percentile of observed rates
        let mut sorted: Vec<f64> = self.rate_samples.iter().copied().collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let p95_idx = (sorted.len() as f64 * 0.95) as usize;
        let p95_rate = sorted.get(p95_idx).copied().unwrap_or(0.0);

        // Predict WAL needed with headroom
        let predicted_mb = p95_rate * seconds_ahead * 1.3; // 30% headroom
        predicted_mb.ceil() as i32
    }
}
```

**Value**: Proactive sizing before forced checkpoints occur

---

### 4.3 Correlation with pg_stat_statements

**Problem**: Don't know which queries cause WAL spikes.

**Solution**: Correlate sizing decisions with top WAL-generating queries.

```rust
#[pg_extern]
fn walrus_top_wal_queries(limit: i32) -> TableIterator<'static, (
    name!(query, String),
    name!(calls, i64),
    name!(wal_bytes, i64),
    name!(wal_bytes_per_call, f64),
)> {
    let results = Spi::connect(|client| {
        client.select(
            "SELECT query, calls, wal_bytes,
                    wal_bytes::float / NULLIF(calls, 0) as wal_per_call
             FROM pg_stat_statements
             WHERE wal_bytes > 0
             ORDER BY wal_bytes DESC
             LIMIT $1",
            None,
            Some(vec![(PgBuiltInOids::INT4OID.oid(), limit.into_datum())]),
        )
    });

    // Convert to iterator
    // ...
}
```

**Value**: Root cause analysis, query optimization guidance

---

### 4.4 Shared Memory State

**Problem**: State lost on worker restart, no inter-process visibility.

**Solution**: Use PostgreSQL shared memory for persistent state.

```rust
use pgrx::shmem::*;

// Shared memory structure
#[derive(Copy, Clone)]
struct WalrusShmem {
    magic: u32,
    total_increases: u64,
    total_decreases: u64,
    last_adjustment_time: i64,
    last_forced_checkpoints: i64,
    current_recommendation: i32,
    worker_pid: i32,
}

unsafe impl PGRXSharedMemory for WalrusShmem {}

static WALRUS_SHMEM: PgSharedMem<WalrusShmem> = PgSharedMem::new();

#[pg_guard]
pub extern "C" fn _PG_init() {
    // Request shared memory
    pg_shmem_init!(WALRUS_SHMEM);

    // ... rest of init
}

// Access from any backend
#[pg_extern]
fn walrus_shmem_status() -> pgrx::JsonB {
    let shmem = WALRUS_SHMEM.get();
    pgrx::JsonB(json!({
        "total_increases": shmem.total_increases,
        "total_decreases": shmem.total_decreases,
        "worker_pid": shmem.worker_pid,
        "current_recommendation": shmem.current_recommendation,
    }))
}
```

**Value**: Persistent state, cross-process visibility, crash recovery

---

## 5. Implementation Complexity Analysis

| Enhancement | Lines of Code | New Dependencies | Risk | Testing Effort |
|-------------|---------------|------------------|------|----------------|
| Auto-Shrink | ~150 | None | Low | Medium |
| SQL Functions | ~200 | None | Low | Medium |
| History Table | ~100 | None | Low | Low |
| Dry-Run Mode | ~50 | None | Very Low | Low |
| Smart Algorithms | ~300 | None | Medium | High |
| Rate Limiting | ~80 | None | Low | Medium |
| NOTIFY Integration | ~60 | None | Low | Low |
| Prometheus Metrics | ~100 | None | Low | Low |
| Time Profiles | ~200 | serde_json, chrono | Medium | Medium |
| WAL Rate Tracking | ~150 | None | Medium | High |
| pg_stat_statements | ~80 | None | Low | Low |
| Shared Memory | ~200 | None | High | High |

---

## 6. Recommended Enhancement Package

### 6.1 "Essential" Package (Recommended for v1.0)

These enhancements provide significant value with low risk:

1. **Auto-Shrink** - Prevents permanent storage growth
2. **SQL Observability Functions** - Essential for production use
3. **History Table** - Audit trail and debugging
4. **Dry-Run Mode** - Safe testing
5. **Rate Limiting** - Stability improvement
6. **NOTIFY Integration** - Real-time awareness

**Total Additional LOC**: ~640
**New GUCs**: 8
**New SQL Functions**: 5

### 6.2 "Professional" Package (v1.1)

Add monitoring integration:

7. **Prometheus Metrics** - Standard monitoring
8. **Smart Algorithms** - Better sizing accuracy

**Total Additional LOC**: ~400
**New GUCs**: 4

### 6.3 "Enterprise" Package (v2.0)

Advanced features for complex deployments:

9. **Time Profiles** - Schedule-aware behavior
10. **WAL Rate Tracking** - Predictive sizing
11. **Shared Memory State** - Crash resilience

**Total Additional LOC**: ~550
**New Dependencies**: serde_json, chrono

---

## 7. Summary: Value Proposition

### Why These Enhancements Justify Rust Conversion

| Enhancement | C Difficulty | Rust Advantage |
|-------------|--------------|----------------|
| Auto-Shrink | Medium (state management) | Easy (struct with methods) |
| SQL Functions | High (manual datum handling) | Easy (`#[pg_extern]` macro) |
| History Table | Medium (SPI complexity) | Easy (type-safe SPI) |
| JSON Output | High (manual string building) | Easy (serde_json) |
| Prometheus | Medium (string formatting) | Easy (format! macro) |
| Smart Algorithms | High (floating point safety) | Easy (Rust numerics) |
| Rate Limiting | Medium (time tracking) | Easy (std::time) |
| Shared Memory | High (manual layout) | Medium (pgrx shmem) |

### Quantified Benefits

| Metric | Current C | Rust + Enhancements |
|--------|-----------|---------------------|
| SQL Functions | 0 | 5+ |
| Configuration Options | 3 | 15+ |
| Monitoring Capabilities | Logs only | Prometheus, NOTIFY, JSON API |
| Sizing Intelligence | Simple multiply | Multiple algorithms |
| Storage Efficiency | Grows only | Auto-shrink |
| Debuggability | Parse logs | History table, SQL queries |
| Testing | Manual | `#[pg_test]` framework |

---

## 8. Conclusion

Rewriting pg_walsizer as pg_walrus using pgrx is justified not just for memory safety and maintainability, but as an opportunity to transform it from a simple utility into a **comprehensive WAL management solution**.

The "Essential" enhancement package alone provides:
- **6 new features** users have been missing
- **5 SQL functions** for monitoring and management
- **8 new configuration options** for fine-tuning
- **Audit trail** for compliance and debugging
- **Safe testing** via dry-run mode

This transforms pg_walrus from a "set and forget" background tool into an **observable, configurable, production-grade WAL management system**.

---

**Document Version**: 1.0
**Date**: December 2025
