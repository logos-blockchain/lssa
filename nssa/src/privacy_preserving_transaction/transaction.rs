use std::collections::HashMap;

use nssa_core::account::Account;

use crate::error::NssaError;
use crate::{Address, V01State};

use super::message::Message;
use super::witness_set::WitnessSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivacyPreservingTransaction {
    message: Message,
    witness_set: WitnessSet,
}

impl PrivacyPreservingTransaction {
    pub(crate) fn validate(
        &self,
        arg: &mut V01State,
    ) -> Result<HashMap<Address, Account>, NssaError> {
        todo!()
    }

    pub fn message(&self) -> &Message {
        &self.message
    }

    pub fn witness_set(&self) -> &WitnessSet {
        &self.witness_set
    }

    pub(crate) fn signer_addresses(&self) -> Vec<Address> {
        self.witness_set
            .signatures_and_public_keys()
            .iter()
            .map(|(_, public_key)| Address::from_public_key(public_key))
            .collect()
    }
}
