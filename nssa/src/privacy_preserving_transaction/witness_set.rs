use crate::{privacy_preserving_transaction::message::Message, PrivateKey, PublicKey, Signature};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WitnessSet {
    pub(super) signatures_and_public_keys: Vec<(Signature, PublicKey)>,
}


impl WitnessSet {
    pub fn for_message(message: &Message, private_keys: &[&PrivateKey]) -> Self {
        todo!()
    }

    pub fn is_valid_for(&self, message: &Message) -> bool {
        todo!()
    }

    pub fn signatures_and_public_keys(&self) -> &[(Signature, PublicKey)] {
        &self.signatures_and_public_keys
    }
}
