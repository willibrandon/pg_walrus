//! Shared memory state for pg_walrus.
//!
//! This module defines the `WalrusState` struct stored in PostgreSQL shared memory,
//! allowing both the background worker and SQL functions to access real-time state.
//!
//! The state includes:
//! - `quiet_intervals`: Consecutive intervals with low checkpoint activity
//! - `total_adjustments`: Total sizing adjustments since PostgreSQL start
//! - `prev_requested`: Previous checkpoint count baseline
//! - `last_check_time`: Unix timestamp of last analysis cycle
//! - `last_adjustment_time`: Unix timestamp of last sizing adjustment

use pgrx::lwlock::PgLwLock;
use pgrx::shmem::PGRXSharedMemory;

/// Worker state exposed via PostgreSQL shared memory for real-time SQL function access.
///
/// This struct must implement `Copy`, `Clone`, and `Default` for shared memory safety.
/// All fields use primitive types that can be safely read/written across process boundaries.
#[derive(Copy, Clone, Default, Debug)]
pub struct WalrusState {
    /// Consecutive intervals with low checkpoint activity (delta < threshold).
    /// Reset to 0 after grow or shrink operations.
    pub quiet_intervals: i32,

    /// Total number of sizing adjustments made since PostgreSQL start.
    /// Monotonically increasing except on reset.
    pub total_adjustments: i64,

    /// Previous checkpoint count baseline (for delta calculation).
    /// Updated each monitoring cycle after the first iteration.
    pub prev_requested: i64,

    /// Unix timestamp of last analysis cycle (seconds since epoch).
    /// Value of 0 means worker hasn't completed first cycle.
    pub last_check_time: i64,

    /// Unix timestamp of last sizing adjustment (seconds since epoch).
    /// Value of 0 means no adjustments have occurred.
    pub last_adjustment_time: i64,
}

// SAFETY: WalrusState contains only primitive types (i32, i64) which are Copy
// and can be safely accessed across PostgreSQL backends via shared memory.
// The struct has no pointers or non-Copy fields.
unsafe impl PGRXSharedMemory for WalrusState {}

/// Global shared memory state protected by a lightweight lock.
///
/// Use `.share()` for read access and `.exclusive()` for write access.
/// The lock name "walrus_state" is registered in PostgreSQL's shared memory.
///
/// NOTE: pg_shmem_init! is called in lib.rs _PG_init() since the macro
/// requires a direct identifier, not a path.
pub static WALRUS_STATE: PgLwLock<WalrusState> = unsafe { PgLwLock::new(c"walrus_state") };

/// Read the current shared memory state with a shared lock.
///
/// Returns a copy of the state. The lock is held only during the read.
///
/// # Example
///
/// ```ignore
/// let state = shmem::read_state();
/// println!("Quiet intervals: {}", state.quiet_intervals);
/// ```
#[inline]
pub fn read_state() -> WalrusState {
    *WALRUS_STATE.share()
}

/// Update the shared memory state with an exclusive lock.
///
/// The provided closure receives a mutable reference to the state.
/// The lock is held only during the update.
///
/// # Example
///
/// ```ignore
/// shmem::update_state(|state| {
///     state.quiet_intervals += 1;
///     state.last_check_time = now_unix();
/// });
/// ```
#[inline]
pub fn update_state<F>(f: F)
where
    F: FnOnce(&mut WalrusState),
{
    let mut state = WALRUS_STATE.exclusive();
    f(&mut state);
}

/// Reset all shared memory state to zero.
///
/// Called by `walrus.reset()` to clear counters and timestamps.
/// The worker will see the reset state on its next cycle.
#[inline]
pub fn reset_state() {
    let mut state = WALRUS_STATE.exclusive();
    state.quiet_intervals = 0;
    state.total_adjustments = 0;
    state.prev_requested = 0;
    state.last_check_time = 0;
    state.last_adjustment_time = 0;
}

/// Get current Unix timestamp in seconds.
///
/// Helper function for consistent timestamp generation.
#[inline]
pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
