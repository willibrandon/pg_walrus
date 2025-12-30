//! pg_walrus - Automatic WAL size management for PostgreSQL
//!
//! This extension monitors checkpoint activity and automatically adjusts
//! `max_wal_size` to prevent performance-degrading forced checkpoints.

mod algorithm;
mod config;
mod functions;
mod guc;
mod history;
mod shmem;
mod stats;
mod worker;

use pgrx::bgworkers::{BackgroundWorkerBuilder, BgWorkerStartTime};
use pgrx::pg_shmem_init;
use pgrx::prelude::*;

// Re-export WALRUS_STATE at crate level so pg_shmem_init! can see it as an identifier
use shmem::WALRUS_STATE;

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
    action TEXT NOT NULL CHECK (action IN ('increase', 'decrease', 'capped', 'dry_run', 'skipped')),
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
COMMENT ON COLUMN walrus.history.action IS 'Decision type: increase, decrease, capped, dry_run, or skipped';
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
    use crate::functions;
    use pgrx::JsonB;
    use pgrx::datum::TimestampWithTimeZone;
    use pgrx::prelude::*;

    /// Returns current extension status as JSONB.
    ///
    /// # Example
    ///
    /// ```sql
    /// SELECT walrus.status();
    /// ```
    #[pg_extern]
    fn status() -> JsonB {
        functions::status()
    }

    /// Returns adjustment history as SETOF RECORD.
    ///
    /// # Example
    ///
    /// ```sql
    /// SELECT * FROM walrus.history();
    /// ```
    #[allow(clippy::type_complexity)]
    #[pg_extern]
    fn history() -> Result<
        pgrx::iter::TableIterator<
            'static,
            (
                pgrx::name!(timestamp, TimestampWithTimeZone),
                pgrx::name!(action, String),
                pgrx::name!(old_size_mb, i32),
                pgrx::name!(new_size_mb, i32),
                pgrx::name!(forced_checkpoints, i64),
                pgrx::name!(reason, Option<String>),
            ),
        >,
        spi::Error,
    > {
        functions::history_srf()
    }

    /// Returns sizing recommendation as JSONB.
    ///
    /// # Example
    ///
    /// ```sql
    /// SELECT walrus.recommendation();
    /// ```
    #[pg_extern]
    fn recommendation() -> JsonB {
        functions::recommendation()
    }

    /// Triggers immediate analysis with optional execution.
    ///
    /// # Arguments
    ///
    /// * `apply` - If true, execute the recommendation (superuser only)
    ///
    /// # Example
    ///
    /// ```sql
    /// SELECT walrus.analyze();
    /// SELECT walrus.analyze(apply := true);
    /// ```
    #[pg_extern]
    fn analyze(apply: pgrx::default!(bool, false)) -> Result<JsonB, spi::Error> {
        functions::analyze(apply)
    }

    /// Resets extension state (superuser only).
    ///
    /// Clears history table and shared memory counters.
    ///
    /// # Example
    ///
    /// ```sql
    /// SELECT walrus.reset();
    /// ```
    #[pg_extern]
    fn reset() -> Result<bool, spi::Error> {
        functions::reset()
    }

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
        functions::cleanup_history()
    }
}

/// Extension initialization entry point.
///
/// Called by PostgreSQL when the extension is loaded. When loaded via
/// `shared_preload_libraries`, registers GUC parameters, shared memory, and
/// the background worker. When loaded via CREATE EXTENSION (after server start),
/// only GUC parameters are available (background worker and shared memory
/// registration require shared_preload_libraries).
#[allow(non_snake_case)]
#[pg_guard]
pub extern "C-unwind" fn _PG_init() {
    // Register GUC parameters (always available, even without shared_preload_libraries)
    guc::register_gucs();

    // Background worker and shared memory registration ONLY work during
    // shared_preload_libraries loading. If loaded via CREATE EXTENSION after
    // server start, skip registration.
    let in_shared_preload = unsafe { pgrx::pg_sys::process_shared_preload_libraries_in_progress };
    if !in_shared_preload {
        // Not loaded via shared_preload_libraries - bgworker/shmem registration not possible
        return;
    }

    // Initialize shared memory for worker state (must be before BackgroundWorkerBuilder)
    // Allow unexpected_cfgs from pgrx macro that checks pg13/pg14 features we don't support
    #[allow(unexpected_cfgs)]
    {
        pg_shmem_init!(WALRUS_STATE);
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
// Tests are in separate files to keep lib.rs under 900 LOC
#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    include!("tests.rs");
    include!("rate_limit_tests.rs");
}
