use common::{error::ExecutionFailureKind, rpc_primitives::requests::SendTxResponse};
use nssa::{Account, program::Program};
use nssa_core::program::InstructionData;

use crate::{AccDecodeData, PrivacyPreservingAccount, WalletCore};

pub mod deshielded;
pub mod private;
pub mod public;
pub mod shielded;

pub struct NativeTokenTransfer<'w>(pub &'w WalletCore);

fn auth_transfer_preparation(
    balance_to_move: u128,
) -> (
    InstructionData,
    Program,
    impl FnOnce(&[&Account]) -> Result<(), ExecutionFailureKind>,
) {
    let instruction_data = Program::serialize_instruction(balance_to_move).unwrap();
    let program = Program::authenticated_transfer_program();
    let tx_pre_check = move |accounts: &[&Account]| {
        let from = accounts[0];
        if from.balance >= balance_to_move {
            Ok(())
        } else {
            Err(ExecutionFailureKind::InsufficientFundsError)
        }
    };

    (instruction_data, program, tx_pre_check)
}

impl NativeTokenTransfer<'_> {
    pub async fn send_privacy_preserving_transfer_unified(
        &self,
        acc_vector: Vec<PrivacyPreservingAccount>,
        method_data: u128,
    ) -> Result<(SendTxResponse, Vec<AccDecodeData>), ExecutionFailureKind> {
        let (instruction_data, program, tx_pre_check) = auth_transfer_preparation(method_data);

        self.0
            .send_privacy_preserving_tx_with_pre_check(
                acc_vector.clone(),
                &instruction_data,
                &program,
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
}
