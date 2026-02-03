use std::{
    ffi::{c_char, CString},
    path::Path,
};

use tokio::runtime::Handle;
use wallet::WalletCore;
use wallet_ffi::{error, FfiBytes32, FfiError, WalletHandle};

extern "C" {
    fn wallet_ffi_create_account_public(
        handle: *mut WalletHandle,
        out_account_id: *mut FfiBytes32,
    ) -> error::WalletFfiError;

    fn wallet_ffi_create_account_private(
        handle: *mut WalletHandle,
        out_account_id: *mut FfiBytes32,
    ) -> error::WalletFfiError;

    fn wallet_ffi_create_new(
        config_path: *const c_char,
        storage_path: *const c_char,
        password: *const c_char,
    ) -> *mut WalletHandle;

    fn wallet_ffi_save(handle: *mut WalletHandle) -> error::WalletFfiError;
}

use tempfile::tempdir;

unsafe fn new_wallet_ffi_for_tests(password: &str) -> *mut WalletHandle {
    let tempdir = tempdir().unwrap();
    let config_path = tempdir.path().join("wallet_config.json");
    let storage_path = tempdir.path().join("storage.json");
    let config_path_c = CString::new(config_path.to_str().unwrap()).unwrap();
    let storage_path_c = CString::new(storage_path.to_str().unwrap()).unwrap();
    let password = CString::new(password).unwrap();

    wallet_ffi_create_new(
        config_path_c.as_ptr(),
        storage_path_c.as_ptr(),
        password.as_ptr(),
    )
}

fn new_wallet_rust_for_tests(password: &str) -> WalletCore {
    let tempdir = tempdir().unwrap();
    let config_path = tempdir.path().join("wallet_config.json");
    let storage_path = tempdir.path().join("storage.json");

    WalletCore::new_init_storage(
        config_path.to_path_buf(),
        storage_path.to_path_buf(),
        None,
        password.to_string(),
    )
    .unwrap()
}

#[test]
fn test_create_public_accounts() {
    let password = "password_for_tests";
    let n_accounts = 10;
    // First `n_accounts` public accounts created with Rust wallet
    let new_public_account_ids_rust = {
        let mut account_ids = Vec::new();

        let mut wallet_rust = new_wallet_rust_for_tests(password);
        for _ in 0..n_accounts {
            let account_id = wallet_rust.create_new_account_public(None).0;
            account_ids.push(*account_id.value());
        }
        account_ids
    };

    // First `n_accounts` public accounts created with wallet FFI
    let new_public_account_ids_ffi = unsafe {
        let mut account_ids = Vec::new();

        let wallet_ffi_handle = new_wallet_ffi_for_tests(password);
        for _ in 0..n_accounts {
            let mut out_account_id = FfiBytes32::from_bytes([0; 32]);
            wallet_ffi_create_account_public(
                wallet_ffi_handle,
                (&mut out_account_id) as *mut FfiBytes32,
            );
            account_ids.push(out_account_id.data);
        }
        account_ids
    };

    assert_eq!(new_public_account_ids_ffi, new_public_account_ids_rust)
}

#[test]
fn test_create_private_accounts() {
    let password = "password_for_tests";
    let n_accounts = 10;
    // First `n_accounts` private accounts created with Rust wallet
    let new_private_account_ids_rust = {
        let mut account_ids = Vec::new();

        let mut wallet_rust = new_wallet_rust_for_tests(password);
        for _ in 0..n_accounts {
            let account_id = wallet_rust.create_new_account_private(None).0;
            account_ids.push(*account_id.value());
        }
        account_ids
    };

    // First `n_accounts` private accounts created with wallet FFI
    let new_private_account_ids_ffi = unsafe {
        let mut account_ids = Vec::new();

        let wallet_ffi_handle = new_wallet_ffi_for_tests(password);
        for _ in 0..n_accounts {
            let mut out_account_id = FfiBytes32::from_bytes([0; 32]);
            wallet_ffi_create_account_private(
                wallet_ffi_handle,
                (&mut out_account_id) as *mut FfiBytes32,
            );
            account_ids.push(out_account_id.data);
        }
        account_ids
    };

    assert_eq!(new_private_account_ids_ffi, new_private_account_ids_rust)
}
