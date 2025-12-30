//! ALTER SYSTEM execution for pg_walrus.
//!
//! This module handles modifying max_wal_size via ALTER SYSTEM SET,
//! constructing the necessary AST nodes and executing within a transaction.
//! Also provides cross-platform signaling to trigger configuration reloads.

use pgrx::pg_sys;
use std::ffi::CString;
use std::ptr;

/// Send SIGHUP to the postmaster to trigger configuration reload.
///
/// This is used after ALTER SYSTEM to apply configuration changes.
/// On Unix, sends SIGHUP signal directly via libc.
/// On Windows, signals via PostgreSQL's named event mechanism.
#[cfg(unix)]
pub fn signal_postmaster_reload() {
    unsafe {
        libc::kill(pg_sys::PostmasterPid, libc::SIGHUP);
    }
}

/// Send SIGHUP to the postmaster to trigger configuration reload.
///
/// On Windows, PostgreSQL uses named events for signal emulation.
/// The event name format is `Global\PostgreSQL.SIGHUP.<pid>`.
#[cfg(windows)]
pub fn signal_postmaster_reload() {
    // Windows API function declarations
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenEventA(
            dwDesiredAccess: u32,
            bInheritHandle: i32,
            lpName: *const i8,
        ) -> *mut std::ffi::c_void;
        fn SetEvent(hEvent: *mut std::ffi::c_void) -> i32;
        fn CloseHandle(hObject: *mut std::ffi::c_void) -> i32;
    }

    const EVENT_MODIFY_STATE: u32 = 0x0002;

    unsafe {
        let pid = pg_sys::PostmasterPid;
        let event_name = format!("Global\\PostgreSQL.SIGHUP.{}", pid);
        let c_name = CString::new(event_name).expect("CString::new failed");

        let handle = OpenEventA(EVENT_MODIFY_STATE, 0, c_name.as_ptr());
        if !handle.is_null() {
            SetEvent(handle);
            CloseHandle(handle);
        }
    }
}

/// Allocates and initializes a PostgreSQL node structure.
///
/// This is equivalent to PostgreSQL's makeNode() macro but works in Rust.
/// Uses palloc0 to zero-initialize the memory.
///
/// # Safety
/// Must be called when PostgreSQL memory context is valid.
#[inline]
unsafe fn make_node<T>() -> *mut T {
    let size = core::mem::size_of::<T>();
    // SAFETY: Called within a valid PostgreSQL memory context.
    unsafe { pg_sys::palloc0(size) as *mut T }
}

/// Constructs the AST nodes for ALTER SYSTEM SET max_wal_size = <value>.
///
/// # Safety
/// Caller must ensure this is called within a valid transaction context
/// and that PostgreSQL memory contexts are properly set up.
unsafe fn alter_max_wal_size(new_value: i32) {
    // SAFETY: All operations within this function are performed in a valid
    // PostgreSQL memory context and transaction.
    unsafe {
        // Allocate nodes in PostgreSQL memory context
        let alter_stmt: *mut pg_sys::AlterSystemStmt = make_node();
        let setstmt: *mut pg_sys::VariableSetStmt = make_node();
        let useval: *mut pg_sys::A_Const = make_node();

        // Configure A_Const with integer value (in MB)
        (*useval).type_ = pg_sys::NodeTag::T_A_Const;
        (*useval).isnull = false;
        // location field only exists in pg18+
        #[cfg(feature = "pg18")]
        {
            (*useval).location = -1;
        }
        (*useval).val.ival.type_ = pg_sys::NodeTag::T_Integer;
        (*useval).val.ival.ival = new_value;

        // Configure VariableSetStmt for max_wal_size
        let name = CString::new("max_wal_size").expect("CString::new failed");
        (*setstmt).type_ = pg_sys::NodeTag::T_VariableSetStmt;
        (*setstmt).kind = pg_sys::VariableSetKind::VAR_SET_VALUE;
        (*setstmt).name = pg_sys::pstrdup(name.as_ptr());
        (*setstmt).is_local = false;

        // jumble_args and location fields only exist in pg18+
        #[cfg(feature = "pg18")]
        {
            (*setstmt).jumble_args = false;
            (*setstmt).location = -1;
        }

        // Build the args list with the value using lappend
        (*setstmt).args = pg_sys::lappend(ptr::null_mut(), useval as *mut std::ffi::c_void);

        // Configure AlterSystemStmt
        (*alter_stmt).type_ = pg_sys::NodeTag::T_AlterSystemStmt;
        (*alter_stmt).setstmt = setstmt;

        // Execute ALTER SYSTEM
        pg_sys::AlterSystemSetConfigFile(alter_stmt);
    }
}

/// Execute ALTER SYSTEM SET max_wal_size = <new_value>.
///
/// This function detects the calling context:
/// - From SQL function context: Calls AlterSystemSetConfigFile directly
///   (we're already in a valid memory/transaction context)
/// - From background worker: Sets up transaction, calls, then commits
///
/// Returns Ok(()) on success, Err with a message on failure.
pub fn execute_alter_system(new_value: i32) -> Result<(), &'static str> {
    unsafe {
        // Check if we're already in a transaction (e.g., called from SQL function)
        let in_transaction = pg_sys::IsTransactionState();

        if in_transaction {
            // SQL function context: call AlterSystemSetConfigFile directly
            // No transaction handling needed - we're already in a valid context
            alter_max_wal_size(new_value);
        } else {
            // Background worker context: need to set up transaction
            if pg_sys::CurrentResourceOwner.is_null() {
                let name = CString::new("pg_walrus").expect("CString::new failed");
                pg_sys::CurrentResourceOwner =
                    pg_sys::ResourceOwnerCreate(ptr::null_mut(), name.as_ptr());
            }
            pg_sys::StartTransactionCommand();
            alter_max_wal_size(new_value);
            pg_sys::CommitTransactionCommand();
        }
    }

    Ok(())
}
