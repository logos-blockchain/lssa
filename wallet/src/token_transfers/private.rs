use common::{error::ExecutionFailureKind, sequencer_client::json::SendTxResponse};
use nssa::Address;
use nssa_core::{
    MembershipProof, NullifierPublicKey, SharedSecretKey, encryption::IncomingViewingPublicKey,
};

use crate::WalletCore;

impl WalletCore {
    pub async fn send_private_native_token_transfer_outer_account(
        &self,
        from: Address,
        to_npk: NullifierPublicKey,
        to_ipk: IncomingViewingPublicKey,
        balance_to_move: u128,
    ) -> Result<(SendTxResponse, [SharedSecretKey; 2]), ExecutionFailureKind> {
        let (instruction_data, program, tx_pre_check) =
            WalletCore::auth_transfer_preparation(balance_to_move);

        self.private_tx_two_accs_receiver_outer(
            from,
            to_npk,
            to_ipk,
            instruction_data,
            tx_pre_check,
            program,
        )
        .await
    }

    pub async fn send_private_native_token_transfer_owned_account_not_initialized(
        &self,
        from: Address,
        to: Address,
        balance_to_move: u128,
    ) -> Result<(SendTxResponse, [SharedSecretKey; 2]), ExecutionFailureKind> {
        let (instruction_data, program, tx_pre_check) =
            WalletCore::auth_transfer_preparation(balance_to_move);

        self.private_tx_two_accs_receiver_uninit(from, to, instruction_data, tx_pre_check, program)
            .await
    }

    pub async fn send_private_native_token_transfer_owned_account_already_initialized(
        &self,
        from: Address,
        to: Address,
        balance_to_move: u128,
        to_proof: MembershipProof,
    ) -> Result<(SendTxResponse, [SharedSecretKey; 2]), ExecutionFailureKind> {
        let (instruction_data, program, tx_pre_check) =
            WalletCore::auth_transfer_preparation(balance_to_move);

        self.private_tx_two_accs_all_init(
            from,
            to,
            instruction_data,
            tx_pre_check,
            program,
            to_proof,
        )
        .await
    }
}
