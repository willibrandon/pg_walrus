//! SQL-callable functions for pg_walrus observability.
//!
//! This module implements the five SQL functions in the `walrus` schema:
//! - `walrus.status()`: JSONB with extension state
//! - `walrus.history()`: SETOF RECORD with adjustment history
//! - `walrus.recommendation()`: JSONB with sizing recommendation
//! - `walrus.analyze(apply)`: JSONB with analysis and optional execution
//! - `walrus.reset()`: Clear state and history (superuser only)
//! - `walrus.cleanup_history()`: Delete old history records (moved from lib.rs)

use crate::algorithm::compute_recommendation;
use crate::config::{execute_alter_system, signal_postmaster_reload};
use crate::guc::{
    WALRUS_COOLDOWN_SEC, WALRUS_ENABLE, WALRUS_MAX, WALRUS_MAX_CHANGES_PER_HOUR, WALRUS_MIN_SIZE,
    WALRUS_SHRINK_ENABLE, WALRUS_SHRINK_FACTOR, WALRUS_SHRINK_INTERVALS, WALRUS_THRESHOLD,
};
use crate::history;
use crate::shmem::{self, now_unix, read_state};
use crate::stats::{checkpoint_timeout, get_current_max_wal_size};

use pgrx::datum::TimestampWithTimeZone;
use pgrx::prelude::*;
use pgrx::{JsonB, pg_sys};
use serde_json::json;

/// Check if the pg_walrus background worker is running.
///
/// Queries pg_stat_activity for a backend with backend_type = 'pg_walrus'.
fn check_worker_running() -> bool {
    let result = Spi::get_one::<bool>(
        "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE backend_type = 'pg_walrus')",
    );
    result.unwrap_or(Some(false)).unwrap_or(false)
}

/// Convert a Unix timestamp (seconds since epoch) to ISO 8601 format.
///
/// Returns None if the timestamp is 0 (indicating "never").
fn unix_timestamp_to_iso(timestamp: i64) -> Option<String> {
    if timestamp == 0 {
        return None;
    }
    // Use chrono for reliable ISO 8601 formatting
    use std::time::{Duration, UNIX_EPOCH};
    let datetime = UNIX_EPOCH + Duration::from_secs(timestamp as u64);
    // Format as ISO 8601 with timezone
    let since_epoch = datetime
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO);
    let secs = since_epoch.as_secs();

    // Manual ISO 8601 formatting without external crate dependency
    // PostgreSQL uses this format: 2025-12-30T10:15:30.000000+00:00
    let days_since_epoch = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Calculate year/month/day from days since epoch (1970-01-01)
    // Using a simplified algorithm for years 1970-2100
    let mut days = days_since_epoch as i64;
    let mut year = 1970i32;

    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }

    let leap = is_leap_year(year);
    let days_in_months: [i64; 12] = if leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1u32;
    for days_in_month in days_in_months.iter() {
        if days < *days_in_month {
            break;
        }
        days -= *days_in_month;
        month += 1;
    }
    let day = days + 1;

    Some(format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.000000+00:00",
        year, month, day, hours, minutes, seconds
    ))
}

/// Helper to check if a year is a leap year
fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

/// Implementation for getting current extension status.
///
/// Returns JSONB with 15 fields covering configuration, worker state, and counters.
///
/// # Fields
///
/// Configuration:
/// - `enabled`: Whether auto-sizing is enabled
/// - `current_max_wal_size_mb`: Current max_wal_size in MB
/// - `configured_maximum_mb`: walrus.max setting in MB
/// - `threshold`: walrus.threshold setting
/// - `checkpoint_timeout_sec`: PostgreSQL checkpoint_timeout in seconds
///
/// Shrink configuration:
/// - `shrink_enabled`: Whether auto-shrink is enabled
/// - `shrink_factor`: walrus.shrink_factor setting
/// - `shrink_intervals`: walrus.shrink_intervals setting
/// - `min_size_mb`: walrus.min_size in MB
///
/// Worker state:
/// - `worker_running`: Whether background worker is active
/// - `last_check_time`: ISO 8601 timestamp of last analysis (null if never)
/// - `last_adjustment_time`: ISO 8601 timestamp of last resize (null if never)
///
/// Counters:
/// - `total_adjustments`: Number of sizing changes since PostgreSQL start
/// - `quiet_intervals`: Consecutive low-activity intervals
///
/// Derived:
/// - `at_ceiling`: Whether current_max_wal_size_mb >= configured_maximum_mb
///
/// Note: Not marked #[pg_extern] - exposed via lib.rs walrus module.
pub fn status() -> JsonB {
    let state = read_state();
    let now = now_unix();
    let current_size = get_current_max_wal_size();
    let configured_max = WALRUS_MAX.get();
    let timeout_secs = checkpoint_timeout().as_secs() as i32;

    // Rate limiting GUC values
    let cooldown_sec = WALRUS_COOLDOWN_SEC.get();
    let max_changes_per_hour = WALRUS_MAX_CHANGES_PER_HOUR.get();

    // Compute cooldown status
    let cooldown_active = cooldown_sec > 0
        && state.last_adjustment_time > 0
        && now < state.last_adjustment_time.saturating_add(cooldown_sec as i64);

    let cooldown_remaining_sec = if cooldown_active {
        state
            .last_adjustment_time
            .saturating_add(cooldown_sec as i64)
            .saturating_sub(now) as i32
    } else {
        0
    };

    // Compute hourly limit status
    let hour_expired = if state.hour_window_start > 0 {
        now >= state.hour_window_start.saturating_add(3600)
    } else {
        true
    };

    let hourly_limit_reached = !hour_expired
        && max_changes_per_hour > 0
        && state.changes_this_hour >= max_changes_per_hour;

    JsonB(json!({
        "enabled": WALRUS_ENABLE.get(),
        "current_max_wal_size_mb": current_size,
        "configured_maximum_mb": configured_max,
        "threshold": WALRUS_THRESHOLD.get(),
        "checkpoint_timeout_sec": timeout_secs,
        "shrink_enabled": WALRUS_SHRINK_ENABLE.get(),
        "shrink_factor": WALRUS_SHRINK_FACTOR.get(),
        "shrink_intervals": WALRUS_SHRINK_INTERVALS.get(),
        "min_size_mb": WALRUS_MIN_SIZE.get(),
        "worker_running": check_worker_running(),
        "last_check_time": unix_timestamp_to_iso(state.last_check_time),
        "last_adjustment_time": unix_timestamp_to_iso(state.last_adjustment_time),
        "total_adjustments": state.total_adjustments,
        "quiet_intervals": state.quiet_intervals,
        "at_ceiling": current_size >= configured_max,
        // Rate limiting fields (7 new fields per FR-012)
        "cooldown_sec": cooldown_sec,
        "max_changes_per_hour": max_changes_per_hour,
        "cooldown_active": cooldown_active,
        "cooldown_remaining_sec": cooldown_remaining_sec,
        "changes_this_hour": state.changes_this_hour,
        "hourly_window_start": unix_timestamp_to_iso(state.hour_window_start),
        "hourly_limit_reached": hourly_limit_reached,
    }))
}

/// Implementation for getting adjustment history.
///
/// Returns SETOF RECORD from walrus.history table with columns:
/// - timestamp: TIMESTAMPTZ
/// - action: TEXT (increase/decrease/capped)
/// - old_size_mb: INTEGER
/// - new_size_mb: INTEGER
/// - forced_checkpoints: BIGINT
/// - reason: TEXT (nullable)
///
/// Note: Not marked #[pg_extern] - exposed via lib.rs walrus module.
#[allow(clippy::type_complexity)]
pub fn history_srf() -> Result<
    TableIterator<
        'static,
        (
            name!(timestamp, TimestampWithTimeZone),
            name!(action, String),
            name!(old_size_mb, i32),
            name!(new_size_mb, i32),
            name!(forced_checkpoints, i64),
            name!(reason, Option<String>),
        ),
    >,
    spi::Error,
> {
    // Check if history table exists
    let table_exists = Spi::get_one::<bool>(
        "SELECT EXISTS (
            SELECT 1 FROM pg_catalog.pg_class c
            JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
            WHERE n.nspname = 'walrus' AND c.relname = 'history'
        )",
    )?;

    if table_exists != Some(true) {
        return Err(spi::Error::InvalidPosition);
    }

    Spi::connect(|client| {
        let results = client.select(
            "SELECT timestamp, action, old_size_mb, new_size_mb, forced_checkpoints, reason
             FROM walrus.history ORDER BY timestamp ASC",
            None,
            &[],
        )?;

        let rows: Vec<_> = results
            .filter_map(|row| {
                let timestamp: TimestampWithTimeZone = row.get_by_name("timestamp").ok()??;
                let action: String = row.get_by_name("action").ok()??;
                let old_size: i32 = row.get_by_name("old_size_mb").ok()??;
                let new_size: i32 = row.get_by_name("new_size_mb").ok()??;
                let checkpoints: i64 = row.get_by_name("forced_checkpoints").ok()??;
                let reason: Option<String> = row.get_by_name("reason").ok()?;
                Some((timestamp, action, old_size, new_size, checkpoints, reason))
            })
            .collect();

        Ok(TableIterator::new(rows))
    })
}

/// Implementation for getting sizing recommendation.
///
/// Returns JSONB with:
/// - `current_size_mb`: Current max_wal_size
/// - `recommended_size_mb`: Suggested size
/// - `action`: "increase" | "decrease" | "none" | "error"
/// - `reason`: Human-readable explanation
/// - `confidence`: 0-100 confidence score
///
/// Note: Not marked #[pg_extern] - exposed via lib.rs walrus module.
pub fn recommendation() -> JsonB {
    let state = read_state();
    let rec = compute_recommendation(&state);

    JsonB(json!({
        "current_size_mb": rec.current_size_mb,
        "recommended_size_mb": rec.recommended_size_mb,
        "action": rec.action,
        "reason": rec.reason,
        "confidence": rec.confidence,
    }))
}

/// Implementation for immediate analysis with optional execution.
///
/// # Arguments
///
/// * `apply` - If true and recommendation action != "none", execute ALTER SYSTEM
///
/// # Returns
///
/// JSONB with:
/// - `analyzed`: true if analysis completed
/// - `recommendation`: The recommendation object
/// - `applied`: true only if apply=true AND change was executed
/// - `reason`: Error reason if analyzed=false
///
/// # Authorization
///
/// - `apply = false`: Any user
/// - `apply = true`: Superuser only (raises error otherwise)
///
/// Note: This is not marked #[pg_extern] because the walrus.analyze function
/// is defined in lib.rs walrus module to ensure proper schema placement.
pub fn analyze(apply: bool) -> Result<JsonB, spi::Error> {
    // Check superuser requirement for apply=true
    if apply && unsafe { !pg_sys::superuser() } {
        pgrx::error!("permission denied: walrus.analyze(apply := true) requires superuser");
    }

    // Check if extension is enabled
    if !WALRUS_ENABLE.get() {
        return Ok(JsonB(json!({
            "analyzed": false,
            "reason": "extension is disabled"
        })));
    }

    // Compute recommendation
    let state = read_state();
    let rec = compute_recommendation(&state);
    let mut applied = false;

    // Apply if requested and action warrants change
    if apply && rec.action != "none" && rec.action != "error" {
        // Check if we're already at the recommended size
        let current = get_current_max_wal_size();
        if rec.action == "increase" && current >= rec.recommended_size_mb {
            // Already at or above target, don't apply
        } else if rec.action == "decrease" && current <= rec.recommended_size_mb {
            // Already at or below target, don't apply
        } else {
            // Execute ALTER SYSTEM
            if execute_alter_system(rec.recommended_size_mb).is_ok() {
                applied = true;

                // Log to history
                let timeout_secs = checkpoint_timeout().as_secs() as i32;
                let current_requested = crate::stats::get_requested_checkpoints();
                let metadata = serde_json::json!({
                    "source": "walrus.analyze",
                    "confidence": rec.confidence,
                });

                let _ = history::insert_history_record(
                    &rec.action,
                    rec.current_size_mb,
                    rec.recommended_size_mb,
                    current_requested,
                    timeout_secs,
                    Some(&rec.reason),
                    Some(metadata),
                );

                // Update shmem state
                shmem::update_state(|s| {
                    s.total_adjustments += 1;
                    s.last_adjustment_time = now_unix();
                    if rec.action == "increase" {
                        s.quiet_intervals = 0;
                    }
                });

                // Send SIGHUP to apply configuration
                signal_postmaster_reload();
            }
        }
    }

    Ok(JsonB(json!({
        "analyzed": true,
        "recommendation": {
            "current_size_mb": rec.current_size_mb,
            "recommended_size_mb": rec.recommended_size_mb,
            "action": rec.action,
            "reason": rec.reason,
            "confidence": rec.confidence,
        },
        "applied": applied,
    })))
}

/// Implementation for resetting extension state.
///
/// Clears:
/// - All shared memory counters (quiet_intervals, total_adjustments, etc.)
/// - All rows from walrus.history table
///
/// # Returns
///
/// true on success
///
/// # Authorization
///
/// Superuser only (raises error otherwise)
///
/// # Edge Cases
///
/// If history table was dropped, logs WARNING but returns true
/// (shared memory reset succeeds regardless)
///
/// Note: Not marked #[pg_extern] - exposed via lib.rs walrus module.
pub fn reset() -> Result<bool, spi::Error> {
    // Check superuser requirement
    if unsafe { !pg_sys::superuser() } {
        pgrx::error!("permission denied: walrus.reset() requires superuser");
    }

    // Reset shared memory state
    shmem::reset_state();

    // Clear history table (with graceful handling if dropped)
    let table_exists = Spi::get_one::<bool>(
        "SELECT EXISTS (
            SELECT 1 FROM pg_catalog.pg_class c
            JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
            WHERE n.nspname = 'walrus' AND c.relname = 'history'
        )",
    )?;

    if table_exists == Some(true) {
        Spi::run("DELETE FROM walrus.history")?;
    } else {
        pgrx::warning!("pg_walrus: history table does not exist");
    }

    Ok(true)
}

/// Implementation for deleting old history records.
///
/// This is a re-export of the history module's cleanup function.
/// Retention period controlled by walrus.history_retention_days GUC.
///
/// # Returns
///
/// Number of records deleted
///
/// Note: Not marked #[pg_extern] - exposed via lib.rs walrus module.
pub fn cleanup_history() -> Result<i64, spi::Error> {
    history::cleanup_old_history()
}
