use anyhow::Result;
use common::{merkle_tree_public::TreeHashType, AccountId};
use log::info;
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{digest::FixedOutput, Digest};

///Raw asset data
pub type Asset = Vec<u8>;
pub type Randomness = [u8; 32];

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
///Container for raw utxo payload
pub struct UTXO {
    pub hash: TreeHashType,
    pub owner: AccountId,
    pub asset: Asset,
    // TODO: change to u256
    pub amount: u128,
    pub privacy_flag: bool,
    pub randomness: Randomness,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UTXOPayload {
    pub owner: AccountId,
    pub asset: Asset,
    // TODO: change to u256
    pub amount: u128,
    pub privacy_flag: bool,
    pub randomness: Randomness,
}

impl UTXOPayload {
    fn to_bytes(&self) -> Vec<u8> {
        let mut result = Vec::new();
        result.extend_from_slice(&self.owner);
        result.extend_from_slice(&self.asset);
        result.extend_from_slice(&self.amount.to_be_bytes());
        result.push(self.privacy_flag as u8);
        result.extend_from_slice(&self.randomness);
        result
    }
}

impl UTXO {
    pub fn new(owner: AccountId, asset: Asset, amount: u128, privacy_flag: bool) -> Self {
        let mut randomness = Randomness::default();
        OsRng.fill_bytes(&mut randomness);
        let payload = UTXOPayload {
            owner,
            asset,
            amount,
            privacy_flag,
            randomness,
        };
        Self::create_utxo_from_payload(payload)
    }
    pub fn create_utxo_from_payload(payload_with_asset: UTXOPayload) -> Self {
        let mut hasher = sha2::Sha256::new();
        hasher.update(payload_with_asset.to_bytes());
        let hash = <TreeHashType>::from(hasher.finalize_fixed());

        Self {
            hash,
            owner: payload_with_asset.owner,
            asset: payload_with_asset.asset,
            amount: payload_with_asset.amount,
            privacy_flag: payload_with_asset.privacy_flag,
            randomness: payload_with_asset.randomness,
        }
    }

    pub fn interpret_asset<'de, ToInterpret: Deserialize<'de>>(&'de self) -> Result<ToInterpret> {
        Ok(serde_json::from_slice(&self.asset)?)
    }

    pub fn into_payload(&self) -> UTXOPayload {
        UTXOPayload {
            owner: self.owner,
            asset: self.asset.clone(),
            amount: self.amount,
            privacy_flag: self.privacy_flag,
            randomness: self.randomness,
        }
    }

    pub fn log(&self) {
        info!("UTXO hash is {:?}", hex::encode(self.hash));
        info!("UTXO owner is {:?}", hex::encode(self.owner));
        info!("UTXO asset is {:?}", hex::encode(self.asset.clone()));
        info!("UTXO amount is {:?}", self.amount);
        info!("UTXO privacy_flag is {:?}", self.privacy_flag);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct TestAsset {
        id: u32,
        name: String,
    }

    fn sample_account() -> AccountId {
        AccountId::default()
    }

    fn sample_payload() -> UTXOPayload {
        UTXOPayload {
            owner: sample_account(),
            asset: serde_json::to_vec(&TestAsset {
                id: 1,
                name: "Test".to_string(),
            })
            .unwrap(),
            amount: 10,
            privacy_flag: false,
            randomness: Randomness::default(),
        }
    }

    #[test]
    fn test_create_utxo_from_payload() {
        let payload = sample_payload();
        let utxo = UTXO::create_utxo_from_payload(payload.clone());

        // Ensure hash is created and the UTXO fields are correctly assigned
        assert_eq!(utxo.owner, payload.owner);
        assert_eq!(utxo.asset, payload.asset);
    }

    #[test]
    fn test_interpret_asset() {
        let payload = sample_payload();
        let utxo = UTXO::create_utxo_from_payload(payload);

        // Interpret asset as TestAsset
        let interpreted: TestAsset = utxo.interpret_asset().unwrap();

        assert_eq!(
            interpreted,
            TestAsset {
                id: 1,
                name: "Test".to_string()
            }
        );
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
