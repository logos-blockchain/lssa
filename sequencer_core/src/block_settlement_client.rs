use std::{fs, path::Path};

use anyhow::{Context, Result, anyhow};
use bedrock_client::BedrockClient;
use common::block::{BedrockStatus, Block, HashableBlockData};
use logos_blockchain_core::mantle::{
    MantleTx, Op, OpProof, SignedMantleTx, Transaction, TxHash, ledger,
    ops::channel::{ChannelId, MsgId, inscribe::InscriptionOp},
};
use logos_blockchain_key_management_system_service::keys::{
    ED25519_SECRET_KEY_SIZE, Ed25519Key, Ed25519PublicKey,
};

use crate::config::BedrockConfig;

/// A component that posts block data to logos blockchain
#[derive(Clone)]
pub struct BlockSettlementClient {
    bedrock_client: BedrockClient,
    bedrock_signing_key: Ed25519Key,
    bedrock_channel_id: ChannelId,
}

impl BlockSettlementClient {
    pub fn try_new(home: &Path, config: &BedrockConfig) -> Result<Self> {
        let bedrock_signing_key = load_or_create_signing_key(&home.join("bedrock_signing_key"))
            .context("Failed to load or create signing key")?;
        let bedrock_channel_id = ChannelId::from(config.channel_id);
        let bedrock_client = BedrockClient::new(None, config.node_url.clone())
            .context("Failed to initialize bedrock client")?;
        Ok(Self {
            bedrock_client,
            bedrock_signing_key,
            bedrock_channel_id,
        })
    }

    /// Create and sign a transaction for inscribing data
    pub fn create_inscribe_tx(&self, block: &Block) -> Result<(SignedMantleTx, MsgId)> {
        let inscription_data = borsh::to_vec(block)?;
        let verifying_key_bytes = self.bedrock_signing_key.public_key().to_bytes();
        let verifying_key =
            Ed25519PublicKey::from_bytes(&verifying_key_bytes).expect("valid ed25519 public key");

        let inscribe_op = InscriptionOp {
            channel_id: self.bedrock_channel_id,
            inscription: inscription_data,
            parent: block.bedrock_parent_id,
            signer: verifying_key,
        };
        let inscribe_op_id = inscribe_op.id();

        let ledger_tx = ledger::Tx::new(vec![], vec![]);

        let inscribe_tx = MantleTx {
            ops: vec![Op::ChannelInscribe(inscribe_op)],
            ledger_tx,
            // Altruistic test config
            storage_gas_price: 0,
            execution_gas_price: 0,
        };

        let tx_hash = inscribe_tx.hash();
        let signature_bytes = self
            .bedrock_signing_key
            .sign_payload(tx_hash.as_signing_bytes().as_ref())
            .to_bytes();
        let signature =
            logos_blockchain_key_management_system_service::keys::Ed25519Signature::from_bytes(
                &signature_bytes,
            );

        let signed_mantle_tx = SignedMantleTx {
            ops_proofs: vec![OpProof::Ed25519Sig(signature)],
            ledger_tx_proof: empty_ledger_signature(&tx_hash),
            mantle_tx: inscribe_tx,
        };
        Ok((signed_mantle_tx, inscribe_op_id))
    }

    /// Post a transaction to the node
    pub async fn submit_block_to_bedrock(&self, block: &Block) -> Result<MsgId> {
        let (tx, new_msg_id) = self.create_inscribe_tx(block)?;

        // Post the transaction
        self.bedrock_client.post_transaction(tx).await?;

        Ok(new_msg_id)
    }
}

/// Load signing key from file or generate a new one if it doesn't exist
fn load_or_create_signing_key(path: &Path) -> Result<Ed25519Key> {
    if path.exists() {
        let key_bytes = fs::read(path)?;
        let key_array: [u8; ED25519_SECRET_KEY_SIZE] = key_bytes
            .try_into()
            .map_err(|_| anyhow!("Found key with incorrect length"))?;
        Ok(Ed25519Key::from_bytes(&key_array))
    } else {
        let mut key_bytes = [0u8; ED25519_SECRET_KEY_SIZE];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut key_bytes);
        fs::write(path, key_bytes)?;
        Ok(Ed25519Key::from_bytes(&key_bytes))
    }
}

fn empty_ledger_signature(
    tx_hash: &TxHash,
) -> logos_blockchain_key_management_system_service::keys::ZkSignature {
    logos_blockchain_key_management_system_service::keys::ZkKey::multi_sign(&[], tx_hash.as_ref())
        .expect("multi-sign with empty key set works")
}
