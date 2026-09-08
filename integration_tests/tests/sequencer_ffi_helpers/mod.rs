#![allow(dead_code, reason = "helper module used only by FFI test binaries")]

use std::ffi::c_char;

use anyhow::Result;
use integration_tests::L2_TO_L1_TIMEOUT;
use sequencer_ffi::{
    OperationStatus, Runtime, SequencerServiceFFI,
    api::{
        PointerResult,
        lifecycle::InitializedSequencerServiceFFIResult,
        query::LastBlockIdResult,
        types::{FfiBlockId, block::FfiBlockOpt},
    },
};

unsafe extern "C" {
    pub unsafe fn query_last_block(sequencer: *const SequencerServiceFFI) -> LastBlockIdResult;

    pub unsafe fn query_block(
        sequencer: *const SequencerServiceFFI,
        block_id: FfiBlockId,
    ) -> PointerResult<FfiBlockOpt, OperationStatus>;

    pub unsafe fn start_sequencer(
        runtime: *const Runtime,
        config_path: *const c_char,
    ) -> InitializedSequencerServiceFFIResult;

    pub unsafe fn stop_sequencer(sequencer: *mut SequencerServiceFFI) -> OperationStatus;
}

pub fn wait_for_sequencer_ffi_block(
    sequencer: &SequencerServiceFFI,
    min_block_id: u64,
) -> Result<u64> {
    let start = std::time::Instant::now();
    loop {
        // SAFETY: `sequencer` is a valid reference for the duration of the call.
        let res = unsafe { query_last_block(std::ptr::from_ref(sequencer)) };
        if res.error.is_ok() && res.is_some && res.block_id >= min_block_id {
            return Ok(res.block_id);
        }
        if start.elapsed() >= L2_TO_L1_TIMEOUT {
            anyhow::bail!(
                "Sequencer FFI did not reach block {min_block_id} within {:?}. Last observed block id: {}",
                L2_TO_L1_TIMEOUT,
                res.block_id
            );
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
}
