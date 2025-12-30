//! GUC (Grand Unified Configuration) parameter definitions for pg_walrus.
//!
//! This module defines the runtime configuration parameters:
//! - `walrus.enable`: Enable/disable automatic WAL size adjustment
//! - `walrus.max`: Maximum allowed max_wal_size (in MB)
//! - `walrus.threshold`: Forced checkpoint count threshold before resize

use pgrx::guc::{GucContext, GucFlags, GucRegistry, GucSetting};
use pgrx::pg_sys;

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

/// Register all pg_walrus GUC parameters with PostgreSQL.
///
/// This function registers the three GUC parameters using GucContext::Sighup,
/// allowing runtime changes via ALTER SYSTEM and pg_reload_conf().
pub fn register_gucs() {
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

    // Reserve the "walrus" GUC prefix to prevent other extensions from using it.
    // This matches pg_walsizer's behavior with MarkGUCPrefixReserved("walsizer").
    unsafe {
        pg_sys::MarkGUCPrefixReserved(c"walrus".as_ptr());
    }
}
