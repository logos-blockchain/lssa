use std::ffi::{c_char, CString};

use wallet::WalletCore;
use wallet_ffi::{error, FfiBytes32, FfiError, WalletHandle};

extern "C" {
    fn wallet_ffi_init_runtime() -> error::WalletFfiError;

    fn wallet_ffi_destroy(handle: *mut WalletHandle);

    fn wallet_ffi_create_account_public(
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

#[test]
fn test() {
    unsafe {
        let result = wallet_ffi_init_runtime();
        println!("wallet init runtim result: {:?}", result);
    }

    let tempdir = tempdir().unwrap();
    let config_path = tempdir.path().join("wallet_config.json");
    let storage_path = tempdir.path().join("storage.json");
    let config_path_c = CString::new(config_path.to_str().unwrap()).unwrap();
    let storage_path_c = CString::new(storage_path.to_str().unwrap()).unwrap();
    let password = CString::new("").unwrap();

    unsafe {
        let wallet_handle = wallet_ffi_create_new(
            config_path_c.as_ptr(),
            storage_path_c.as_ptr(),
            password.as_ptr(),
        );

        let mut out_account_id = FfiBytes32::from_bytes([0; 32]);

        let result = wallet_ffi_create_account_public(
            wallet_handle,
            (&mut out_account_id) as *mut FfiBytes32,
        );
        println!("{:?}", out_account_id.data);
        println!("create result: {:?}", result);

        let result = wallet_ffi_save(wallet_handle);
        println!("save result: {:?}", result);

        wallet_ffi_destroy(wallet_handle);
    }
    // let mut wallet_core = WalletCore::new_update_chain(config_path.to_path_buf(),
    // storage_path.to_path_buf(), None).unwrap(); let (account_id, _) =
    // wallet_core.create_new_account_public(None); println!("{:?}", account_id);
}
