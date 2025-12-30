//! Background worker implementation for pg_walrus.
//!
//! This module contains the main worker loop that monitors checkpoint activity
//! and triggers max_wal_size adjustments when forced checkpoints exceed the threshold.

use crate::config::execute_alter_system;
use crate::guc::{WALRUS_ENABLE, WALRUS_MAX, WALRUS_THRESHOLD};
use crate::stats::{checkpoint_timeout, get_current_max_wal_size, get_requested_checkpoints};

use pgrx::bgworkers::{BackgroundWorker, SignalWakeFlags};
use pgrx::prelude::*;
use pgrx::pg_sys;

use std::sync::atomic::{AtomicBool, Ordering};

/// Atomic flag to suppress processing of self-triggered SIGHUP.
///
/// When we send SIGHUP to the postmaster after ALTER SYSTEM, we set this flag
/// to prevent the next iteration from reprocessing the configuration reload.
static SUPPRESS_NEXT_SIGHUP: AtomicBool = AtomicBool::new(false);

/// Send SIGHUP to the postmaster to trigger configuration reload.
///
/// This is called after executing ALTER SYSTEM to apply the new max_wal_size.
/// The atomic flag is set to suppress our own handling of the resulting SIGHUP.
fn send_sighup_to_postmaster() {
    SUPPRESS_NEXT_SIGHUP.store(true, Ordering::SeqCst);
    unsafe {
        libc::kill(pg_sys::PostmasterPid, libc::SIGHUP);
    }
}

/// Check if we should skip this iteration due to self-triggered SIGHUP.
///
/// Returns true if we should skip processing (self-triggered signal).
#[inline]
fn should_skip_iteration() -> bool {
    SUPPRESS_NEXT_SIGHUP.swap(false, Ordering::SeqCst)
}

/// Calculate the new max_wal_size based on forced checkpoint count.
///
/// Formula: current_size * (delta + 1)
///
/// Uses saturating_mul to prevent i32 overflow. Returns the calculated value
/// before capping at walrus.max (capping is done by the caller).
#[inline]
pub fn calculate_new_size(current_size: i32, delta: i64) -> i32 {
    let multiplier = (delta + 1) as i32;
    current_size.saturating_mul(multiplier)
}

/// Process checkpoint statistics and trigger resize if needed.
///
/// This is the core monitoring logic called each wake cycle:
/// 1. Fetch current checkpoint statistics
/// 2. Calculate delta from previous count
/// 3. If delta >= threshold, calculate and apply new max_wal_size
fn process_checkpoint_stats(first_iteration: &mut bool, prev_requested: &mut i64) {
    // Fetch current checkpoint count
    let current_requested = get_requested_checkpoints();

    // Handle null pointer from pgstat (returns -1)
    if current_requested < 0 {
        pgrx::warning!("pg_walrus: checkpoint statistics unavailable, skipping cycle");
        return;
    }

    // First iteration: establish baseline
    if *first_iteration {
        *prev_requested = current_requested;
        *first_iteration = false;
        pgrx::debug1!(
            "pg_walrus: established baseline checkpoint count: {}",
            current_requested
        );
        return;
    }

    // Calculate delta since last check
    let delta = current_requested - *prev_requested;
    *prev_requested = current_requested;

    // Check threshold
    let threshold = WALRUS_THRESHOLD.get() as i64;
    if delta < threshold {
        return;
    }

    // Get current max_wal_size
    let current_size = get_current_max_wal_size();

    // Calculate new size with overflow protection
    let mut new_size = calculate_new_size(current_size, delta);

    // Cap at walrus.max
    let max_allowed = WALRUS_MAX.get();
    if new_size > max_allowed {
        pgrx::warning!(
            "pg_walrus: requested max_wal_size of {} MB exceeds maximum of {} MB; using maximum",
            new_size,
            max_allowed
        );
        new_size = max_allowed;
    }

    // Skip if already at cap
    if current_size >= new_size {
        pgrx::debug1!(
            "pg_walrus: max_wal_size already at maximum ({} MB)",
            current_size
        );
        return;
    }

    // Log the resize decision
    let timeout_secs = checkpoint_timeout().as_secs();
    pgrx::log!(
        "pg_walrus: detected {} forced checkpoints over {} seconds",
        delta,
        timeout_secs
    );
    pgrx::log!(
        "pg_walrus: resizing max_wal_size from {} MB to {} MB",
        current_size,
        new_size
    );

    // Execute ALTER SYSTEM
    if let Err(e) = execute_alter_system(new_size) {
        pgrx::warning!(
            "pg_walrus: failed to execute ALTER SYSTEM, will retry next cycle: {}",
            e
        );
        return;
    }

    // Send SIGHUP to postmaster to apply configuration
    send_sighup_to_postmaster();
}

/// Background worker main entry point.
///
/// This function is called by PostgreSQL when the background worker starts.
/// It runs the main monitoring loop until SIGTERM is received.
#[pg_guard]
#[unsafe(no_mangle)]
pub extern "C-unwind" fn walrus_worker_main(_arg: pg_sys::Datum) {
    // Attach signal handlers for SIGHUP (config reload) and SIGTERM (shutdown)
    BackgroundWorker::attach_signal_handlers(SignalWakeFlags::SIGHUP | SignalWakeFlags::SIGTERM);

    // Set application_name to match pg_walsizer behavior.
    // This makes the worker more identifiable in pg_stat_activity.
    unsafe {
        pg_sys::SetConfigOption(
            c"application_name".as_ptr(),
            c"pg_walrus".as_ptr(),
            pg_sys::GucContext::PGC_BACKEND,
            pg_sys::GucSource::PGC_S_OVERRIDE,
        );
    }

    // Connect to the postgres database to enable SPI access and pg_stat_activity visibility
    BackgroundWorker::connect_worker_to_spi(Some("postgres"), None);

    pgrx::log!("pg_walrus worker started");

    // Worker state
    let mut first_iteration = true;
    let mut prev_requested: i64 = 0;

    // Main loop: wake every checkpoint_timeout, process stats, repeat
    while BackgroundWorker::wait_latch(Some(checkpoint_timeout())) {
        // Check if we should skip due to self-triggered SIGHUP
        if should_skip_iteration() {
            continue;
        }

        // Check for external SIGHUP (configuration reload)
        if BackgroundWorker::sighup_received() {
            // GUC values are automatically reloaded by PostgreSQL
            pgrx::debug1!("pg_walrus: configuration reloaded");
        }

        // Check if monitoring is enabled
        if !WALRUS_ENABLE.get() {
            continue;
        }

        // Process checkpoint statistics and potentially resize
        process_checkpoint_stats(&mut first_iteration, &mut prev_requested);
    }

    pgrx::log!("pg_walrus worker shutting down");
}

// Pure Rust unit tests (do not require PostgreSQL)
#[cfg(test)]
mod tests {
    use super::*;

    /// Test that calculate_new_size follows the formula: current_size * (delta + 1)
    #[test]
    fn test_new_size_calculation() {
        // 1024 MB with 3 forced checkpoints: 1024 * 4 = 4096
        assert_eq!(calculate_new_size(1024, 3), 4096);

        // 2048 MB with 1 forced checkpoint: 2048 * 2 = 4096
        assert_eq!(calculate_new_size(2048, 1), 4096);

        // 512 MB with 2 forced checkpoints: 512 * 3 = 1536
        assert_eq!(calculate_new_size(512, 2), 1536);

        // Minimum case: 1 MB with 0 delta (should not happen, but test anyway)
        assert_eq!(calculate_new_size(1, 0), 1);
    }

    /// Test that calculate_new_size handles i32 overflow with saturating_mul
    #[test]
    fn test_overflow_protection() {
        // Large base * large multiplier should saturate to i32::MAX
        // i32::MAX / 2 = 1073741823, * 3 = 3221225469 which overflows
        let result = calculate_new_size(i32::MAX / 2, 2);
        assert_eq!(result, i32::MAX, "Should saturate to i32::MAX on overflow");

        // i32::MAX * 2 overflows
        let result = calculate_new_size(i32::MAX, 1);
        assert_eq!(result, i32::MAX, "Should saturate to i32::MAX on overflow");

        // 1_000_000_000 * 3 = 3_000_000_000 which overflows i32
        let result = calculate_new_size(1_000_000_000, 2);
        assert_eq!(result, i32::MAX, "Should saturate to i32::MAX on overflow");
    }
}
