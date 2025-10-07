use common::{ExecutionFailureKind, sequencer_client::json::SendTxResponse};
use nssa::{
    Address, PublicTransaction,
    program::Program,
    public_transaction::{Message, WitnessSet},
};

use crate::WalletCore;

impl WalletCore {
    pub async fn send_public_native_token_transfer(
        &self,
        from: Address,
        to: Address,
        balance_to_move: u128,
    ) -> Result<SendTxResponse, ExecutionFailureKind> {
        let Ok(balance) = self.get_account_balance(from).await else {
            return Err(ExecutionFailureKind::SequencerError);
        };

        if balance >= balance_to_move {
            let Ok(nonces) = self.get_accounts_nonces(vec![from]).await else {
                return Err(ExecutionFailureKind::SequencerError);
            };

            let addresses = vec![from, to];
            let program_id = Program::authenticated_transfer_program().id();
            let message = Message::try_new(program_id, addresses, nonces, balance_to_move).unwrap();

            let signing_key = self.storage.user_data.get_pub_account_signing_key(&from);

            let Some(signing_key) = signing_key else {
                return Err(ExecutionFailureKind::KeyNotFoundError);
            };

            let witness_set = WitnessSet::for_message(&message, &[signing_key]);

            let tx = PublicTransaction::new(message, witness_set);

            Ok(self.sequencer_client.send_tx_public(tx).await?)
        } else {
            Err(ExecutionFailureKind::InsufficientFundsError)
        }
    }
}
