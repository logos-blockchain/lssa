use common::error::ExecutionFailureKind;
use key_protocol::key_management::ephemeral_key_holder::EphemeralKeyHolder;
use nssa::{AccountId, PrivateKey};
use nssa_core::{
    MembershipProof, NullifierPublicKey, NullifierSecretKey, SharedSecretKey,
    account::{AccountWithMetadata, Nonce},
    encryption::{EphemeralPublicKey, IncomingViewingPublicKey},
};

use crate::{WalletCore, transaction_utils::AccountPreparedData};

pub enum PrivacyPreservingAccount {
    Public(AccountId),
    PrivateLocal(AccountId),
    PrivateForeign {
        npk: NullifierPublicKey,
        ipk: IncomingViewingPublicKey,
    },
}

pub struct PrivateAccountKeys {
    pub npk: NullifierPublicKey,
    pub ssk: SharedSecretKey,
    pub ipk: IncomingViewingPublicKey,
    pub epk: EphemeralPublicKey,
}

enum State {
    Public {
        account: AccountWithMetadata,
        sk: Option<PrivateKey>,
    },
    Private(AccountPreparedData),
}

pub struct Payload {
    states: Vec<State>,
    visibility_mask: Vec<u8>,
}

impl Payload {
    pub async fn new(
        wallet: &WalletCore,
        accounts: Vec<PrivacyPreservingAccount>,
    ) -> Result<Self, ExecutionFailureKind> {
        let mut pre_states = Vec::with_capacity(accounts.len());
        let mut visibility_mask = Vec::with_capacity(accounts.len());

        for account in accounts {
            let (state, mask) = match account {
                PrivacyPreservingAccount::Public(account_id) => {
                    let acc = wallet
                        .get_account_public(account_id)
                        .await
                        .map_err(|_| ExecutionFailureKind::KeyNotFoundError)?;

                    let sk = wallet.get_account_public_signing_key(&account_id).cloned();
                    let account = AccountWithMetadata::new(acc.clone(), sk.is_some(), account_id);

                    (State::Public { account, sk }, 0)
                }
                PrivacyPreservingAccount::PrivateLocal(account_id) => {
                    let mut pre = wallet
                        .private_acc_preparation(account_id, true, true)
                        .await?;
                    let mut mask = 1;

                    if pre.proof.is_none() {
                        pre.auth_acc.is_authorized = false;
                        pre.nsk = None;
                        mask = 2
                    };

                    (State::Private(pre), mask)
                }
                PrivacyPreservingAccount::PrivateForeign { npk, ipk } => {
                    let acc = nssa_core::account::Account::default();
                    let auth_acc = AccountWithMetadata::new(acc, false, &npk);
                    let pre = AccountPreparedData {
                        nsk: None,
                        npk,
                        ipk,
                        auth_acc,
                        proof: None,
                    };

                    (State::Private(pre), 2)
                }
            };

            pre_states.push(state);
            visibility_mask.push(mask);
        }

        Ok(Self {
            states: pre_states,
            visibility_mask,
        })
    }

    pub fn pre_states(&self) -> Vec<AccountWithMetadata> {
        self.states
            .iter()
            .map(|state| match state {
                State::Public { account, .. } => account.clone(),
                State::Private(pre) => pre.auth_acc.clone(),
            })
            .collect()
    }

    pub fn visibility_mask(&self) -> &[u8] {
        &self.visibility_mask
    }

    pub fn public_account_nonces(&self) -> Vec<Nonce> {
        self.states
            .iter()
            .filter_map(|state| match state {
                State::Public { account, .. } => Some(account.account.nonce),
                _ => None,
            })
            .collect()
    }

    pub fn private_account_keys(&self) -> Vec<PrivateAccountKeys> {
        self.states
            .iter()
            .filter_map(|state| match state {
                State::Private(pre) => {
                    let eph_holder = EphemeralKeyHolder::new(&pre.npk);

                    Some(PrivateAccountKeys {
                        npk: pre.npk.clone(),
                        ssk: eph_holder.calculate_shared_secret_sender(&pre.ipk),
                        ipk: pre.ipk.clone(),
                        epk: eph_holder.generate_ephemeral_public_key(),
                    })
                }
                _ => None,
            })
            .collect()
    }

    pub fn private_account_auth(&self) -> Vec<(NullifierSecretKey, MembershipProof)> {
        self.states
            .iter()
            .filter_map(|state| match state {
                State::Private(pre) => Some((pre.nsk?, pre.proof.clone()?)),
                _ => None,
            })
            .collect()
    }

    pub fn public_account_ids(&self) -> Vec<AccountId> {
        self.states
            .iter()
            .filter_map(|state| match state {
                State::Public { account, .. } => Some(account.account_id),
                _ => None,
            })
            .collect()
    }

    pub fn witness_signing_keys(&self) -> Vec<&PrivateKey> {
        self.states
            .iter()
            .filter_map(|state| match state {
                State::Public { sk, .. } => sk.as_ref(),
                _ => None,
            })
            .collect()
    }
}
