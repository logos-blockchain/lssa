use std::fmt::Display;

use borsh::{BorshDeserialize, BorshSerialize};
use log::warn;
use nssa::{AccountId, V02State};
use serde::{Deserialize, Serialize};

use crate::HashType;

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum NSSATransaction {
    Public(nssa::PublicTransaction),
    PrivacyPreserving(nssa::PrivacyPreservingTransaction),
    ProgramDeployment(nssa::ProgramDeploymentTransaction),
}

impl NSSATransaction {
    pub fn hash(&self) -> HashType {
        HashType(match self {
            NSSATransaction::Public(tx) => tx.hash(),
            NSSATransaction::PrivacyPreserving(tx) => tx.hash(),
            NSSATransaction::ProgramDeployment(tx) => tx.hash(),
        })
    }
}

impl From<nssa::PublicTransaction> for NSSATransaction {
    fn from(value: nssa::PublicTransaction) -> Self {
        Self::Public(value)
    }
}

impl From<nssa::PrivacyPreservingTransaction> for NSSATransaction {
    fn from(value: nssa::PrivacyPreservingTransaction) -> Self {
        Self::PrivacyPreserving(value)
    }
}

impl NSSATransaction {
    pub fn affected_public_account_ids(&self) -> Vec<AccountId> {
        match self {
            NSSATransaction::ProgramDeployment(tx) => tx.affected_public_account_ids(),
            NSSATransaction::Public(tx) => tx.affected_public_account_ids(),
            NSSATransaction::PrivacyPreserving(tx) => tx.affected_public_account_ids(),
        }
    }
}

impl From<nssa::ProgramDeploymentTransaction> for NSSATransaction {
    fn from(value: nssa::ProgramDeploymentTransaction) -> Self {
        Self::ProgramDeployment(value)
    }
}

#[derive(
    Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize,
)]
pub enum TxKind {
    Public,
    PrivacyPreserving,
    ProgramDeployment,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TransactionMalformationError {
    InvalidSignature,
    FailedToDecode { tx: HashType },
}

impl Display for TransactionMalformationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:#?}")
    }
}

impl std::error::Error for TransactionMalformationError {}

// TODO: Introduce type-safe wrapper around checked transaction, e.g. AuthenticatedTransaction
pub fn transaction_pre_check(
    tx: NSSATransaction,
) -> Result<NSSATransaction, TransactionMalformationError> {
    // Stateless checks here
    match tx {
        NSSATransaction::Public(tx) => {
            if tx.witness_set().is_valid_for(tx.message()) {
                Ok(NSSATransaction::Public(tx))
            } else {
                Err(TransactionMalformationError::InvalidSignature)
            }
        }
        NSSATransaction::PrivacyPreserving(tx) => {
            if tx.witness_set().signatures_are_valid_for(tx.message()) {
                Ok(NSSATransaction::PrivacyPreserving(tx))
            } else {
                Err(TransactionMalformationError::InvalidSignature)
            }
        }
        NSSATransaction::ProgramDeployment(tx) => Ok(NSSATransaction::ProgramDeployment(tx)),
    }
}

pub fn execute_check_transaction_on_state(
    state: &mut V02State,
    tx: NSSATransaction,
) -> Result<NSSATransaction, nssa::error::NssaError> {
    match &tx {
        NSSATransaction::Public(tx) => state.transition_from_public_transaction(tx),
        NSSATransaction::PrivacyPreserving(tx) => {
            state.transition_from_privacy_preserving_transaction(tx)
        }
        NSSATransaction::ProgramDeployment(tx) => {
            state.transition_from_program_deployment_transaction(tx)
        }
    }
    .inspect_err(|err| warn!("Error at transition {err:#?}"))?;

    Ok(tx)
}
