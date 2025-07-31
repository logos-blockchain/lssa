use common::transaction::SignaturePublicKey;
use tiny_keccak::{Hasher, Keccak};

// TODO: Consider wrapping `AccountAddress` in a struct.

pub type AccountAddress = [u8; 32];

/// Returns the address associated with a public key
pub fn from_public_key(public_key: &SignaturePublicKey) -> AccountAddress {
    let mut address = [0; 32];
    let mut keccak_hasher = Keccak::v256();
    keccak_hasher.update(&public_key.to_sec1_bytes());
    keccak_hasher.finalize(&mut address);
    address
}

#[cfg(test)]
mod tests {
    use common::transaction::SignaturePrivateKey;

    use super::*;
    use crate::account_core::address;

    #[test]
    fn test_address_key_equal_keccak_pub_sign_key() {
        let signing_key = SignaturePrivateKey::from_slice(&[1; 32]).unwrap();
        let public_key = signing_key.verifying_key();

        let mut expected_address = [0; 32];
        let mut keccak_hasher = Keccak::v256();
        keccak_hasher.update(&public_key.to_sec1_bytes());
        keccak_hasher.finalize(&mut expected_address);

        assert_eq!(expected_address, address::from_public_key(public_key));
    }
}
