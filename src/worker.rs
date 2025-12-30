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
use crate::config::{execute_alter_system, signal_postmaster_reload};
use crate::guc::{
    WALRUS_COOLDOWN_SEC, WALRUS_DRY_RUN, WALRUS_ENABLE, WALRUS_MAX, WALRUS_MAX_CHANGES_PER_HOUR,
    WALRUS_MIN_SIZE, WALRUS_SHRINK_ENABLE, WALRUS_SHRINK_FACTOR, WALRUS_SHRINK_INTERVALS,
    WALRUS_THRESHOLD,
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
    signal_postmaster_reload();
}

/// Check if we should skip this iteration due to self-triggered SIGHUP.
///
/// Returns true if we should skip processing (self-triggered signal).
#[inline]
fn should_skip_iteration() -> bool {
    SUPPRESS_NEXT_SIGHUP.swap(false, Ordering::SeqCst)
}

/// Result of a rate limit check.
///
/// If the adjustment is blocked, contains the reason and metadata for history logging.
/// If allowed, the `blocked_by` field is None.
pub struct RateLimitResult {
    /// If Some, the adjustment is blocked. The string describes which limit blocked it.
    pub blocked_by: Option<String>,
    /// Reason text for history record (only set when blocked).
    pub reason: Option<String>,
    /// Metadata for history record (only set when blocked).
    pub metadata: Option<serde_json::Value>,
}

impl RateLimitResult {
    /// Create an allowed result (not blocked).
    fn allowed() -> Self {
        Self {
            blocked_by: None,
            reason: None,
            metadata: None,
        }
    }

    /// Create a blocked result with the specified reason and metadata.
    fn blocked(blocked_by: &str, reason: &str, metadata: serde_json::Value) -> Self {
        Self {
            blocked_by: Some(blocked_by.to_string()),
            reason: Some(reason.to_string()),
            metadata: Some(metadata),
        }
    }

    /// Check if the adjustment is blocked.
    fn is_blocked(&self) -> bool {
        self.blocked_by.is_some()
    }
}

/// Check rate limiting constraints before applying an adjustment.
///
/// Checks two rate limits in order (FR-014 specifies cooldown is checked first):
/// 1. Cooldown period: minimum seconds between adjustments (walrus.cooldown_sec)
/// 2. Hourly limit: maximum adjustments per rolling one-hour window (walrus.max_changes_per_hour)
///
/// Special cases:
/// - If cooldown_sec = 0, cooldown check is skipped entirely
/// - If max_changes_per_hour = 0, all automatic adjustments are blocked
///
/// Returns:
/// - RateLimitResult with blocked_by=None if adjustment is allowed
/// - RateLimitResult with blocked_by=Some("cooldown"|"hourly_limit") if blocked
fn check_rate_limit() -> RateLimitResult {
    let now = now_unix();
    let cooldown_sec = WALRUS_COOLDOWN_SEC.get();
    let max_changes_per_hour = WALRUS_MAX_CHANGES_PER_HOUR.get();
    let state = shmem::read_state();

    // Edge case: max_changes_per_hour = 0 blocks all automatic adjustments
    if max_changes_per_hour == 0 {
        return RateLimitResult::blocked(
            "hourly_limit",
            "automatic adjustments disabled (max_changes_per_hour = 0)",
            json!({
                "blocked_by": "hourly_limit",
                "max_changes_per_hour": 0,
                "changes_this_hour": state.changes_this_hour
            }),
        );
    }

    // Check 1: Cooldown period (skip if cooldown_sec = 0)
    if cooldown_sec > 0 && state.last_adjustment_time > 0 {
        let cooldown_end = state.last_adjustment_time.saturating_add(cooldown_sec as i64);
        // Use strict inequality: blocked if now < cooldown_end (not <=)
        // This means adjustment is allowed when now >= cooldown_end
        if now < cooldown_end {
            let remaining = cooldown_end.saturating_sub(now);
            return RateLimitResult::blocked(
                "cooldown",
                "cooldown active",
                json!({
                    "blocked_by": "cooldown",
                    "cooldown_sec": cooldown_sec,
                    "cooldown_remaining_sec": remaining,
                    "last_adjustment_time": state.last_adjustment_time
                }),
            );
        }
    }

    // Check 2: Hourly limit (only after cooldown passes)
    // First, check if the current hour window has expired
    let hour_expired = if state.hour_window_start > 0 {
        now >= state.hour_window_start.saturating_add(3600)
    } else {
        // No previous window, will start fresh
        true
    };

    // If window hasn't expired, check if we're at the limit
    if !hour_expired && state.changes_this_hour >= max_changes_per_hour {
        return RateLimitResult::blocked(
            "hourly_limit",
            "hourly limit reached",
            json!({
                "blocked_by": "hourly_limit",
                "max_changes_per_hour": max_changes_per_hour,
                "changes_this_hour": state.changes_this_hour,
                "hour_window_start": state.hour_window_start
            }),
        );
    }

    // Adjustment is allowed
    RateLimitResult::allowed()
}

/// Update rate limiting state after a successful adjustment.
///
/// Called after an adjustment is applied (or would be applied in dry-run mode).
/// Updates changes_this_hour and hour_window_start in shared memory.
fn update_rate_limit_state_after_adjustment() {
    let now = now_unix();
    shmem::update_state(|state| {
        // Check if hour window has expired
        let hour_expired = if state.hour_window_start > 0 {
            now >= state.hour_window_start.saturating_add(3600)
        } else {
            true
        };

        if hour_expired {
            // Start new window
            state.changes_this_hour = 1;
            state.hour_window_start = now;
        } else {
            // Increment count in current window
            state.changes_this_hour += 1;
        }
    });
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

        // Determine reason text based on capped status
        let reason_text = if is_capped {
            "capped at walrus.max"
        } else {
            "threshold exceeded"
        };

        let timeout_secs = checkpoint_timeout().as_secs();

        // RATE LIMIT CHECK: Must occur BEFORE dry-run check per FR-014
        // This ensures rate-limited adjustments are logged correctly in both modes.
        let rate_limit_result = check_rate_limit();
        if rate_limit_result.is_blocked() {
            let blocked_by = rate_limit_result.blocked_by.as_deref().unwrap_or("unknown");
            let reason = rate_limit_result.reason.as_deref().unwrap_or("rate limit blocked");
            let remaining = if blocked_by == "cooldown" {
                rate_limit_result
                    .metadata
                    .as_ref()
                    .and_then(|m| m.get("cooldown_remaining_sec"))
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0)
            } else {
                0
            };

            // Log rate limit message
            if blocked_by == "cooldown" {
                pgrx::log!(
                    "pg_walrus: adjustment blocked - cooldown active ({} seconds remaining)",
                    remaining
                );
            } else {
                let changes = rate_limit_result
                    .metadata
                    .as_ref()
                    .and_then(|m| m.get("changes_this_hour"))
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let max_changes = rate_limit_result
                    .metadata
                    .as_ref()
                    .and_then(|m| m.get("max_changes_per_hour"))
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                pgrx::log!(
                    "pg_walrus: adjustment blocked - hourly limit reached ({} of {})",
                    changes,
                    max_changes
                );
            }

            // Record skipped adjustment in history
            if let Err(e) = BackgroundWorker::transaction(|| {
                history::insert_history_record(
                    "skipped",
                    current_size,
                    new_size,
                    current_requested,
                    timeout_secs as i32,
                    Some(reason),
                    rate_limit_result.metadata.clone(),
                )
            }) {
                pgrx::warning!("pg_walrus: failed to log skipped history: {}", e);
            }

            return;
        }

        // DRY-RUN CHECK: If dry-run enabled, log what would happen and insert history,
        // but skip ALTER SYSTEM and SIGHUP. Mode change takes effect on next iteration.
        if WALRUS_DRY_RUN.get() {
            // Log dry-run message with [DRY-RUN] prefix
            pgrx::log!(
                "pg_walrus [DRY-RUN]: would change max_wal_size from {} MB to {} MB ({})",
                current_size,
                new_size,
                reason_text
            );

            // Build metadata with dry-run fields
            let would_apply = if is_capped { "capped" } else { "increase" };
            let metadata = if is_capped {
                json!({
                    "dry_run": true,
                    "would_apply": would_apply,
                    "delta": delta,
                    "multiplier": delta + 1,
                    "calculated_size_mb": calculated_size,
                    "walrus_max_mb": max_allowed
                })
            } else {
                json!({
                    "dry_run": true,
                    "would_apply": would_apply,
                    "delta": delta,
                    "multiplier": delta + 1,
                    "calculated_size_mb": calculated_size
                })
            };

            // Insert history with action='dry_run'
            if let Err(e) = BackgroundWorker::transaction(|| {
                history::insert_history_record(
                    "dry_run",
                    current_size,
                    new_size,
                    current_requested,
                    timeout_secs as i32,
                    Some(reason_text),
                    Some(metadata.clone()),
                )
            }) {
                pgrx::warning!("pg_walrus: failed to log dry-run history: {}", e);
            }

            // Update rate limiting state for dry-run (counts against limits per FR-014)
            update_rate_limit_state_after_adjustment();

            // Skip ALTER SYSTEM and SIGHUP in dry-run mode
            return;
        }

        // Log the resize decision (normal mode)
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

        // Update rate limiting state
        update_rate_limit_state_after_adjustment();

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

        let timeout_secs = checkpoint_timeout().as_secs();

        // RATE LIMIT CHECK: Must occur BEFORE dry-run check per FR-014
        // This ensures rate-limited shrink adjustments are logged correctly in both modes.
        let rate_limit_result = check_rate_limit();
        if rate_limit_result.is_blocked() {
            let blocked_by = rate_limit_result.blocked_by.as_deref().unwrap_or("unknown");
            let reason = rate_limit_result.reason.as_deref().unwrap_or("rate limit blocked");
            let remaining = if blocked_by == "cooldown" {
                rate_limit_result
                    .metadata
                    .as_ref()
                    .and_then(|m| m.get("cooldown_remaining_sec"))
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0)
            } else {
                0
            };

            // Log rate limit message
            if blocked_by == "cooldown" {
                pgrx::log!(
                    "pg_walrus: shrink blocked - cooldown active ({} seconds remaining)",
                    remaining
                );
            } else {
                let changes = rate_limit_result
                    .metadata
                    .as_ref()
                    .and_then(|m| m.get("changes_this_hour"))
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let max_changes = rate_limit_result
                    .metadata
                    .as_ref()
                    .and_then(|m| m.get("max_changes_per_hour"))
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                pgrx::log!(
                    "pg_walrus: shrink blocked - hourly limit reached ({} of {})",
                    changes,
                    max_changes
                );
            }

            // Record skipped shrink in history
            if let Err(e) = BackgroundWorker::transaction(|| {
                history::insert_history_record(
                    "skipped",
                    current_size,
                    new_size,
                    current_requested,
                    timeout_secs as i32,
                    Some(reason),
                    rate_limit_result.metadata.clone(),
                )
            }) {
                pgrx::warning!("pg_walrus: failed to log skipped shrink history: {}", e);
            }

            // Do NOT reset quiet_intervals when rate-limited - we want to try again next cycle
            return;
        }

        // DRY-RUN CHECK: If dry-run enabled, log what would happen and insert history,
        // but skip ALTER SYSTEM and SIGHUP. Mode change takes effect on next iteration.
        if WALRUS_DRY_RUN.get() {
            // Log dry-run message with [DRY-RUN] prefix
            pgrx::log!(
                "pg_walrus [DRY-RUN]: would change max_wal_size from {} MB to {} MB (sustained low activity)",
                current_size,
                new_size
            );

            // Build metadata with dry-run fields
            let metadata = json!({
                "dry_run": true,
                "would_apply": "decrease",
                "shrink_factor": shrink_factor,
                "quiet_intervals": new_quiet_intervals,
                "calculated_size_mb": new_size
            });

            // Insert history with action='dry_run'
            if let Err(e) = BackgroundWorker::transaction(|| {
                history::insert_history_record(
                    "dry_run",
                    current_size,
                    new_size,
                    current_requested,
                    timeout_secs as i32,
                    Some("sustained low activity"),
                    Some(metadata.clone()),
                )
            }) {
                pgrx::warning!("pg_walrus: failed to log dry-run history: {}", e);
            }

            // Reset quiet_intervals after dry-run shrink decision (algorithm state must update)
            shmem::update_state(|state| {
                state.quiet_intervals = 0;
            });

            // Update rate limiting state for dry-run (counts against limits per FR-014)
            update_rate_limit_state_after_adjustment();

            // Skip ALTER SYSTEM and SIGHUP in dry-run mode
            return;
        }

        // Log the shrink decision (normal mode)
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

        // Update rate limiting state
        update_rate_limit_state_after_adjustment();

        // Log to history table (FR-004, FR-011)
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
