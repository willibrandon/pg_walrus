//! Checkpoint statistics access for pg_walrus.
//!
//! This module provides version-specific access to PostgreSQL checkpoint statistics
//! and the checkpoint_timeout GUC variable.

use pgrx::pg_sys;
use std::time::Duration;

#[cfg(unix)]
use std::ffi::c_int;
#[cfg(windows)]
use std::ffi::{CStr, CString};
#[cfg(windows)]
use std::ptr;

// On Unix, we can access CheckPointTimeout directly via extern declaration.
// On Windows, PostgreSQL DLL symbols require proper import which extern "C"
// doesn't provide, so we use GetConfigOptionByName instead.
#[cfg(unix)]
unsafe extern "C" {
    /// Checkpoint timeout in seconds (default 300, range 30-86400).
    /// Defined in src/backend/postmaster/checkpointer.c
    /// Declared in src/include/postmaster/bgwriter.h
    static CheckPointTimeout: c_int;
}

/// Returns the raw CheckPointTimeout value in seconds (for testing).
#[cfg(all(unix, any(test, feature = "pg_test")))]
#[inline]
pub fn checkpoint_timeout_secs() -> i32 {
    unsafe { CheckPointTimeout }
}

/// Returns the raw CheckPointTimeout value in seconds (for testing).
#[cfg(all(windows, any(test, feature = "pg_test")))]
#[inline]
pub fn checkpoint_timeout_secs() -> i32 {
    checkpoint_timeout().as_secs() as i32
}

/// Returns the checkpoint_timeout GUC value as a Duration.
///
/// On Unix, reads directly from PostgreSQL's CheckPointTimeout global variable.
/// On Windows, uses GetConfigOptionByName to retrieve the GUC value.
#[cfg(unix)]
#[inline]
pub fn checkpoint_timeout() -> Duration {
    // SAFETY: CheckPointTimeout is exported by PostgreSQL with PGDLLIMPORT,
    // guaranteed to exist and be initialized before any extension code runs.
    let secs = unsafe { CheckPointTimeout } as u64;
    Duration::from_secs(secs)
}

/// Returns the checkpoint_timeout GUC value as a Duration.
///
/// On Windows, uses GetConfigOptionByName to retrieve the GUC value
/// since direct extern symbol access doesn't work with PostgreSQL DLLs.
#[cfg(windows)]
#[inline]
pub fn checkpoint_timeout() -> Duration {
    unsafe {
        let name = CString::new("checkpoint_timeout").expect("CString::new failed");
        let value_ptr = pg_sys::GetConfigOptionByName(name.as_ptr(), ptr::null_mut(), false);
        if value_ptr.is_null() {
            return Duration::from_secs(300); // default
        }
        let value_str = CStr::from_ptr(value_ptr).to_str().unwrap_or("300s");
        parse_interval_to_secs(value_str)
    }
}

/// Parse a PostgreSQL interval string to seconds Duration.
///
/// Handles formats like "5min", "300s", "300", etc.
#[cfg(windows)]
fn parse_interval_to_secs(s: &str) -> Duration {
    let s = s.trim();

    // Try to parse as plain number (seconds)
    if let Ok(secs) = s.parse::<u64>() {
        return Duration::from_secs(secs);
    }

    // Handle suffixes: s, min, h, d
    if let Some(num_str) = s.strip_suffix("ms") {
        if let Ok(ms) = num_str.trim().parse::<u64>() {
            return Duration::from_millis(ms);
        }
    } else if let Some(num_str) = s.strip_suffix('s') {
        if let Ok(secs) = num_str.trim().parse::<u64>() {
            return Duration::from_secs(secs);
        }
    } else if let Some(num_str) = s.strip_suffix("min") {
        if let Ok(mins) = num_str.trim().parse::<u64>() {
            return Duration::from_secs(mins * 60);
        }
    } else if let Some(num_str) = s.strip_suffix('h') {
        if let Ok(hours) = num_str.trim().parse::<u64>() {
            return Duration::from_secs(hours * 3600);
        }
    } else if let Some(num_str) = s.strip_suffix('d') {
        if let Ok(days) = num_str.trim().parse::<u64>() {
            return Duration::from_secs(days * 86400);
        }
    }

    // Default fallback
    Duration::from_secs(300)
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
