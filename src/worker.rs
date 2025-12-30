//! Background worker implementation for pg_walrus.
//!
//! This module contains the main worker loop that monitors checkpoint activity
//! and triggers max_wal_size adjustments when forced checkpoints exceed the threshold.
//! It also handles automatic shrinking of max_wal_size after sustained periods of
//! low checkpoint activity.
//!
//! Worker state is persisted to shared memory (`shmem::WALRUS_STATE`) so SQL functions
//! can read real-time metrics.

use crate::algorithm::{calculate_new_size, calculate_shrink_size};
use crate::config::execute_alter_system;
use crate::guc::{
    WALRUS_ENABLE, WALRUS_MAX, WALRUS_MIN_SIZE, WALRUS_SHRINK_ENABLE, WALRUS_SHRINK_FACTOR,
    WALRUS_SHRINK_INTERVALS, WALRUS_THRESHOLD,
};
use crate::history;
use crate::shmem::{self, now_unix};
use crate::stats::{checkpoint_timeout, get_current_max_wal_size, get_requested_checkpoints};

use pgrx::bgworkers::{BackgroundWorker, SignalWakeFlags};
use pgrx::pg_sys;
use pgrx::prelude::*;

use serde_json::json;
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

/// Process checkpoint statistics and trigger resize if needed.
///
/// This is the core monitoring logic called each wake cycle:
/// 1. Fetch current checkpoint statistics
/// 2. Calculate delta from previous count
/// 3. GROW PATH: If delta >= threshold, calculate and apply new max_wal_size, reset quiet_intervals
/// 4. SHRINK PATH: If delta < threshold, increment quiet_intervals, potentially shrink
/// 5. Update shared memory state for SQL function visibility
///
/// The quiet_intervals counter tracks consecutive intervals with low activity.
/// State is persisted to shared memory so SQL functions can read real-time metrics.
fn process_checkpoint_stats(first_iteration: &mut bool) {
    // Fetch current checkpoint count
    let current_requested = get_requested_checkpoints();

    // Handle null pointer from pgstat (returns -1)
    if current_requested < 0 {
        pgrx::warning!("pg_walrus: checkpoint statistics unavailable, skipping cycle");
        return;
    }

    // Update last_check_time in shared memory
    let now = now_unix();
    shmem::update_state(|state| {
        state.last_check_time = now;
    });

    // First iteration: establish baseline
    if *first_iteration {
        shmem::update_state(|state| {
            state.prev_requested = current_requested;
        });
        *first_iteration = false;
        pgrx::debug1!(
            "pg_walrus: established baseline checkpoint count: {}",
            current_requested
        );
        return;
    }

    // Read current state from shared memory
    let state = shmem::read_state();
    let prev_requested = state.prev_requested;
    let quiet_intervals = state.quiet_intervals;

    // Calculate delta since last check
    let delta = current_requested - prev_requested;

    // Update prev_requested in shared memory
    shmem::update_state(|state| {
        state.prev_requested = current_requested;
    });

    // Check threshold
    let threshold = WALRUS_THRESHOLD.get() as i64;

    if delta >= threshold {
        // =====================================================================
        // GROW PATH: Activity detected, reset quiet intervals and potentially grow
        // =====================================================================
        // Reset quiet intervals (we only need the shmem update, not local variable)
        shmem::update_state(|state| {
            state.quiet_intervals = 0;
        });

        // Get current max_wal_size
        let current_size = get_current_max_wal_size();

        // Calculate new size with overflow protection
        let calculated_size = calculate_new_size(current_size, delta);
        let mut new_size = calculated_size;

        // Cap at walrus.max and track if capped
        let max_allowed = WALRUS_MAX.get();
        let is_capped = new_size > max_allowed;
        if is_capped {
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

        // Update shared memory state for successful adjustment
        shmem::update_state(|state| {
            state.total_adjustments += 1;
            state.last_adjustment_time = now_unix();
        });

        // Log to history table (FR-002, FR-003, FR-011)
        let (action, reason, metadata) = if is_capped {
            (
                "capped",
                "Calculated size exceeded walrus.max",
                json!({
                    "delta": delta,
                    "multiplier": delta + 1,
                    "calculated_size_mb": calculated_size,
                    "walrus_max_mb": max_allowed
                }),
            )
        } else {
            (
                "increase",
                "Forced checkpoints exceeded threshold",
                json!({
                    "delta": delta,
                    "multiplier": delta + 1,
                    "calculated_size_mb": calculated_size
                }),
            )
        };

        if let Err(e) = BackgroundWorker::transaction(|| {
            history::insert_history_record(
                action,
                current_size,
                new_size,
                current_requested,
                timeout_secs as i32,
                Some(reason),
                Some(metadata.clone()),
            )
        }) {
            pgrx::warning!("pg_walrus: failed to log history: {}", e);
        }

        // Send SIGHUP to postmaster to apply configuration
        send_sighup_to_postmaster();
    } else {
        // =====================================================================
        // SHRINK PATH: Low activity, increment quiet intervals and potentially shrink
        // =====================================================================
        shmem::update_state(|state| {
            state.quiet_intervals += 1;
        });
        // Re-read the incremented value for shrink logic
        let new_quiet_intervals = quiet_intervals + 1;

        // Check all shrink conditions
        let shrink_enable = WALRUS_SHRINK_ENABLE.get();
        let shrink_intervals = WALRUS_SHRINK_INTERVALS.get();
        let min_size = WALRUS_MIN_SIZE.get();
        let current_size = get_current_max_wal_size();

        // Shrink condition: enabled AND enough quiet intervals AND above minimum floor
        if !shrink_enable {
            return;
        }

        if new_quiet_intervals < shrink_intervals {
            return;
        }

        if current_size <= min_size {
            pgrx::debug1!(
                "pg_walrus: skipping shrink, max_wal_size ({} MB) already at or below min_size ({} MB)",
                current_size,
                min_size
            );
            return;
        }

        // Calculate new shrink target
        let shrink_factor = WALRUS_SHRINK_FACTOR.get();
        let new_size = calculate_shrink_size(current_size, shrink_factor, min_size);

        // Skip if shrink would not reduce size (e.g., already at floor)
        if new_size >= current_size {
            pgrx::debug1!(
                "pg_walrus: skipping shrink, calculated size ({} MB) >= current ({} MB)",
                new_size,
                current_size
            );
            return;
        }

        // Log the shrink decision
        pgrx::log!(
            "pg_walrus: shrinking max_wal_size from {} MB to {} MB",
            current_size,
            new_size
        );

        // Execute ALTER SYSTEM for shrink
        if let Err(e) = execute_alter_system(new_size) {
            pgrx::warning!(
                "pg_walrus: failed to execute ALTER SYSTEM for shrink, will retry next cycle: {}",
                e
            );
            return;
        }

        // Update shared memory state for successful adjustment
        shmem::update_state(|state| {
            state.total_adjustments += 1;
            state.last_adjustment_time = now_unix();
            state.quiet_intervals = 0; // Reset after successful shrink
        });

        // Log to history table (FR-004, FR-011)
        let timeout_secs = checkpoint_timeout().as_secs();
        let metadata = json!({
            "shrink_factor": shrink_factor,
            "quiet_intervals": new_quiet_intervals,
            "calculated_size_mb": new_size
        });

        if let Err(e) = BackgroundWorker::transaction(|| {
            history::insert_history_record(
                "decrease",
                current_size,
                new_size,
                current_requested,
                timeout_secs as i32,
                Some("Sustained low checkpoint activity"),
                Some(metadata.clone()),
            )
        }) {
            pgrx::warning!("pg_walrus: failed to log history: {}", e);
        }

        // Send SIGHUP to postmaster to apply configuration
        send_sighup_to_postmaster();
    }
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

    // Connect to the configured database to enable SPI access and pg_stat_activity visibility
    // Default: "postgres", configurable via walrus.database GUC (requires restart)
    let db_name: String = crate::guc::WALRUS_DATABASE
        .get()
        .and_then(|s| s.to_str().ok().map(|s| s.to_owned()))
        .unwrap_or_else(|| "postgres".to_owned());
    BackgroundWorker::connect_worker_to_spi(Some(&db_name), None);

    pgrx::log!("pg_walrus worker started");

    // Worker state - only first_iteration is local, rest is in shared memory
    let mut first_iteration = true;

    // Main loop: wake every checkpoint_timeout, process stats, repeat
    while BackgroundWorker::wait_latch(Some(checkpoint_timeout())) {
        // Check for SIGHUP (configuration reload) - must process BEFORE skip check
        // so that self-triggered SIGHUPs still reload our copy of GUC values
        if BackgroundWorker::sighup_received() {
            // Reload configuration - this updates our copy of GUC values
            // (max_wal_size_mb, etc.) to match what postmaster loaded
            unsafe {
                pg_sys::ProcessConfigFile(pg_sys::GucContext::PGC_SIGHUP);
            }
            pgrx::debug1!("pg_walrus: configuration reloaded");
        }

        // Check if we should skip processing due to self-triggered SIGHUP
        // (config is already reloaded above, just skip the stats processing)
        if should_skip_iteration() {
            continue;
        }

        // Check if monitoring is enabled
        if !WALRUS_ENABLE.get() {
            continue;
        }

        // Process checkpoint statistics and potentially resize or shrink
        // State (quiet_intervals, prev_requested, etc.) is managed in shared memory
        process_checkpoint_stats(&mut first_iteration);

        // Cleanup old history records (FR-009)
        if let Err(e) = BackgroundWorker::transaction(history::cleanup_old_history) {
            pgrx::warning!("pg_walrus: failed to cleanup history: {}", e);
        }
    }

    pgrx::log!("pg_walrus worker shutting down");
}

// Pure Rust unit tests (do not require PostgreSQL)
// These tests verify the algorithm functions imported from the algorithm module.
#[cfg(test)]
mod tests {
    use crate::algorithm::{calculate_new_size, calculate_shrink_size};

    // =========================================================================
    // Tests for calculate_new_size (grow)
    // =========================================================================

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

    // =========================================================================
    // Tests for calculate_shrink_size (shrink)
    // =========================================================================

    /// Test that calculate_shrink_size follows the formula: ceil(current_size * shrink_factor)
    #[test]
    fn test_shrink_size_normal() {
        // 4096 MB * 0.75 = 3072.0 -> ceil = 3072
        assert_eq!(calculate_shrink_size(4096, 0.75, 1024), 3072);

        // 2048 MB * 0.75 = 1536.0 -> ceil = 1536
        assert_eq!(calculate_shrink_size(2048, 0.75, 1024), 1536);

        // 1536 MB * 0.75 = 1152.0 -> ceil = 1152
        assert_eq!(calculate_shrink_size(1536, 0.75, 1024), 1152);
    }

    /// Test that calculate_shrink_size rounds up via f64::ceil()
    #[test]
    fn test_shrink_size_rounding_up() {
        // 1001 MB * 0.75 = 750.75 -> ceil = 751
        assert_eq!(calculate_shrink_size(1001, 0.75, 100), 751);

        // 1000 MB * 0.75 = 750.0 -> ceil = 750
        assert_eq!(calculate_shrink_size(1000, 0.75, 100), 750);

        // 1003 MB * 0.75 = 752.25 -> ceil = 753
        assert_eq!(calculate_shrink_size(1003, 0.75, 100), 753);

        // Test with very small fraction
        // 101 MB * 0.01 = 1.01 -> ceil = 2
        assert_eq!(calculate_shrink_size(101, 0.01, 1), 2);
    }

    /// Test that calculate_shrink_size clamps to min_size
    #[test]
    fn test_shrink_size_clamped_to_min() {
        // 2560 MB * 0.75 = 1920.0, but min_size is 2048 -> returns 2048
        assert_eq!(calculate_shrink_size(2560, 0.75, 2048), 2048);

        // 1024 MB * 0.75 = 768.0, but min_size is 1024 -> returns 1024
        assert_eq!(calculate_shrink_size(1024, 0.75, 1024), 1024);

        // 900 MB * 0.75 = 675.0, but min_size is 1024 -> returns 1024 (below floor)
        assert_eq!(calculate_shrink_size(900, 0.75, 1024), 1024);
    }

    /// Test calculate_shrink_size with different shrink factors (US5)
    #[test]
    fn test_shrink_size_different_factors() {
        // 4096 MB * 0.5 = 2048.0 (50% reduction)
        assert_eq!(calculate_shrink_size(4096, 0.5, 1024), 2048);

        // 4096 MB * 0.9 = 3686.4 -> ceil = 3687 (10% reduction)
        assert_eq!(calculate_shrink_size(4096, 0.9, 1024), 3687);

        // 4096 MB * 0.1 = 409.6 -> ceil = 410, but min_size 1024 -> 1024
        assert_eq!(calculate_shrink_size(4096, 0.1, 1024), 1024);
    }

    /// Test fractional MB rounding edge case (T053)
    #[test]
    fn test_shrink_size_fractional_mb() {
        // 1001 MB * 0.75 = 750.75 -> ceil = 751
        assert_eq!(calculate_shrink_size(1001, 0.75, 100), 751);
    }

    /// Test large value edge case (T058)
    #[test]
    fn test_shrink_size_large_value() {
        // i32::MAX * 0.99 should not overflow (shrink always produces smaller values)
        let result = calculate_shrink_size(i32::MAX, 0.99, 1024);
        // i32::MAX = 2147483647, * 0.99 = 2126008810.53 -> ceil = 2126008811
        assert_eq!(result, 2126008811);
        assert!(result < i32::MAX);
    }
}
