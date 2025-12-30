//! ALTER SYSTEM execution for pg_walrus.
//!
//! This module handles modifying max_wal_size via ALTER SYSTEM SET,
//! constructing the necessary AST nodes and executing within a transaction.

use pgrx::pg_sys;
use std::ffi::CString;
use std::ptr;

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
