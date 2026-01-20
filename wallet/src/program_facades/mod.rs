//! This module contains [`WalletCore`](crate::WalletCore) facades for interacting with various
//! on-chain programs.

use common::{error::ExecutionFailureKind, rpc_primitives::requests::SendTxResponse};
use nssa::{Account, program::Program};
use nssa_core::program::InstructionData;

use crate::{AccDecodeData, PrivacyPreservingAccount, WalletCore};

pub mod amm;
pub mod native_token_transfer;
pub mod pinata;
pub mod token;

pub trait ProgramArgs {
    fn prepare_private_transfer(
        &self,
    ) -> (
        InstructionData,
        Program,
        impl FnOnce(&[&Account]) -> Result<(), ExecutionFailureKind>,
    );
}

pub async fn send_privacy_preserving_transaction_unified<PD: ProgramArgs>(
    wallet_core: &WalletCore,
    acc_vector: Vec<PrivacyPreservingAccount>,
    method_data: PD,
) -> Result<(SendTxResponse, Vec<AccDecodeData>), ExecutionFailureKind> {
    let (instruction_data, program, tx_pre_check) = method_data.prepare_private_transfer();

    wallet_core
        .send_privacy_preserving_tx_with_pre_check(
            acc_vector.clone(),
            &instruction_data,
            &program.into(),
            tx_pre_check,
        )
        .await
        .map(|(resp, secrets)| {
            let mut secrets_iter = secrets.into_iter();

            (
                resp,
                acc_vector
                    .into_iter()
                    .filter_map(|acc| {
                        if acc.is_private() {
                            let secret = secrets_iter.next().expect("expected next secret");

                            if let Some(acc_id) = acc.account_id_decode_data() {
                                Some(AccDecodeData::Decode(secret, acc_id))
                            } else {
                                Some(AccDecodeData::Skip)
                            }
                        } else {
                            None
                        }
                    })
                    .collect(),
            )
        })
}
