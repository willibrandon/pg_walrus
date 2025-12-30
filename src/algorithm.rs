//! Sizing algorithm module for pg_walrus.
//!
//! This module contains the core sizing calculations used by both the background
//! worker and SQL functions (recommendation, analyze). Extracting this logic
//! enables consistent behavior and comprehensive testing.
//!
//! Key functions:
//! - `calculate_new_size()`: Compute grow target based on checkpoint delta
//! - `calculate_shrink_size()`: Compute shrink target with floor clamping
//! - `compute_recommendation()`: Full recommendation with action and confidence
//! - `compute_confidence()`: Data quality confidence score

use crate::guc::{
    WALRUS_ENABLE, WALRUS_MAX, WALRUS_MIN_SIZE, WALRUS_SHRINK_ENABLE, WALRUS_SHRINK_FACTOR,
    WALRUS_SHRINK_INTERVALS, WALRUS_THRESHOLD,
};
use crate::shmem::WalrusState;
use crate::stats::{get_current_max_wal_size, get_requested_checkpoints};
use serde::{Deserialize, Serialize};

/// Recommendation result from sizing analysis.
///
/// Contains the computed recommendation along with confidence metrics.
/// Returned by `walrus.recommendation()` and embedded in `walrus.analyze()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    /// Current max_wal_size in MB
    pub current_size_mb: i32,

    /// Recommended max_wal_size in MB (may equal current if no change)
    pub recommended_size_mb: i32,

    /// Action type: "increase", "decrease", "none", or "error"
    pub action: String,

    /// Human-readable explanation
    pub reason: String,

    /// Confidence level 0-100 based on data quality
    pub confidence: i32,
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

/// Calculate the shrink target size for max_wal_size.
///
/// Formula: ceil(current_size * shrink_factor), clamped to min_size
///
/// Uses f64 multiplication then rounds up via ceiling to ensure we don't
/// under-size. The result is clamped to min_size as a floor.
#[inline]
pub fn calculate_shrink_size(current_size: i32, shrink_factor: f64, min_size: i32) -> i32 {
    let raw = (current_size as f64) * shrink_factor;
    let rounded = raw.ceil() as i32;
    rounded.max(min_size)
}

/// Compute confidence score for a recommendation.
///
/// Confidence calculation:
/// - Base: 50 (default with valid stats)
/// - +20 if checkpoint count > 10 (sufficient samples)
/// - +15 if quiet_intervals > 0 (stable observation period)
/// - +15 if prev_requested > 0 (established baseline)
/// - Returns 0 if stats unavailable (error case)
///
/// # Arguments
///
/// * `state` - Current worker state from shared memory
/// * `checkpoint_count` - Current checkpoint count (-1 if unavailable)
///
/// # Returns
///
/// Confidence score 0-100
pub fn compute_confidence(state: &WalrusState, checkpoint_count: i64) -> i32 {
    // Error case: stats unavailable
    if checkpoint_count < 0 {
        return 0;
    }

    let mut confidence = 50;

    // +20 if checkpoint count > 10 (sufficient samples)
    if checkpoint_count > 10 {
        confidence += 20;
    }

    // +15 if quiet_intervals > 0 (stable observation period)
    if state.quiet_intervals > 0 {
        confidence += 15;
    }

    // +15 if prev_requested > 0 (established baseline)
    if state.prev_requested > 0 {
        confidence += 15;
    }

    confidence
}

/// Compute a sizing recommendation based on current state and statistics.
///
/// This function performs the same analysis as the background worker but
/// returns a recommendation without applying changes. Used by both
/// `walrus.recommendation()` and `walrus.analyze()`.
///
/// # Arguments
///
/// * `state` - Current worker state from shared memory
///
/// # Returns
///
/// A `Recommendation` with action, sizes, reason, and confidence.
///
/// # Action Values
///
/// - `"increase"`: Checkpoint activity warrants size increase
/// - `"decrease"`: Sustained low activity warrants shrink
/// - `"none"`: Current size is optimal
/// - `"error"`: Cannot compute (stats unavailable or extension disabled)
pub fn compute_recommendation(state: &WalrusState) -> Recommendation {
    let current_size = get_current_max_wal_size();
    let max_allowed = WALRUS_MAX.get();
    let threshold = WALRUS_THRESHOLD.get() as i64;

    // Check if extension is enabled
    if !WALRUS_ENABLE.get() {
        return Recommendation {
            current_size_mb: current_size,
            recommended_size_mb: current_size,
            action: "error".to_string(),
            reason: "extension is disabled".to_string(),
            confidence: 0,
        };
    }

    // Fetch current checkpoint count
    let current_requested = get_requested_checkpoints();

    // Handle stats unavailable
    if current_requested < 0 {
        return Recommendation {
            current_size_mb: current_size,
            recommended_size_mb: current_size,
            action: "error".to_string(),
            reason: "checkpoint statistics unavailable".to_string(),
            confidence: 0,
        };
    }

    // Calculate confidence
    let confidence = compute_confidence(state, current_requested);

    // Calculate delta from previous count
    // On first run (prev_requested = 0), delta will be the full current count
    // which may be large; we handle this gracefully
    let delta = if state.prev_requested > 0 {
        current_requested - state.prev_requested
    } else {
        // First run: no baseline yet, cannot recommend grow/shrink
        return Recommendation {
            current_size_mb: current_size,
            recommended_size_mb: current_size,
            action: "none".to_string(),
            reason: "awaiting baseline checkpoint count".to_string(),
            confidence: confidence.min(50), // Cap confidence without baseline
        };
    };

    // Check if delta exceeds threshold (grow path)
    if delta >= threshold {
        let calculated_size = calculate_new_size(current_size, delta);
        let mut new_size = calculated_size;
        let is_capped = new_size > max_allowed;

        if is_capped {
            new_size = max_allowed;
        }

        // Already at or above target
        if current_size >= new_size {
            return Recommendation {
                current_size_mb: current_size,
                recommended_size_mb: current_size,
                action: "none".to_string(),
                reason: format!(
                    "already at maximum ({} MB), {} forced checkpoints detected",
                    current_size, delta
                ),
                confidence,
            };
        }

        let reason = if is_capped {
            format!(
                "{} forced checkpoints detected, recommend {} MB (capped from {} MB)",
                delta, new_size, calculated_size
            )
        } else {
            format!(
                "{} forced checkpoints detected, recommend increase to {} MB",
                delta, new_size
            )
        };

        return Recommendation {
            current_size_mb: current_size,
            recommended_size_mb: new_size,
            action: "increase".to_string(),
            reason,
            confidence,
        };
    }

    // Check shrink conditions
    let shrink_enable = WALRUS_SHRINK_ENABLE.get();
    let shrink_intervals = WALRUS_SHRINK_INTERVALS.get();
    let min_size = WALRUS_MIN_SIZE.get();

    if !shrink_enable {
        return Recommendation {
            current_size_mb: current_size,
            recommended_size_mb: current_size,
            action: "none".to_string(),
            reason: format!(
                "low activity ({} forced checkpoints), shrink disabled",
                delta
            ),
            confidence,
        };
    }

    // Check if enough quiet intervals have accumulated
    // Note: state.quiet_intervals is updated by the worker, so this reflects
    // the count as of the last worker cycle
    if state.quiet_intervals < shrink_intervals {
        return Recommendation {
            current_size_mb: current_size,
            recommended_size_mb: current_size,
            action: "none".to_string(),
            reason: format!(
                "low activity, {} of {} quiet intervals needed for shrink",
                state.quiet_intervals, shrink_intervals
            ),
            confidence,
        };
    }

    // Check floor
    if current_size <= min_size {
        return Recommendation {
            current_size_mb: current_size,
            recommended_size_mb: current_size,
            action: "none".to_string(),
            reason: format!(
                "already at minimum ({} MB), {} quiet intervals accumulated",
                current_size, state.quiet_intervals
            ),
            confidence,
        };
    }

    // Calculate shrink target
    let shrink_factor = WALRUS_SHRINK_FACTOR.get();
    let new_size = calculate_shrink_size(current_size, shrink_factor, min_size);

    // Check if shrink would actually reduce size
    if new_size >= current_size {
        return Recommendation {
            current_size_mb: current_size,
            recommended_size_mb: current_size,
            action: "none".to_string(),
            reason: format!(
                "shrink target ({} MB) not less than current ({} MB)",
                new_size, current_size
            ),
            confidence,
        };
    }

    Recommendation {
        current_size_mb: current_size,
        recommended_size_mb: new_size,
        action: "decrease".to_string(),
        reason: format!(
            "{} quiet intervals, recommend decrease to {} MB",
            state.quiet_intervals, new_size
        ),
        confidence,
    }
}

// Pure Rust unit tests (do not require PostgreSQL)
#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Tests for calculate_new_size (grow) - T012
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
    // Tests for calculate_shrink_size (shrink) - T013
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

    /// Test calculate_shrink_size with different shrink factors
    #[test]
    fn test_shrink_size_different_factors() {
        // 4096 MB * 0.5 = 2048.0 (50% reduction)
        assert_eq!(calculate_shrink_size(4096, 0.5, 1024), 2048);

        // 4096 MB * 0.9 = 3686.4 -> ceil = 3687 (10% reduction)
        assert_eq!(calculate_shrink_size(4096, 0.9, 1024), 3687);

        // 4096 MB * 0.1 = 409.6 -> ceil = 410, but min_size 1024 -> 1024
        assert_eq!(calculate_shrink_size(4096, 0.1, 1024), 1024);
    }

    /// Test large value edge case
    #[test]
    fn test_shrink_size_large_value() {
        // i32::MAX * 0.99 should not overflow (shrink always produces smaller values)
        let result = calculate_shrink_size(i32::MAX, 0.99, 1024);
        // i32::MAX = 2147483647, * 0.99 = 2126008810.53 -> ceil = 2126008811
        assert_eq!(result, 2126008811);
        assert!(result < i32::MAX);
    }

    // =========================================================================
    // Tests for compute_confidence - T012
    // =========================================================================

    /// Test confidence calculation with various state combinations
    #[test]
    fn test_compute_confidence_base() {
        let state = WalrusState {
            quiet_intervals: 0,
            total_adjustments: 0,
            prev_requested: 0,
            last_check_time: 0,
            last_adjustment_time: 0,
            changes_this_hour: 0,
            hour_window_start: 0,
        };

        // Base case: valid stats but no history
        let confidence = compute_confidence(&state, 5);
        assert_eq!(confidence, 50, "Base confidence should be 50");
    }

    /// Test confidence with sufficient checkpoint samples
    #[test]
    fn test_compute_confidence_with_samples() {
        let state = WalrusState {
            quiet_intervals: 0,
            total_adjustments: 0,
            prev_requested: 0,
            last_check_time: 0,
            last_adjustment_time: 0,
            changes_this_hour: 0,
            hour_window_start: 0,
        };

        // Checkpoint count > 10 adds 20
        let confidence = compute_confidence(&state, 15);
        assert_eq!(confidence, 70, "Should add 20 for checkpoint count > 10");
    }

    /// Test confidence with quiet intervals
    #[test]
    fn test_compute_confidence_with_quiet_intervals() {
        let state = WalrusState {
            quiet_intervals: 3,
            total_adjustments: 0,
            prev_requested: 0,
            last_check_time: 0,
            last_adjustment_time: 0,
            changes_this_hour: 0,
            hour_window_start: 0,
        };

        // quiet_intervals > 0 adds 15
        let confidence = compute_confidence(&state, 5);
        assert_eq!(confidence, 65, "Should add 15 for quiet_intervals > 0");
    }

    /// Test confidence with established baseline
    #[test]
    fn test_compute_confidence_with_baseline() {
        let state = WalrusState {
            quiet_intervals: 0,
            total_adjustments: 0,
            prev_requested: 100,
            last_check_time: 0,
            last_adjustment_time: 0,
            changes_this_hour: 0,
            hour_window_start: 0,
        };

        // prev_requested > 0 adds 15
        let confidence = compute_confidence(&state, 5);
        assert_eq!(confidence, 65, "Should add 15 for prev_requested > 0");
    }

    /// Test maximum confidence
    #[test]
    fn test_compute_confidence_maximum() {
        let state = WalrusState {
            quiet_intervals: 5,
            total_adjustments: 10,
            prev_requested: 100,
            last_check_time: 1000,
            last_adjustment_time: 900,
            changes_this_hour: 0,
            hour_window_start: 0,
        };

        // All conditions: 50 + 20 + 15 + 15 = 100
        let confidence = compute_confidence(&state, 50);
        assert_eq!(confidence, 100, "Maximum confidence should be 100");
    }

    /// Test confidence when stats unavailable
    #[test]
    fn test_compute_confidence_stats_unavailable() {
        let state = WalrusState {
            quiet_intervals: 5,
            total_adjustments: 10,
            prev_requested: 100,
            last_check_time: 1000,
            last_adjustment_time: 900,
            changes_this_hour: 0,
            hour_window_start: 0,
        };

        // Stats unavailable (-1) returns 0
        let confidence = compute_confidence(&state, -1);
        assert_eq!(confidence, 0, "Should return 0 when stats unavailable");
    }
}
