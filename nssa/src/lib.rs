#[cfg(not(feature = "no_docker"))]
pub mod program_methods {
    include!(concat!(env!("OUT_DIR"), "/program_methods/mod.rs"));
}

#[cfg(feature = "no_docker")]
#[allow(clippy::single_component_path_imports)]
use program_methods;

pub mod encoding;
pub mod error;
mod merkle_tree;
pub mod privacy_preserving_transaction;
pub mod program;
pub mod program_deployment_transaction;
pub mod public_transaction;
mod signature;
mod state;

pub use nssa_core::account::{Account, AccountId};
pub use nssa_core::address::Address;
pub use privacy_preserving_transaction::{
    PrivacyPreservingTransaction, circuit::execute_and_prove,
};
pub use program_deployment_transaction::ProgramDeploymentTransaction;
pub use program_methods::PRIVACY_PRESERVING_CIRCUIT_ID;
pub use public_transaction::PublicTransaction;
pub use signature::PrivateKey;
pub use signature::PublicKey;
pub use signature::Signature;
pub use state::V02State;
