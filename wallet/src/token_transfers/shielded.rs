use common::{error::ExecutionFailureKind, sequencer_client::json::SendTxResponse};
use nssa::AccountId;
use nssa_core::{NullifierPublicKey, SharedSecretKey, encryption::IncomingViewingPublicKey};

use crate::{PrivacyPreservingAccount, WalletCore};

impl WalletCore {
    pub async fn send_shielded_native_token_transfer(
        &self,
        from: AccountId,
        to: AccountId,
        balance_to_move: u128,
    ) -> Result<(SendTxResponse, SharedSecretKey), ExecutionFailureKind> {
        let (instruction_data, program, tx_pre_check) =
            WalletCore::auth_transfer_preparation(balance_to_move);

        self.send_privacy_preserving_tx(
            vec![
                PrivacyPreservingAccount::Public(from),
                PrivacyPreservingAccount::PrivateLocal(to),
            ],
            instruction_data,
            tx_pre_check,
            program,
        )
        .await
        .map(|(resp, secrets)| {
            let first = secrets
                .into_iter()
                .next()
                .expect("expected sender's secret");
            (resp, first)
        })
    }

    pub async fn send_shielded_native_token_transfer_outer_account(
        &self,
        from: AccountId,
        to_npk: NullifierPublicKey,
        to_ipk: IncomingViewingPublicKey,
        balance_to_move: u128,
    ) -> Result<(SendTxResponse, SharedSecretKey), ExecutionFailureKind> {
        let (instruction_data, program, tx_pre_check) =
            WalletCore::auth_transfer_preparation(balance_to_move);

        self.send_privacy_preserving_tx(
            vec![
                PrivacyPreservingAccount::Public(from),
                PrivacyPreservingAccount::PrivateForeign {
                    npk: to_npk,
                    ipk: to_ipk,
                },
            ],
            instruction_data,
            tx_pre_check,
            program,
        )
        .await
        .map(|(resp, secrets)| {
            let first = secrets
                .into_iter()
                .next()
                .expect("expected sender's secret");
            (resp, first)
        })
    }
}
