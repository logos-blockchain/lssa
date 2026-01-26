//! Token transfer functions.

use std::{ffi::CString, ptr};

use common::error::ExecutionFailureKind;
use nssa::AccountId;
use wallet::program_facades::native_token_transfer::NativeTokenTransfer;

use crate::{
    block_on,
    error::{print_error, WalletFfiError},
    types::{FfiBytes32, FfiTransferResult, WalletHandle},
    wallet::get_wallet,
};

/// Send a public token transfer.
///
/// Transfers tokens from one public account to another on the network.
///
/// # Parameters
/// - `handle`: Valid wallet handle
/// - `from`: Source account ID (must be owned by this wallet)
/// - `to`: Destination account ID
/// - `amount`: Amount to transfer as little-endian [u8; 16]
/// - `out_result`: Output pointer for transfer result
///
/// # Returns
/// - `Success` if the transfer was submitted successfully
/// - `InsufficientFunds` if the source account doesn't have enough balance
/// - `KeyNotFound` if the source account's signing key is not in this wallet
/// - Error code on other failures
///
/// # Memory
/// The result must be freed with `wallet_ffi_free_transfer_result()`.
///
/// # Safety
/// - `handle` must be a valid wallet handle from `wallet_ffi_create_new` or `wallet_ffi_open`
/// - `from` must be a valid pointer to a `FfiBytes32` struct
/// - `to` must be a valid pointer to a `FfiBytes32` struct
/// - `amount` must be a valid pointer to a `[u8; 16]` array
/// - `out_result` must be a valid pointer to a `FfiTransferResult` struct
#[no_mangle]
pub unsafe extern "C" fn wallet_ffi_transfer_public(
    handle: *mut WalletHandle,
    from: *const FfiBytes32,
    to: *const FfiBytes32,
    amount: *const [u8; 16],
    out_result: *mut FfiTransferResult,
) -> WalletFfiError {
    let wrapper = match get_wallet(handle) {
        Ok(w) => w,
        Err(e) => return e,
    };

    if from.is_null() || to.is_null() || amount.is_null() || out_result.is_null() {
        print_error("Null pointer argument");
        return WalletFfiError::NullPointer;
    }

    let wallet = match wrapper.core.lock() {
        Ok(w) => w,
        Err(e) => {
            print_error(format!("Failed to lock wallet: {}", e));
            return WalletFfiError::InternalError;
        }
    };

    let from_id = AccountId::new(unsafe { (*from).data });
    let to_id = AccountId::new(unsafe { (*to).data });
    let amount = u128::from_le_bytes(unsafe { *amount });

    let transfer = NativeTokenTransfer(&wallet);

    match block_on(transfer.send_public_transfer(from_id, to_id, amount)) {
        Ok(Ok(response)) => {
            let tx_hash = CString::new(response.tx_hash)
                .map(|s| s.into_raw())
                .unwrap_or(ptr::null_mut());

            unsafe {
                (*out_result).tx_hash = tx_hash;
                (*out_result).success = true;
            }
            WalletFfiError::Success
        }
        Ok(Err(e)) => {
            print_error(format!("Transfer failed: {:?}", e));
            unsafe {
                (*out_result).tx_hash = ptr::null_mut();
                (*out_result).success = false;
            }
            match e {
                ExecutionFailureKind::InsufficientFundsError => WalletFfiError::InsufficientFunds,
                ExecutionFailureKind::KeyNotFoundError => WalletFfiError::KeyNotFound,
                ExecutionFailureKind::SequencerError => WalletFfiError::NetworkError,
                ExecutionFailureKind::SequencerClientError(_) => WalletFfiError::NetworkError,
                _ => WalletFfiError::InternalError,
            }
        }
        Err(e) => e,
    }
}

/// Register a public account on the network.
///
/// This initializes a public account on the blockchain. The account must be
/// owned by this wallet.
///
/// # Parameters
/// - `handle`: Valid wallet handle
/// - `account_id`: Account ID to register
/// - `out_result`: Output pointer for registration result
///
/// # Returns
/// - `Success` if the registration was submitted successfully
/// - Error code on failure
///
/// # Memory
/// The result must be freed with `wallet_ffi_free_transfer_result()`.
///
/// # Safety
/// - `handle` must be a valid wallet handle from `wallet_ffi_create_new` or `wallet_ffi_open`
/// - `account_id` must be a valid pointer to a `FfiBytes32` struct
/// - `out_result` must be a valid pointer to a `FfiTransferResult` struct
#[no_mangle]
pub unsafe extern "C" fn wallet_ffi_register_public_account(
    handle: *mut WalletHandle,
    account_id: *const FfiBytes32,
    out_result: *mut FfiTransferResult,
) -> WalletFfiError {
    let wrapper = match get_wallet(handle) {
        Ok(w) => w,
        Err(e) => return e,
    };

    if account_id.is_null() || out_result.is_null() {
        print_error("Null pointer argument");
        return WalletFfiError::NullPointer;
    }

    let wallet = match wrapper.core.lock() {
        Ok(w) => w,
        Err(e) => {
            print_error(format!("Failed to lock wallet: {}", e));
            return WalletFfiError::InternalError;
        }
    };

    let account_id = AccountId::new(unsafe { (*account_id).data });

    let transfer = NativeTokenTransfer(&wallet);

    match block_on(transfer.register_account(account_id)) {
        Ok(Ok(response)) => {
            let tx_hash = CString::new(response.tx_hash)
                .map(|s| s.into_raw())
                .unwrap_or(ptr::null_mut());

            unsafe {
                (*out_result).tx_hash = tx_hash;
                (*out_result).success = true;
            }
            WalletFfiError::Success
        }
        Ok(Err(e)) => {
            print_error(format!("Registration failed: {:?}", e));
            unsafe {
                (*out_result).tx_hash = ptr::null_mut();
                (*out_result).success = false;
            }
            match e {
                ExecutionFailureKind::KeyNotFoundError => WalletFfiError::KeyNotFound,
                ExecutionFailureKind::SequencerError => WalletFfiError::NetworkError,
                ExecutionFailureKind::SequencerClientError(_) => WalletFfiError::NetworkError,
                _ => WalletFfiError::InternalError,
            }
        }
        Err(e) => e,
    }
}

/// Free a transfer result returned by `wallet_ffi_transfer_public` or
/// `wallet_ffi_register_public_account`.
///
/// # Safety
/// The result must be either null or a valid result from a transfer function.
#[no_mangle]
pub unsafe extern "C" fn wallet_ffi_free_transfer_result(result: *mut FfiTransferResult) {
    if result.is_null() {
        return;
    }

    unsafe {
        let result = &*result;
        if !result.tx_hash.is_null() {
            drop(CString::from_raw(result.tx_hash));
        }
    }
}
