//! GUC (Grand Unified Configuration) parameter definitions for pg_walrus.
//!
//! This module defines the runtime configuration parameters:
//! - `walrus.enable`: Enable/disable automatic WAL size adjustment
//! - `walrus.max`: Maximum allowed max_wal_size (in MB)
//! - `walrus.threshold`: Forced checkpoint count threshold before resize
//! - `walrus.shrink_enable`: Enable/disable automatic shrinking
//! - `walrus.shrink_factor`: Multiplication factor when shrinking (0.01-0.99)
//! - `walrus.shrink_intervals`: Quiet intervals before triggering shrink
//! - `walrus.min_size`: Minimum floor for max_wal_size (in MB)
//! - `walrus.history_retention_days`: Days to retain history records before cleanup

use pgrx::guc::{GucContext, GucFlags, GucRegistry, GucSetting};
use pgrx::pg_sys;
use std::ffi::CString;

// =========================================================================
// Grow GUC Parameters
// =========================================================================

/// Enable automatic resizing of max_wal_size parameter.
/// When enabled, pg_walrus monitors forced checkpoints and adjusts max_wal_size.
/// Default: true
pub static WALRUS_ENABLE: GucSetting<bool> = GucSetting::<bool>::new(true);

/// Maximum size for max_wal_size that pg_walrus will not exceed.
/// Set lower than available WAL device storage.
/// Default: 4096 (4GB), Min: 2 MB, Max: i32::MAX MB
pub static WALRUS_MAX: GucSetting<i32> = GucSetting::<i32>::new(4096);

/// Forced checkpoints per checkpoint_timeout interval before increasing max_wal_size.
/// Higher values ignore occasional WAL spikes from batch jobs.
/// Default: 2, Min: 1, Max: 1000
pub static WALRUS_THRESHOLD: GucSetting<i32> = GucSetting::<i32>::new(2);

// =========================================================================
// Shrink GUC Parameters
// =========================================================================

/// Enable automatic shrinking of max_wal_size parameter.
/// When enabled (and walrus.enable is also true), pg_walrus shrinks max_wal_size
/// after sustained periods of low checkpoint activity.
/// Default: true
pub static WALRUS_SHRINK_ENABLE: GucSetting<bool> = GucSetting::<bool>::new(true);

/// Multiplication factor when shrinking max_wal_size.
/// Lower values shrink more aggressively. Must be between 0.01 and 0.99 (exclusive).
/// Default: 0.75 (reduces by 25%)
pub static WALRUS_SHRINK_FACTOR: GucSetting<f64> = GucSetting::<f64>::new(0.75);

/// Number of consecutive quiet checkpoint intervals before triggering shrink.
/// A quiet interval is one where forced checkpoints < threshold.
/// Default: 5, Min: 1, Max: 1000
pub static WALRUS_SHRINK_INTERVALS: GucSetting<i32> = GucSetting::<i32>::new(5);

/// Minimum floor for max_wal_size in MB.
/// pg_walrus will never shrink max_wal_size below this value.
/// Default: 1024 (1GB), Min: 2 MB, Max: i32::MAX MB
pub static WALRUS_MIN_SIZE: GucSetting<i32> = GucSetting::<i32>::new(1024);

// =========================================================================
// History GUC Parameters
// =========================================================================

/// Days to retain history records before automatic cleanup.
/// Records older than this are deleted by cleanup_history().
/// Default: 7, Min: 0 (delete all), Max: 3650 (10 years)
pub static WALRUS_HISTORY_RETENTION_DAYS: GucSetting<i32> = GucSetting::<i32>::new(7);

// =========================================================================
// Dry-Run GUC Parameters
// =========================================================================

/// Enable dry-run mode for testing without making configuration changes.
/// When enabled, pg_walrus logs what sizing decisions WOULD be made
/// but does not execute ALTER SYSTEM or send SIGHUP.
/// Default: false
pub static WALRUS_DRY_RUN: GucSetting<bool> = GucSetting::<bool>::new(false);

// =========================================================================
// Rate Limiting GUC Parameters
// =========================================================================

/// Minimum seconds between automatic max_wal_size adjustments (cooldown period).
/// Prevents rapid successive changes during volatile workloads.
/// Set to 0 to disable cooldown (only hourly limit applies).
/// Default: 300 (5 minutes), Min: 0, Max: 86400 (24 hours)
pub static WALRUS_COOLDOWN_SEC: GucSetting<i32> = GucSetting::<i32>::new(300);

/// Maximum number of automatic adjustments allowed per rolling one-hour window.
/// Provides a secondary safety limit beyond the cooldown period.
/// Set to 0 to block all automatic adjustments (emergency stop).
/// Default: 4, Min: 0, Max: 1000
pub static WALRUS_MAX_CHANGES_PER_HOUR: GucSetting<i32> = GucSetting::<i32>::new(4);

// =========================================================================
// Database GUC Parameter (Postmaster context - requires restart)
// =========================================================================

/// Database for pg_walrus metadata and history table.
/// The background worker connects to this database for SPI access.
/// Must be set in postgresql.conf and requires restart to change.
/// Default: "postgres"
pub static WALRUS_DATABASE: GucSetting<Option<CString>> =
    GucSetting::<Option<CString>>::new(Some(c"postgres"));

/// Register all pg_walrus GUC parameters with PostgreSQL.
///
/// This function registers all eight GUC parameters using GucContext::Sighup,
/// allowing runtime changes via ALTER SYSTEM and pg_reload_conf().
pub fn register_gucs() {
    // =========================================================================
    // Grow GUCs
    // =========================================================================

    GucRegistry::define_bool_guc(
        c"walrus.enable",
        c"Enable automatic resizing of max_wal_size parameter.",
        c"When enabled, pg_walrus monitors forced checkpoints and adjusts max_wal_size.",
        &WALRUS_ENABLE,
        GucContext::Sighup,
        GucFlags::default(),
    );

    GucRegistry::define_int_guc(
        c"walrus.max",
        c"Maximum size for max_wal_size that pg_walrus will not exceed.",
        c"Set lower than available WAL device storage.",
        &WALRUS_MAX,
        2,
        i32::MAX,
        GucContext::Sighup,
        GucFlags::UNIT_MB,
    );

    GucRegistry::define_int_guc(
        c"walrus.threshold",
        c"Forced checkpoints per timeout before increasing max_wal_size.",
        c"Higher values ignore occasional WAL spikes from batch jobs.",
        &WALRUS_THRESHOLD,
        1,
        1000,
        GucContext::Sighup,
        GucFlags::default(),
    );

    // =========================================================================
    // Shrink GUCs
    // =========================================================================

    GucRegistry::define_bool_guc(
        c"walrus.shrink_enable",
        c"Enable automatic shrinking of max_wal_size parameter.",
        c"When enabled, pg_walrus shrinks max_wal_size after sustained low activity.",
        &WALRUS_SHRINK_ENABLE,
        GucContext::Sighup,
        GucFlags::default(),
    );

    GucRegistry::define_float_guc(
        c"walrus.shrink_factor",
        c"Multiplication factor when shrinking max_wal_size.",
        c"Lower values shrink more aggressively. Must be between 0.01 and 0.99.",
        &WALRUS_SHRINK_FACTOR,
        0.01,
        0.99,
        GucContext::Sighup,
        GucFlags::default(),
    );

    GucRegistry::define_int_guc(
        c"walrus.shrink_intervals",
        c"Quiet checkpoint intervals before triggering shrink.",
        c"A quiet interval is one where forced checkpoints are below threshold.",
        &WALRUS_SHRINK_INTERVALS,
        1,
        1000,
        GucContext::Sighup,
        GucFlags::default(),
    );

    GucRegistry::define_int_guc(
        c"walrus.min_size",
        c"Minimum floor for max_wal_size in MB.",
        c"pg_walrus will never shrink max_wal_size below this value.",
        &WALRUS_MIN_SIZE,
        2,
        i32::MAX,
        GucContext::Sighup,
        GucFlags::UNIT_MB,
    );

    // =========================================================================
    // History GUCs
    // =========================================================================

    GucRegistry::define_int_guc(
        c"walrus.history_retention_days",
        c"Days to retain history records before automatic cleanup.",
        c"Records older than this are deleted by cleanup_history(). Range: 0-3650.",
        &WALRUS_HISTORY_RETENTION_DAYS,
        0,
        3650,
        GucContext::Sighup,
        GucFlags::default(),
    );

    // =========================================================================
    // Dry-Run GUCs
    // =========================================================================

    GucRegistry::define_bool_guc(
        c"walrus.dry_run",
        c"Enable dry-run mode (log decisions without applying).",
        c"When enabled, pg_walrus logs sizing decisions but does not execute ALTER SYSTEM.",
        &WALRUS_DRY_RUN,
        GucContext::Sighup,
        GucFlags::default(),
    );

    // =========================================================================
    // Rate Limiting GUCs
    // =========================================================================

    GucRegistry::define_int_guc(
        c"walrus.cooldown_sec",
        c"Minimum seconds between automatic max_wal_size adjustments.",
        c"Prevents rapid successive changes. Set to 0 to disable cooldown.",
        &WALRUS_COOLDOWN_SEC,
        0,
        86400,
        GucContext::Sighup,
        GucFlags::default(),
    );

    GucRegistry::define_int_guc(
        c"walrus.max_changes_per_hour",
        c"Maximum automatic adjustments per rolling one-hour window.",
        c"Set to 0 to block all automatic adjustments (emergency stop).",
        &WALRUS_MAX_CHANGES_PER_HOUR,
        0,
        1000,
        GucContext::Sighup,
        GucFlags::default(),
    );

    // =========================================================================
    // Database GUC (Postmaster context - requires restart)
    // =========================================================================

    GucRegistry::define_string_guc(
        c"walrus.database",
        c"Database for pg_walrus metadata and history table.",
        c"Background worker connects to this database. Requires restart to change.",
        &WALRUS_DATABASE,
        GucContext::Postmaster,
        GucFlags::SUPERUSER_ONLY,
    );

    // Reserve the "walrus" GUC prefix to prevent other extensions from using it.
    // This matches pg_walsizer's behavior with MarkGUCPrefixReserved("walsizer").
    unsafe {
        pg_sys::MarkGUCPrefixReserved(c"walrus".as_ptr());
    }
}
