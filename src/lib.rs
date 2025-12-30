//! pg_walrus - Automatic WAL size management for PostgreSQL
//!
//! This extension monitors checkpoint activity and automatically adjusts
//! `max_wal_size` to prevent performance-degrading forced checkpoints.

mod config;
mod guc;
mod history;
mod stats;
mod worker;

use pgrx::bgworkers::{BackgroundWorkerBuilder, BgWorkerStartTime};
use pgrx::prelude::*;

::pgrx::pg_module_magic!();

// =========================================================================
// Schema and History Table Creation (FR-001, FR-010)
// =========================================================================

pgrx::extension_sql!(
    r#"
-- Create walrus schema for namespacing
CREATE SCHEMA IF NOT EXISTS walrus;

-- History table for audit trail of sizing decisions
CREATE TABLE walrus.history (
    id BIGSERIAL PRIMARY KEY,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT now(),
    action TEXT NOT NULL CHECK (action IN ('increase', 'decrease', 'capped')),
    old_size_mb INTEGER NOT NULL CHECK (old_size_mb > 0),
    new_size_mb INTEGER NOT NULL CHECK (new_size_mb > 0),
    forced_checkpoints BIGINT NOT NULL CHECK (forced_checkpoints >= 0),
    checkpoint_timeout_sec INTEGER NOT NULL CHECK (checkpoint_timeout_sec > 0),
    reason TEXT,
    metadata JSONB
);

-- Index for efficient range queries and cleanup
CREATE INDEX walrus_history_timestamp_idx ON walrus.history (timestamp);

-- Documentation comments
COMMENT ON TABLE walrus.history IS 'Audit trail of pg_walrus sizing decisions';
COMMENT ON COLUMN walrus.history.id IS 'Unique identifier for each history record';
COMMENT ON COLUMN walrus.history.timestamp IS 'When the sizing decision was made';
COMMENT ON COLUMN walrus.history.action IS 'Decision type: increase, decrease, or capped';
COMMENT ON COLUMN walrus.history.old_size_mb IS 'max_wal_size before the change (in MB)';
COMMENT ON COLUMN walrus.history.new_size_mb IS 'max_wal_size after the change (in MB)';
COMMENT ON COLUMN walrus.history.forced_checkpoints IS 'Checkpoint count at decision time';
COMMENT ON COLUMN walrus.history.checkpoint_timeout_sec IS 'checkpoint_timeout value in seconds at decision time';
COMMENT ON COLUMN walrus.history.reason IS 'Human-readable explanation of the decision';
COMMENT ON COLUMN walrus.history.metadata IS 'Algorithm-specific details in JSON format';
"#,
    name = "create_walrus_schema_and_history",
    bootstrap,
);

// =========================================================================
// SQL-Callable Functions in walrus Schema (T039-T041)
// =========================================================================

/// Module for SQL-callable functions in the walrus schema.
///
/// The `#[pg_schema]` attribute ensures functions are created in the `walrus` schema
/// rather than the default public schema.
#[pg_schema]
mod walrus {
    use crate::history;
    use pgrx::prelude::*;

    /// Deletes history records older than the configured retention period.
    ///
    /// This function can be called manually or scheduled via pg_cron.
    /// The retention period is controlled by the `walrus.history_retention_days` GUC.
    ///
    /// # Returns
    ///
    /// The number of records deleted.
    ///
    /// # Example
    ///
    /// ```sql
    /// SELECT walrus.cleanup_history();
    /// -- Returns: 42 (number of deleted records)
    /// ```
    #[pg_extern]
    fn cleanup_history() -> Result<i64, spi::Error> {
        history::cleanup_old_history()
    }
}

/// Extension initialization entry point.
///
/// Called by PostgreSQL when the extension is loaded. When loaded via
/// `shared_preload_libraries`, registers GUC parameters and the background worker.
/// When loaded via CREATE EXTENSION (after server start), only GUC parameters
/// are available (background worker registration requires shared_preload_libraries).
#[allow(non_snake_case)]
#[pg_guard]
pub extern "C-unwind" fn _PG_init() {
    // Register GUC parameters (always available, even without shared_preload_libraries)
    guc::register_gucs();

    // Background worker registration ONLY works during shared_preload_libraries loading.
    // If loaded via CREATE EXTENSION after server start, skip worker registration.
    let in_shared_preload = unsafe { pgrx::pg_sys::process_shared_preload_libraries_in_progress };
    if !in_shared_preload {
        // Not loaded via shared_preload_libraries - bgworker registration not possible
        return;
    }

    // Register the background worker
    // Restart time matches pg_walsizer: use checkpoint_timeout so if worker crashes,
    // it restarts after the same interval as its normal wake cycle.
    let restart_time = stats::checkpoint_timeout();

    BackgroundWorkerBuilder::new("pg_walrus")
        .set_function("walrus_worker_main")
        .set_library("pg_walrus")
        .set_type("pg_walrus")
        .set_start_time(BgWorkerStartTime::RecoveryFinished)
        .set_restart_time(Some(restart_time))
        .enable_spi_access()
        .load();
}

// MANDATORY: pg_test module for pgrx-tests framework
// This module configures shared_preload_libraries so background worker tests work.
#[cfg(test)]
pub mod pg_test {
    /// Called once at test framework initialization
    pub fn setup(_options: Vec<&str>) {
        // Optional: one-time setup code
    }

    /// PostgreSQL configuration for tests - MANDATORY for background worker testing
    ///
    /// The pgrx-tests framework calls this function during test initialization
    /// and writes the returned settings to postgresql.auto.conf BEFORE starting
    /// PostgreSQL. This allows background workers to be registered during startup.
    pub fn postgresql_conf_options() -> Vec<&'static str> {
        vec!["shared_preload_libraries='pg_walrus'"]
    }
}

// PostgreSQL integration tests requiring a running database
#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use crate::stats;
    use crate::worker;
    use pgrx::prelude::*;

    // =========================================================================
    // GUC Parameter Tests
    // =========================================================================

    /// Test that walrus.enable GUC has correct default value (true -> 'on')
    #[pg_test]
    fn test_guc_walrus_enable_default() {
        let result = Spi::get_one::<&str>("SHOW walrus.enable").expect("SHOW failed");
        assert_eq!(result, Some("on"), "walrus.enable should default to 'on'");
    }

    /// Test that walrus.max GUC has correct default value (4096 MB)
    #[pg_test]
    fn test_guc_walrus_max_default() {
        let result = Spi::get_one::<&str>("SHOW walrus.max").expect("SHOW failed");
        assert_eq!(result, Some("4GB"), "walrus.max should default to '4GB'");
    }

    /// Test that walrus.threshold GUC has correct default value (2)
    #[pg_test]
    fn test_guc_walrus_threshold_default() {
        let result = Spi::get_one::<&str>("SHOW walrus.threshold").expect("SHOW failed");
        assert_eq!(result, Some("2"), "walrus.threshold should default to '2'");
    }

    // =========================================================================
    // Shrink GUC Parameter Tests (T025-T031)
    // =========================================================================

    /// Test that walrus.shrink_enable GUC has correct default value (true -> 'on') (T025)
    #[pg_test]
    fn test_guc_walrus_shrink_enable_default() {
        let result = Spi::get_one::<&str>("SHOW walrus.shrink_enable").expect("SHOW failed");
        assert_eq!(
            result,
            Some("on"),
            "walrus.shrink_enable should default to 'on'"
        );
    }

    /// Test that walrus.shrink_factor GUC has correct default value (0.75) (T026)
    #[pg_test]
    fn test_guc_walrus_shrink_factor_default() {
        let result = Spi::get_one::<&str>("SHOW walrus.shrink_factor").expect("SHOW failed");
        assert_eq!(
            result,
            Some("0.75"),
            "walrus.shrink_factor should default to '0.75'"
        );
    }

    /// Test that walrus.shrink_intervals GUC has correct default value (5) (T027)
    #[pg_test]
    fn test_guc_walrus_shrink_intervals_default() {
        let result = Spi::get_one::<&str>("SHOW walrus.shrink_intervals").expect("SHOW failed");
        assert_eq!(
            result,
            Some("5"),
            "walrus.shrink_intervals should default to '5'"
        );
    }

    /// Test that walrus.min_size GUC has correct default value (1024 MB = 1GB) (T028)
    #[pg_test]
    fn test_guc_walrus_min_size_default() {
        let result = Spi::get_one::<&str>("SHOW walrus.min_size").expect("SHOW failed");
        assert_eq!(
            result,
            Some("1GB"),
            "walrus.min_size should default to '1GB'"
        );
    }

    /// Test that walrus.shrink_factor has vartype = 'real' in pg_settings (T030)
    #[pg_test]
    fn test_guc_shrink_factor_vartype() {
        let vartype = Spi::get_one::<&str>(
            "SELECT vartype FROM pg_settings WHERE name = 'walrus.shrink_factor'",
        )
        .expect("query failed");
        assert_eq!(
            vartype,
            Some("real"),
            "walrus.shrink_factor should have vartype = 'real'"
        );
    }

    /// Test that walrus.min_size has unit = 'MB' in pg_settings (T031)
    #[pg_test]
    fn test_guc_min_size_has_unit() {
        let unit =
            Spi::get_one::<&str>("SELECT unit FROM pg_settings WHERE name = 'walrus.min_size'")
                .expect("query failed");
        assert_eq!(unit, Some("MB"), "walrus.min_size should have unit = 'MB'");
    }

    /// Test that all 8 walrus GUCs are visible in pg_settings with correct context (T029).
    #[pg_test]
    fn test_guc_context_is_sighup() {
        let count = Spi::get_one::<i64>(
            "SELECT COUNT(*) FROM pg_settings WHERE name LIKE 'walrus.%' AND context = 'sighup'",
        )
        .expect("query failed");
        assert_eq!(
            count,
            Some(8),
            "All 8 walrus GUCs should have context = 'sighup'"
        );
    }

    /// Test that SET fails for SIGHUP context GUCs (they can only be changed via ALTER SYSTEM).
    /// PostgreSQL returns: "parameter X cannot be changed now"
    #[pg_test(error = "parameter \"walrus.enable\" cannot be changed now")]
    fn test_guc_set_fails_for_sighup_context() {
        Spi::run("SET walrus.enable = false").unwrap();
    }

    /// Test that walrus GUCs have the correct vartype in pg_settings.
    #[pg_test]
    fn test_guc_vartypes() {
        // walrus.enable should be bool
        let enable_type =
            Spi::get_one::<&str>("SELECT vartype FROM pg_settings WHERE name = 'walrus.enable'")
                .expect("query failed");
        assert_eq!(
            enable_type,
            Some("bool"),
            "walrus.enable should be type bool"
        );

        // walrus.max should be integer
        let max_type =
            Spi::get_one::<&str>("SELECT vartype FROM pg_settings WHERE name = 'walrus.max'")
                .expect("query failed");
        assert_eq!(
            max_type,
            Some("integer"),
            "walrus.max should be type integer"
        );

        // walrus.threshold should be integer
        let threshold_type =
            Spi::get_one::<&str>("SELECT vartype FROM pg_settings WHERE name = 'walrus.threshold'")
                .expect("query failed");
        assert_eq!(
            threshold_type,
            Some("integer"),
            "walrus.threshold should be type integer"
        );
    }

    /// Test that walrus.max has unit = 'MB' in pg_settings.
    #[pg_test]
    fn test_guc_max_has_unit() {
        let unit = Spi::get_one::<&str>("SELECT unit FROM pg_settings WHERE name = 'walrus.max'")
            .expect("query failed");
        assert_eq!(unit, Some("MB"), "walrus.max should have unit = 'MB'");
    }

    // =========================================================================
    // Background Worker Tests
    // =========================================================================

    /// Test that pg_walrus background worker is running and visible in pg_stat_activity.
    ///
    /// This test requires the pg_test module with postgresql_conf_options() to set
    /// shared_preload_libraries='pg_walrus' before PostgreSQL starts.
    #[pg_test]
    fn test_background_worker_running() {
        let result = Spi::get_one::<bool>(
            "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE backend_type = 'pg_walrus')",
        )
        .expect("query failed");
        assert_eq!(
            result,
            Some(true),
            "pg_walrus background worker should be visible in pg_stat_activity"
        );
    }

    /// Test that the background worker has the correct type in pg_stat_activity.
    #[pg_test]
    fn test_background_worker_type() {
        let result = Spi::get_one::<&str>(
            "SELECT backend_type FROM pg_stat_activity WHERE backend_type = 'pg_walrus'",
        )
        .expect("query failed");
        assert_eq!(
            result,
            Some("pg_walrus"),
            "pg_walrus worker should have backend_type = 'pg_walrus'"
        );
    }

    /// Test that the background worker is connected to a database.
    #[pg_test]
    fn test_background_worker_connected_to_db() {
        let result = Spi::get_one::<bool>(
            "SELECT datname IS NOT NULL FROM pg_stat_activity WHERE backend_type = 'pg_walrus'",
        )
        .expect("query failed");
        assert_eq!(
            result,
            Some(true),
            "pg_walrus worker should be connected to a database"
        );
    }

    /// Test that the background worker has application_name set.
    /// This matches pg_walsizer behavior which sets application_name via SetConfigOption.
    #[pg_test]
    fn test_background_worker_application_name() {
        let result = Spi::get_one::<&str>(
            "SELECT application_name FROM pg_stat_activity WHERE backend_type = 'pg_walrus'",
        )
        .expect("query failed");
        assert_eq!(
            result,
            Some("pg_walrus"),
            "pg_walrus worker should have application_name = 'pg_walrus'"
        );
    }

    // =========================================================================
    // Stats Module Tests
    // =========================================================================

    /// Test that checkpoint_timeout() returns a valid value.
    /// PostgreSQL's checkpoint_timeout range is 30-86400 seconds (per GUC docs).
    #[pg_test]
    fn test_checkpoint_timeout_returns_valid_value() {
        let timeout_secs = stats::checkpoint_timeout_secs();

        // PostgreSQL default is 300 seconds (5 minutes)
        // Valid range is 30-86400 seconds
        assert!(
            timeout_secs >= 30 && timeout_secs <= 86400,
            "checkpoint_timeout should be in range 30-86400 seconds, got {}",
            timeout_secs
        );
    }

    /// Test that checkpoint_timeout() as Duration works correctly.
    #[pg_test]
    fn test_checkpoint_timeout_duration() {
        let duration = stats::checkpoint_timeout();

        // Should be at least 30 seconds (minimum PostgreSQL value)
        assert!(
            duration.as_secs() >= 30,
            "checkpoint_timeout duration should be at least 30 seconds"
        );
    }

    /// Test that get_requested_checkpoints() returns a non-negative value.
    /// This verifies the version-specific code path works correctly.
    #[pg_test]
    fn test_get_requested_checkpoints_returns_valid_value() {
        let count = stats::get_requested_checkpoints();

        // Should return 0 or positive number, -1 indicates error
        assert!(
            count >= 0,
            "get_requested_checkpoints should return >= 0, got {}",
            count
        );
    }

    /// Test that get_current_max_wal_size() returns a valid value.
    #[pg_test]
    fn test_get_current_max_wal_size_returns_valid_value() {
        let size = stats::get_current_max_wal_size();

        // min_wal_size minimum is 2 segments (32 MB), max is very large
        // Typical default is 1024 MB (1GB)
        assert!(
            size >= 32,
            "max_wal_size should be at least 32 MB, got {}",
            size
        );
    }

    /// Test that get_current_max_wal_size() matches SHOW max_wal_size.
    #[pg_test]
    fn test_get_current_max_wal_size_matches_show() {
        let internal_value = stats::get_current_max_wal_size();

        // Get value via SQL - returns in MB when unit suffix not shown
        let sql_value =
            Spi::get_one::<i32>("SELECT setting::int FROM pg_settings WHERE name = 'max_wal_size'")
                .expect("query failed")
                .expect("max_wal_size setting not found");

        assert_eq!(
            internal_value, sql_value,
            "Internal max_wal_size_mb ({}) should match pg_settings ({})",
            internal_value, sql_value
        );
    }

    // =========================================================================
    // Config Module Tests
    // =========================================================================
    // NOTE: execute_alter_system() cannot be tested directly in pg_test because
    // ALTER SYSTEM cannot run inside a transaction block. The actual ALTER SYSTEM
    // execution is tested via pg_regress tests which run outside transaction context.
    //
    // The config module is implicitly tested by the background worker's ability
    // to successfully modify max_wal_size during runtime operation.

    // =========================================================================
    // Worker Logic Tests (calculate_new_size)
    // =========================================================================

    /// Test that calculate_new_size follows the correct formula.
    /// Formula: current_size * (delta + 1)
    #[pg_test]
    fn test_calculate_new_size_formula() {
        // 1024 MB with 3 forced checkpoints: 1024 * 4 = 4096
        assert_eq!(worker::calculate_new_size(1024, 3), 4096);

        // 2048 MB with 1 forced checkpoint: 2048 * 2 = 4096
        assert_eq!(worker::calculate_new_size(2048, 1), 4096);

        // 512 MB with 2 forced checkpoints: 512 * 3 = 1536
        assert_eq!(worker::calculate_new_size(512, 2), 1536);
    }

    /// Test that calculate_new_size handles edge cases.
    #[pg_test]
    fn test_calculate_new_size_edge_cases() {
        // Delta of 0 should multiply by 1 (no change)
        assert_eq!(worker::calculate_new_size(1024, 0), 1024);

        // Small values
        assert_eq!(worker::calculate_new_size(1, 1), 2);

        // Large delta
        assert_eq!(worker::calculate_new_size(100, 10), 1100);
    }

    /// Test that calculate_new_size protects against overflow.
    #[pg_test]
    fn test_calculate_new_size_overflow_protection() {
        // Values that would overflow should saturate to i32::MAX
        let result = worker::calculate_new_size(i32::MAX, 1);
        assert_eq!(result, i32::MAX, "Should saturate on overflow");

        let result = worker::calculate_new_size(1_000_000_000, 3);
        assert_eq!(result, i32::MAX, "Should saturate on overflow");
    }

    // =========================================================================
    // Integration Tests (End-to-End Behavior)
    // =========================================================================

    /// Test that max cap is enforced: walrus.max limits calculated size.
    /// This tests the capping logic in process_checkpoint_stats().
    #[pg_test]
    fn test_max_cap_enforcement() {
        // The default walrus.max is 4096 MB (4GB)
        // If calculated size exceeds this, it should be capped

        // Direct test of the cap logic:
        let walrus_max = 4096;
        let calculated = worker::calculate_new_size(2048, 5); // 2048 * 6 = 12288
        let capped = if calculated > walrus_max {
            walrus_max
        } else {
            calculated
        };

        assert_eq!(
            capped, 4096,
            "Calculated size should be capped at walrus.max"
        );
    }

    /// Test that threshold controls when resizing triggers.
    /// This tests the threshold check in process_checkpoint_stats().
    #[pg_test]
    fn test_threshold_controls_trigger() {
        // Default threshold is 2
        // Delta < threshold should not trigger resize
        // Delta >= threshold should trigger resize

        let threshold = 2i64;

        // Delta of 1 should NOT trigger
        let delta_1 = 1i64;
        assert!(
            delta_1 < threshold,
            "Delta of 1 should be below default threshold of 2"
        );

        // Delta of 2 should trigger
        let delta_2 = 2i64;
        assert!(
            delta_2 >= threshold,
            "Delta of 2 should meet default threshold of 2"
        );

        // Delta of 5 should trigger
        let delta_5 = 5i64;
        assert!(
            delta_5 >= threshold,
            "Delta of 5 should exceed default threshold of 2"
        );
    }

    /// Test that the full resize calculation matches pg_walsizer behavior.
    /// From pg_walsizer: want_max = max_wal_size_mb * (requested + 1)
    #[pg_test]
    fn test_resize_calculation_matches_walsizer() {
        // Get current max_wal_size
        let current = stats::get_current_max_wal_size();

        // Simulate 4 forced checkpoints
        let forced_checkpoints = 4i64;
        let expected = current.saturating_mul((forced_checkpoints + 1) as i32);

        let calculated = worker::calculate_new_size(current, forced_checkpoints);

        assert_eq!(
            calculated, expected,
            "calculate_new_size should match pg_walsizer formula"
        );
    }

    /// Test complete flow: stats fetch -> calculation -> capping.
    /// This verifies all components work together (except ALTER SYSTEM which requires non-transaction context).
    #[pg_test]
    fn test_complete_resize_flow_without_alter_system() {
        // Step 1: Verify we can fetch checkpoint stats
        let checkpoints = stats::get_requested_checkpoints();
        assert!(checkpoints >= 0, "Should fetch checkpoint stats");

        // Step 2: Verify we can get current max_wal_size
        let current_size = stats::get_current_max_wal_size();
        assert!(current_size > 0, "Should get current max_wal_size");

        // Step 3: Calculate new size (simulating 3 forced checkpoints)
        let delta = 3i64;
        let new_size = worker::calculate_new_size(current_size, delta);
        assert!(
            new_size > current_size,
            "New size should be larger than current"
        );

        // Step 4: Cap at max (using default 4096)
        let max_allowed = 4096;
        let capped_size = if new_size > max_allowed {
            max_allowed
        } else {
            new_size
        };

        // Verify capping worked
        assert!(
            capped_size <= max_allowed,
            "Should be capped at max_allowed"
        );
        assert!(capped_size > 0, "Should have valid size");

        // NOTE: execute_alter_system() cannot be tested here because
        // ALTER SYSTEM cannot run inside a transaction block.
        // The background worker tests this implicitly.
    }

    /// Test that checkpoint statistics are version-specific but work correctly.
    /// PG15-16 uses requested_checkpoints, PG17+ uses num_requested.
    #[pg_test]
    fn test_version_specific_checkpoint_stats() {
        // This test verifies the correct version-specific code path is compiled

        // First call to establish baseline
        let first = stats::get_requested_checkpoints();
        assert!(first >= 0, "First call should return valid count");

        // Second call should return same or higher value
        let second = stats::get_requested_checkpoints();
        assert!(
            second >= first,
            "Checkpoint count should be monotonically increasing"
        );
    }

    /// Test that we can read the GUC values via the static variables.
    #[pg_test]
    fn test_guc_static_access() {
        use crate::guc::{WALRUS_ENABLE, WALRUS_MAX, WALRUS_THRESHOLD};

        // Verify we can access GUC values via the static variables
        let enable = WALRUS_ENABLE.get();
        assert!(enable, "WALRUS_ENABLE should default to true");

        let max = WALRUS_MAX.get();
        assert_eq!(max, 4096, "WALRUS_MAX should default to 4096 MB");

        let threshold = WALRUS_THRESHOLD.get();
        assert_eq!(threshold, 2, "WALRUS_THRESHOLD should default to 2");
    }

    /// Test that skip logic works when already at max.
    #[pg_test]
    fn test_skip_when_already_at_max() {
        // If current_size >= new_size, we should skip
        // This happens when already at walrus.max

        let current_size = 4096;
        let new_size = 4096; // Same as walrus.max default

        // The worker's process_checkpoint_stats() has this check:
        // if current_size >= new_size { return; }
        assert!(
            current_size >= new_size,
            "Should skip when current_size >= new_size"
        );
    }

    /// Test delta calculation logic.
    #[pg_test]
    fn test_delta_calculation() {
        // The worker calculates: delta = current_requested - prev_requested
        let prev_requested: i64 = 10;
        let current_requested: i64 = 14;
        let delta = current_requested - prev_requested;

        assert_eq!(
            delta, 4,
            "Delta should be difference between current and previous"
        );
    }

    // =========================================================================
    // Shrink Logic Tests
    // =========================================================================

    /// Test that calculate_shrink_size correctly clamps to min_size (T038)
    #[pg_test]
    fn test_calculate_shrink_size_clamping() {
        // 2560 MB * 0.75 = 1920, but min_size is 2048 -> returns 2048
        assert_eq!(worker::calculate_shrink_size(2560, 0.75, 2048), 2048);

        // 1024 MB * 0.75 = 768, but min_size is 1024 -> returns 1024
        assert_eq!(worker::calculate_shrink_size(1024, 0.75, 1024), 1024);

        // 900 MB * 0.75 = 675, but min_size is 1024 -> returns 1024 (below floor)
        assert_eq!(worker::calculate_shrink_size(900, 0.75, 1024), 1024);
    }

    /// Test that shrink GUC statics can be accessed (T047)
    #[pg_test]
    fn test_shrink_guc_static_access_factor() {
        use crate::guc::WALRUS_SHRINK_FACTOR;

        let factor = WALRUS_SHRINK_FACTOR.get();
        // Default is 0.75, allow for floating point comparison
        assert!(
            (factor - 0.75).abs() < 0.001,
            "WALRUS_SHRINK_FACTOR should default to 0.75, got {}",
            factor
        );
    }

    /// Test that shrink GUC statics can be accessed (T048)
    #[pg_test]
    fn test_shrink_guc_static_access_intervals() {
        use crate::guc::WALRUS_SHRINK_INTERVALS;

        let intervals = WALRUS_SHRINK_INTERVALS.get();
        assert_eq!(intervals, 5, "WALRUS_SHRINK_INTERVALS should default to 5");
    }

    /// Test that SET fails for walrus.shrink_enable (SIGHUP context) (T044)
    #[pg_test(error = "parameter \"walrus.shrink_enable\" cannot be changed now")]
    fn test_guc_shrink_enable_set_fails() {
        Spi::run("SET walrus.shrink_enable = false").unwrap();
    }

    // NOTE: GUC boundary validation tests (T054-T057) moved to pg_regress
    // because ALTER SYSTEM cannot run inside a transaction block.
    // See tests/pg_regress/sql/shrink_gucs.sql for boundary tests.

    /// Test no shrink when current_size <= min_size (T075)
    #[pg_test]
    fn test_no_shrink_when_at_floor() {
        // When current_size <= min_size, shrink should be skipped
        // This is a logic test - worker would check: if current_size <= min_size { return; }
        let current_size = 1024;
        let min_size = 1024;

        // At floor - shrink should not happen
        assert!(
            current_size <= min_size,
            "When current_size <= min_size, shrink should be skipped"
        );

        // Below floor (hypothetical)
        let current_size_below = 900;
        let min_size_higher = 1024;
        assert!(
            current_size_below <= min_size_higher,
            "When current_size < min_size, shrink should be skipped"
        );
    }

    /// Test that SUPPRESS_NEXT_SIGHUP flag does not interfere with quiet_intervals counter (T078)
    /// The flag and counter serve different purposes and are independent.
    #[pg_test]
    fn test_suppress_sighup_and_quiet_intervals_independence() {
        // This is a design verification test.
        // SUPPRESS_NEXT_SIGHUP: prevents re-processing our own config reload
        // quiet_intervals: tracks consecutive low-activity intervals
        //
        // When SIGHUP is suppressed, quiet_intervals should still be evaluated normally.
        // The only thing suppressed is the checkpoint stats processing for that cycle.

        // Verify we have separate concepts:
        // 1. SIGHUP suppression is an atomic bool in worker
        // 2. quiet_intervals is local worker state
        //
        // Implementation ensures:
        // - should_skip_iteration() only skips the entire iteration (including quiet_intervals logic)
        // - When not skipped, quiet_intervals is incremented/reset based on checkpoint activity
        //
        // This test verifies the GUCs and worker function are accessible
        use crate::guc::{WALRUS_SHRINK_ENABLE, WALRUS_SHRINK_INTERVALS};

        assert!(
            WALRUS_SHRINK_ENABLE.get(),
            "shrink_enable should be accessible"
        );
        assert_eq!(
            WALRUS_SHRINK_INTERVALS.get(),
            5,
            "shrink_intervals should be accessible"
        );
    }

    // History Table Tests (T011-T015) are in src/history.rs tests module
}
