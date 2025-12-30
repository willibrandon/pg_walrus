//! Checkpoint statistics access for pg_walrus.
//!
//! This module provides version-specific access to PostgreSQL checkpoint statistics
//! and the checkpoint_timeout GUC variable (which is not exposed by pgrx).

use pgrx::pg_sys;
use std::ffi::c_int;
use std::time::Duration;

/// Returns the raw CheckPointTimeout value in seconds (for testing).
#[cfg(any(test, feature = "pg_test"))]
#[inline]
pub fn checkpoint_timeout_secs() -> i32 {
    // SAFETY: CheckPointTimeout is exported by PostgreSQL with PGDLLIMPORT,
    // guaranteed to exist and be initialized before any extension code runs.
    unsafe { CheckPointTimeout }
}

// Direct access to PostgreSQL's checkpoint-related GUC variables.
// These are exported by PostgreSQL with PGDLLIMPORT but not included
// in pgrx's default bindgen headers.
unsafe extern "C" {
    /// Checkpoint timeout in seconds (default 300, range 30-86400).
    /// Defined in src/backend/postmaster/checkpointer.c
    /// Declared in src/include/postmaster/bgwriter.h
    static CheckPointTimeout: c_int;
}

/// Returns the checkpoint_timeout GUC value as a Duration.
///
/// This is used as the background worker's wait interval, matching
/// the checkpoint monitoring cycle.
#[inline]
pub fn checkpoint_timeout() -> Duration {
    // SAFETY: CheckPointTimeout is exported by PostgreSQL with PGDLLIMPORT,
    // guaranteed to exist and be initialized before any extension code runs.
    let secs = unsafe { CheckPointTimeout } as u64;
    Duration::from_secs(secs)
}

/// Returns the current count of forced (requested) checkpoints since PostgreSQL startup.
///
/// Returns -1 if checkpoint statistics are unavailable (null pointer from pgstat).
///
/// The field name differs between PostgreSQL versions:
/// - PG 15-16: `requested_checkpoints`
/// - PG 17+: `num_requested`
#[cfg(any(feature = "pg15", feature = "pg16"))]
pub fn get_requested_checkpoints() -> i64 {
    unsafe {
        // Clear snapshot to get fresh statistics
        pg_sys::pgstat_clear_snapshot();
        let stats = pg_sys::pgstat_fetch_stat_checkpointer();
        if stats.is_null() {
            return -1;
        }
        (*stats).requested_checkpoints
    }
}

/// Returns the current count of forced (requested) checkpoints since PostgreSQL startup.
///
/// Returns -1 if checkpoint statistics are unavailable (null pointer from pgstat).
///
/// The field name differs between PostgreSQL versions:
/// - PG 15-16: `requested_checkpoints`
/// - PG 17+: `num_requested`
#[cfg(any(feature = "pg17", feature = "pg18"))]
pub fn get_requested_checkpoints() -> i64 {
    unsafe {
        // Clear snapshot to get fresh statistics
        pg_sys::pgstat_clear_snapshot();
        let stats = pg_sys::pgstat_fetch_stat_checkpointer();
        if stats.is_null() {
            return -1;
        }
        (*stats).num_requested
    }
}

/// Returns the current max_wal_size value in MB.
///
/// This reads directly from PostgreSQL's global variable, which is
/// automatically updated when configuration is reloaded.
#[inline]
pub fn get_current_max_wal_size() -> i32 {
    // SAFETY: max_wal_size_mb is a global PostgreSQL variable, always valid.
    unsafe { pg_sys::max_wal_size_mb }
}
