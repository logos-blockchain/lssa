use common::{error::ExecutionFailureKind, sequencer_client::json::SendTxResponse};
use nssa::Address;

use crate::WalletCore;

impl WalletCore {
    pub async fn send_deshielded_native_token_transfer(
        &self,
        from: Address,
        to: Address,
        balance_to_move: u128,
    ) -> Result<(SendTxResponse, [nssa_core::SharedSecretKey; 1]), ExecutionFailureKind> {
        let (instruction_data, program, tx_pre_check) =
            WalletCore::auth_transfer_preparation(balance_to_move);

        self.deshielded_tx_two_accs(from, to, instruction_data, tx_pre_check, program)
            .await
    }
}
