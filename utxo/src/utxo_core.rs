use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{digest::FixedOutput, Digest};
use storage::{merkle_tree_public::TreeHashType, nullifier::UTXONullifier, AccountId};

///Raw asset data
pub type Asset = Vec<u8>;

#[derive(Debug, PartialEq, Eq, Clone)]
///Container for raw utxo payload
pub struct UTXO {
    pub hash: TreeHashType,
    pub owner: AccountId,
    pub nullifier: Option<UTXONullifier>,
    pub asset: Asset,
}

#[derive(Debug, Clone, Serialize)]
pub struct UTXOPayload {
    pub owner: AccountId,
    pub asset: Asset,
}

impl UTXO {
    pub fn create_utxo_from_payload(payload_with_asset: UTXOPayload) -> Self {
        let raw_payload = serde_json::to_vec(&payload_with_asset).unwrap();

        let mut hasher = sha2::Sha256::new();

        hasher.update(&raw_payload);

        let hash = <TreeHashType>::from(hasher.finalize_fixed());

        Self {
            hash,
            owner: payload_with_asset.owner,
            nullifier: None,
            asset: payload_with_asset.asset,
        }
    }

    pub fn consume_utxo(&mut self, nullifier: UTXONullifier) -> Result<()> {
        if self.nullifier.is_some() {
            anyhow::bail!("UTXO already consumed");
        } else {
            self.nullifier = Some(nullifier);
        }

        Ok(())
    }

    pub fn interpret_asset<'de, ToInterpret: Deserialize<'de>>(&'de self) -> Result<ToInterpret> {
        Ok(serde_json::from_slice(&self.asset)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use storage::{merkle_tree_public::TreeHashType, nullifier::UTXONullifier, AccountId};

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct TestAsset {
        id: u32,
        name: String,
    }

    fn sample_account() -> AccountId {
        AccountId::default()
    }

    fn sample_nullifier() -> UTXONullifier {
        UTXONullifier::default()
    }

    fn sample_tree_hash() -> TreeHashType {
        TreeHashType::default()
    }

    fn sample_payload() -> UTXOPayload {
        UTXOPayload {
            owner: sample_account(),
            asset: serde_json::to_vec(&TestAsset { id: 1, name: "Test".to_string() }).unwrap(),
        }
    }

    #[test]
    fn test_create_utxo_from_payload() {
        let payload = sample_payload();
        let utxo = UTXO::create_utxo_from_payload(payload.clone());

        // Ensure hash is created and the UTXO fields are correctly assigned
        assert_eq!(utxo.owner, payload.owner);
        assert_eq!(utxo.asset, payload.asset);
        assert!(utxo.nullifier.is_none());
    }

    #[test]
    fn test_consume_utxo() {
        let payload = sample_payload();
        let mut utxo = UTXO::create_utxo_from_payload(payload);

        let nullifier = sample_nullifier();

        // First consumption should succeed
        assert!(utxo.consume_utxo(nullifier.clone()).is_ok());
        assert_eq!(utxo.nullifier, Some(nullifier));

        // Second consumption should fail
        let result = utxo.consume_utxo(sample_nullifier());
        assert!(result.is_err());
    }

    #[test]
    fn test_interpret_asset() {
        let payload = sample_payload();
        let utxo = UTXO::create_utxo_from_payload(payload);

        // Interpret asset as TestAsset
        let interpreted: TestAsset = utxo.interpret_asset().unwrap();

        assert_eq!(interpreted, TestAsset { id: 1, name: "Test".to_string() });
    }

    #[test]
    fn test_interpret_invalid_asset() {
        let mut payload = sample_payload();
        payload.asset = vec![0, 1, 2, 3]; // Invalid data for deserialization
        let utxo = UTXO::create_utxo_from_payload(payload);

        // This should fail because the asset is not valid JSON for TestAsset
        let result: Result<TestAsset> = utxo.interpret_asset();
        assert!(result.is_err());
    }
}
