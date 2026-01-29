//! This library contains common code for integration tests.

use std::{net::SocketAddr, path::PathBuf, sync::LazyLock};

use actix_web::dev::ServerHandle;
use anyhow::{Context as _, Result};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use common::{
    sequencer_client::SequencerClient,
    transaction::{EncodedTransaction, NSSATransaction},
};
use futures::FutureExt as _;
use log::debug;
use nssa::PrivacyPreservingTransaction;
use nssa_core::Commitment;
use sequencer_core::config::SequencerConfig;
use tempfile::TempDir;
use tokio::task::JoinHandle;
use wallet::{WalletCore, config::WalletConfigOverrides};

// TODO: Remove this and control time from tests
pub const TIME_TO_WAIT_FOR_BLOCK_SECONDS: u64 = 12;

pub const ACC_SENDER: &str = "BLgCRDXYdQPMMWVHYRFGQZbgeHx9frkipa8GtpG2Syqy";
pub const ACC_RECEIVER: &str = "Gj1mJy5W7J5pfmLRujmQaLfLMWidNxQ6uwnhb666ZwHw";

pub const ACC_SENDER_PRIVATE: &str = "3oCG8gqdKLMegw4rRfyaMQvuPHpcASt7xwttsmnZLSkw";
pub const ACC_RECEIVER_PRIVATE: &str = "AKTcXgJ1xoynta1Ec7y6Jso1z1JQtHqd7aPQ1h9er6xX";

pub const NSSA_PROGRAM_FOR_TEST_DATA_CHANGER: &str = "data_changer.bin";

static LOGGER: LazyLock<()> = LazyLock::new(env_logger::init);

/// Test context which sets up a sequencer and a wallet for integration tests.
///
/// It's memory and logically safe to create multiple instances of this struct in parallel tests,
/// as each instance uses its own temporary directories for sequencer and wallet data.
pub struct TestContext {
    sequencer_server_handle: ServerHandle,
    sequencer_loop_handle: JoinHandle<Result<()>>,
    sequencer_client: SequencerClient,
    wallet: WalletCore,
    _temp_sequencer_dir: TempDir,
    _temp_wallet_dir: TempDir,
}

impl TestContext {
    /// Create new test context.
    pub async fn new() -> Result<Self> {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");

        let sequencer_config_path =
            PathBuf::from(manifest_dir).join("configs/sequencer/sequencer_config.json");

        let sequencer_config = SequencerConfig::from_path(&sequencer_config_path)
            .context("Failed to create sequencer config from file")?;

        Self::new_with_sequencer_config(sequencer_config).await
    }

    /// Create new test context with custom sequencer config.
    ///
    /// `home` and `port` fields of the provided config will be overridden to meet tests parallelism
    /// requirements.
    pub async fn new_with_sequencer_config(sequencer_config: SequencerConfig) -> Result<Self> {
        // Ensure logger is initialized only once
        *LOGGER;

        debug!("Test context setup");

        let (sequencer_server_handle, sequencer_addr, sequencer_loop_handle, temp_sequencer_dir) =
            Self::setup_sequencer(sequencer_config)
                .await
                .context("Failed to setup sequencer")?;

        // Convert 0.0.0.0 to 127.0.0.1 for client connections
        // When binding to port 0, the server binds to 0.0.0.0:<random_port>
        // but clients need to connect to 127.0.0.1:<port> to work reliably
        let sequencer_addr = if sequencer_addr.ip().is_unspecified() {
            format!("http://127.0.0.1:{}", sequencer_addr.port())
        } else {
            format!("http://{sequencer_addr}")
        };

        let (wallet, temp_wallet_dir) = Self::setup_wallet(sequencer_addr.clone())
            .await
            .context("Failed to setup wallet")?;

        let sequencer_client =
            SequencerClient::new(sequencer_addr).context("Failed to create sequencer client")?;

        Ok(Self {
            sequencer_server_handle,
            sequencer_loop_handle,
            sequencer_client,
            wallet,
            _temp_sequencer_dir: temp_sequencer_dir,
            _temp_wallet_dir: temp_wallet_dir,
        })
    }

    async fn setup_sequencer(
        mut config: SequencerConfig,
    ) -> Result<(ServerHandle, SocketAddr, JoinHandle<Result<()>>, TempDir)> {
        let temp_sequencer_dir =
            tempfile::tempdir().context("Failed to create temp dir for sequencer home")?;

        debug!(
            "Using temp sequencer home at {:?}",
            temp_sequencer_dir.path()
        );
        config.home = temp_sequencer_dir.path().to_owned();
        // Setting port to 0 lets the OS choose a free port for us
        config.port = 0;

        let (sequencer_server_handle, sequencer_addr, sequencer_loop_handle, _) =
            sequencer_runner::startup_sequencer(config).await?;

        Ok((
            sequencer_server_handle,
            sequencer_addr,
            sequencer_loop_handle,
            temp_sequencer_dir,
        ))
    }

    async fn setup_wallet(sequencer_addr: String) -> Result<(WalletCore, TempDir)> {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let wallet_config_source_path =
            PathBuf::from(manifest_dir).join("configs/wallet/wallet_config.json");

        let temp_wallet_dir =
            tempfile::tempdir().context("Failed to create temp dir for wallet home")?;

        let config_path = temp_wallet_dir.path().join("wallet_config.json");
        std::fs::copy(&wallet_config_source_path, &config_path)
            .context("Failed to copy wallet config to temp dir")?;

        let storage_path = temp_wallet_dir.path().join("storage.json");
        let config_overrides = WalletConfigOverrides {
            sequencer_addr: Some(sequencer_addr),
            ..Default::default()
        };

        let wallet = WalletCore::new_init_storage(
            config_path,
            storage_path,
            Some(config_overrides),
            "test_pass".to_owned(),
        )
        .context("Failed to init wallet")?;
        wallet
            .store_persistent_data()
            .await
            .context("Failed to store wallet persistent data")?;

        Ok((wallet, temp_wallet_dir))
    }

    /// Get reference to the wallet.
    pub fn wallet(&self) -> &WalletCore {
        &self.wallet
    }

    /// Get mutable reference to the wallet.
    pub fn wallet_mut(&mut self) -> &mut WalletCore {
        &mut self.wallet
    }

    /// Get reference to the sequencer client.
    pub fn sequencer_client(&self) -> &SequencerClient {
        &self.sequencer_client
    }
}

impl Drop for TestContext {
    fn drop(&mut self) {
        debug!("Test context cleanup");

        let Self {
            sequencer_server_handle,
            sequencer_loop_handle,
            sequencer_client: _,
            wallet: _,
            _temp_sequencer_dir,
            _temp_wallet_dir,
        } = self;

        sequencer_loop_handle.abort();

        // Can't wait here as Drop can't be async, but anyway stop signal should be sent
        sequencer_server_handle.stop(true).now_or_never();
    }
}

pub fn format_public_account_id(account_id: &str) -> String {
    format!("Public/{account_id}")
}

pub fn format_private_account_id(account_id: &str) -> String {
    format!("Private/{account_id}")
}

pub async fn fetch_privacy_preserving_tx(
    seq_client: &SequencerClient,
    tx_hash: String,
) -> PrivacyPreservingTransaction {
    let transaction_encoded = seq_client
        .get_transaction_by_hash(tx_hash.clone())
        .await
        .unwrap()
        .transaction
        .unwrap();

    let tx_base64_decode = BASE64.decode(transaction_encoded).unwrap();
    match NSSATransaction::try_from(
        &borsh::from_slice::<EncodedTransaction>(&tx_base64_decode).unwrap(),
    )
    .unwrap()
    {
        NSSATransaction::PrivacyPreserving(privacy_preserving_transaction) => {
            privacy_preserving_transaction
        }
        _ => panic!("Invalid tx type"),
    }
}

pub async fn verify_commitment_is_in_state(
    commitment: Commitment,
    seq_client: &SequencerClient,
) -> bool {
    matches!(
        seq_client.get_proof_for_commitment(commitment).await,
        Ok(Some(_))
    )
}

#[cfg(test)]
mod tests {
    use super::{format_private_account_id, format_public_account_id};

    #[test]
    fn correct_account_id_from_prefix() {
        let account_id1 = "cafecafe";
        let account_id2 = "deadbeaf";

        let account_id1_pub = format_public_account_id(account_id1);
        let account_id2_priv = format_private_account_id(account_id2);

        assert_eq!(account_id1_pub, "Public/cafecafe".to_string());
        assert_eq!(account_id2_priv, "Private/deadbeaf".to_string());
    }
}
