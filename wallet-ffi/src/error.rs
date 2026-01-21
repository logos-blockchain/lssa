//! Error handling for the FFI layer.
//!
//! Uses numeric error codes with a thread-local last error message.

use std::cell::RefCell;
use std::ffi::{c_char, CString};
use std::ptr;

/// Error codes returned by FFI functions.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalletFfiError {
    /// Operation completed successfully
    Success = 0,
    /// A null pointer was passed where a valid pointer was expected
    NullPointer = 1,
    /// Invalid UTF-8 string
    InvalidUtf8 = 2,
    /// Wallet handle is not initialized
    WalletNotInitialized = 3,
    /// Configuration error
    ConfigError = 4,
    /// Storage/persistence error
    StorageError = 5,
    /// Network/RPC error
    NetworkError = 6,
    /// Account not found
    AccountNotFound = 7,
    /// Key not found for account
    KeyNotFound = 8,
    /// Insufficient funds for operation
    InsufficientFunds = 9,
    /// Invalid account ID format
    InvalidAccountId = 10,
    /// Tokio runtime error
    RuntimeError = 11,
    /// Password required but not provided
    PasswordRequired = 12,
    /// Block synchronization error
    SyncError = 13,
    /// Serialization/deserialization error
    SerializationError = 14,
    /// Internal error (catch-all)
    InternalError = 99,
}

// Thread-local storage for the last error message
thread_local! {
    static LAST_ERROR: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Set the last error message for the current thread.
pub fn set_last_error(msg: impl Into<String>) {
    LAST_ERROR.with(|e| {
        *e.borrow_mut() = Some(msg.into());
    });
}

/// Clear the last error message.
pub fn clear_last_error() {
    LAST_ERROR.with(|e| {
        *e.borrow_mut() = None;
    });
}

/// Get the last error message.
///
/// Returns a pointer to a null-terminated string, or null if no error is set.
/// The caller owns the returned string and must free it with
/// `wallet_ffi_free_error_string`.
#[no_mangle]
pub extern "C" fn wallet_ffi_get_last_error() -> *mut c_char {
    LAST_ERROR.with(|e| match e.borrow_mut().take() {
        Some(msg) => CString::new(msg)
            .map(|s| s.into_raw())
            .unwrap_or(ptr::null_mut()),
        None => ptr::null_mut(),
    })
}

/// Free an error string returned by `wallet_ffi_get_last_error`.
///
/// # Safety
/// The pointer must be either null or a valid pointer returned by
/// `wallet_ffi_get_last_error`.
#[no_mangle]
pub extern "C" fn wallet_ffi_free_error_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe {
            drop(CString::from_raw(ptr));
        }
    }
}
