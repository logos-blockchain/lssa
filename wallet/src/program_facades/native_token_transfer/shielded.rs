use common::{error::ExecutionFailureKind, rpc_primitives::requests::SendTxResponse};
use nssa::AccountId;
use nssa_core::{NullifierPublicKey, SharedSecretKey, encryption::IncomingViewingPublicKey};

use super::{NativeTokenTransfer, auth_transfer_preparation};
use crate::PrivacyPreservingAccount;

impl NativeTokenTransfer<'_> {
    pub async fn send_shielded_transfer(
        &self,
        from: AccountId,
        to: AccountId,
        balance_to_move: u128,
    ) -> Result<(SendTxResponse, SharedSecretKey), ExecutionFailureKind> {
        let (instruction_data, program, tx_pre_check) = auth_transfer_preparation(balance_to_move);

        self.0
            .send_privacy_preserving_tx_with_pre_check(
                vec![
                    PrivacyPreservingAccount::Public(from),
                    PrivacyPreservingAccount::PrivateOwned(to),
                ],
                &instruction_data,
                &program,
                tx_pre_check,
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

    pub async fn send_shielded_transfer_to_outer_account(
        &self,
        from: AccountId,
        to_npk: NullifierPublicKey,
        to_ipk: IncomingViewingPublicKey,
        balance_to_move: u128,
    ) -> Result<(SendTxResponse, SharedSecretKey), ExecutionFailureKind> {
        let (instruction_data, program, tx_pre_check) = auth_transfer_preparation(balance_to_move);

        self.0
            .send_privacy_preserving_tx_with_pre_check(
                vec![
                    PrivacyPreservingAccount::Public(from),
                    PrivacyPreservingAccount::PrivateForeign {
                        npk: to_npk,
                        ipk: to_ipk,
                    },
                ],
                &instruction_data,
                &program,
                tx_pre_check,
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
