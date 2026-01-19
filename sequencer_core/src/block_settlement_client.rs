use std::{fs, path::Path};

use anyhow::Result;
use bedrock_client::BedrockClient;
use common::block::HashableBlockData;
use key_management_system_service::keys::{ED25519_SECRET_KEY_SIZE, Ed25519Key, Ed25519PublicKey};
use nomos_core::mantle::{
    MantleTx, Op, OpProof, SignedMantleTx, Transaction, TxHash, ledger,
    ops::channel::{ChannelId, MsgId, inscribe::InscriptionOp},
};
use reqwest::Url;

use crate::config::BedrockConfig;

/// A component that posts block data to logos blockchain
pub struct BlockSettlementClient {
    bedrock_node_url: Url,
    bedrock_client: BedrockClient,
    bedrock_signing_key: Ed25519Key,
    bedrock_channel_id: ChannelId,
    last_message_id: MsgId,
}

impl BlockSettlementClient {
    pub fn new(home: &Path, config: &BedrockConfig) -> Self {
        let bedrock_signing_key = load_or_create_signing_key(&home.join("bedrock_signing_key"))
            .expect("Signing key should load or be created successfully");
        let bedrock_node_url =
            Url::parse(&config.node_url).expect("Bedrock URL should be a valid URL");
        let bedrock_channel_id = config.channel_id;
        let bedrock_client =
            BedrockClient::new(None).expect("Bedrock client should be able to initialize");
        Self {
            bedrock_node_url,
            bedrock_client,
            bedrock_signing_key,
            bedrock_channel_id,
            last_message_id: MsgId::from([0; 32]),
        }
    }

    /// Create and sign a transaction for inscribing data
    pub fn create_inscribe_tx(&self, data: Vec<u8>) -> SignedMantleTx {
        let verifying_key_bytes = self.bedrock_signing_key.public_key().to_bytes();
        let verifying_key =
            Ed25519PublicKey::from_bytes(&verifying_key_bytes).expect("valid ed25519 public key");

        let inscribe_op = InscriptionOp {
            channel_id: self.bedrock_channel_id,
            inscription: data,
            parent: self.last_message_id,
            signer: verifying_key,
        };

        let ledger_tx = ledger::Tx::new(vec![], vec![]);

        let inscribe_tx = MantleTx {
            ops: vec![Op::ChannelInscribe(inscribe_op)],
            ledger_tx,
            storage_gas_price: 0,
            execution_gas_price: 0,
        };

        let tx_hash = inscribe_tx.hash();
        let signature_bytes = self
            .bedrock_signing_key
            .sign_payload(tx_hash.as_signing_bytes().as_ref())
            .to_bytes();
        let signature =
            key_management_system_service::keys::Ed25519Signature::from_bytes(&signature_bytes);

        SignedMantleTx {
            ops_proofs: vec![OpProof::Ed25519Sig(signature)],
            ledger_tx_proof: empty_ledger_signature(&tx_hash),
            mantle_tx: inscribe_tx,
        }
    }

    /// Post a transaction to the node and wait for inclusion
    pub async fn post_and_wait(&mut self, block_data: &HashableBlockData) -> Result<u64> {
        let inscription_data = borsh::to_vec(&block_data)?;
        let tx = self.create_inscribe_tx(inscription_data);

        // Post the transaction
        self.bedrock_client
            .0
            .post_transaction(self.bedrock_node_url.clone(), tx.clone())
            .await?;

        match tx.mantle_tx.ops.first() {
            Some(Op::ChannelInscribe(inscribe)) => self.last_message_id = inscribe.id(),
            _ => {}
        }

        Ok(block_data.block_id)
    }
}

/// Load signing key from file or generate a new one if it doesn't exist
fn load_or_create_signing_key(path: &Path) -> Result<Ed25519Key, ()> {
    if path.exists() {
        let key_bytes = fs::read(path).map_err(|_| ())?;
        if key_bytes.len() != ED25519_SECRET_KEY_SIZE {
            // TODO: proper error
            return Err(());
        }
        let key_array: [u8; ED25519_SECRET_KEY_SIZE] =
            key_bytes.try_into().expect("length already checked");
        Ok(Ed25519Key::from_bytes(&key_array))
    } else {
        let mut key_bytes = [0u8; ED25519_SECRET_KEY_SIZE];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut key_bytes);
        fs::write(path, key_bytes).map_err(|_| ())?;
        Ok(Ed25519Key::from_bytes(&key_bytes))
    }
}

fn empty_ledger_signature(tx_hash: &TxHash) -> key_management_system_service::keys::ZkSignature {
    key_management_system_service::keys::ZkKey::multi_sign(&[], tx_hash.as_ref())
        .expect("multi-sign with empty key set works")
}
