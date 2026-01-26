//! NSSA Wallet FFI Library
//!
//! This crate provides C-compatible bindings for the NSSA wallet functionality.
//!
//! # Usage
//!
//! 1. Initialize the runtime with `wallet_ffi_init_runtime()`
//! 2. Create or open a wallet with `wallet_ffi_create_new()` or `wallet_ffi_open()`
//! 3. Use the wallet functions to manage accounts and transfers
//! 4. Destroy the wallet with `wallet_ffi_destroy()` when done
//!
//! # Thread Safety
//!
//! All functions are thread-safe. The wallet handle uses internal locking
//! to ensure safe concurrent access.
//!
//! # Memory Management
//!
//! - Functions returning pointers allocate memory that must be freed
//! - Use the corresponding `wallet_ffi_free_*` function to free memory
//! - Never free memory returned by FFI using standard C `free()`

pub mod account;
pub mod error;
pub mod keys;
pub mod sync;
pub mod transfer;
pub mod types;
pub mod wallet;

// Re-export public types for cbindgen
pub use error::WalletFfiError as FfiError;
use tokio::runtime::Handle;
pub use types::*;

use crate::error::{print_error, WalletFfiError};

/// Get a reference to the global runtime.
pub(crate) fn get_runtime() -> Result<Handle, WalletFfiError> {
    Handle::try_current().map_err(|_| WalletFfiError::RuntimeError)
}

/// Run an async future on the global runtime, blocking until completion.
pub(crate) fn block_on<F: std::future::Future>(future: F) -> Result<F::Output, WalletFfiError> {
    let runtime = get_runtime()?;
    Ok(runtime.block_on(future))
}

/// Initialize the global Tokio runtime.
///
/// This must be called before any async operations (like network calls).
/// Safe to call multiple times - subsequent calls are no-ops.
///
/// # Returns
/// - `Success` if the runtime was initialized or already exists
/// - `RuntimeError` if runtime creation failed
#[no_mangle]
pub extern "C" fn wallet_ffi_init_runtime() -> WalletFfiError {
    let result = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build();

    match result {
        Ok(_) => WalletFfiError::Success,
        Err(e) => {
            print_error(format!("Failed to initialize runtime: {}", e));
            WalletFfiError::RuntimeError
        }
    }
}
