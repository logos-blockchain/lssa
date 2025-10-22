use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use std::path::PathBuf;

use actix_web::dev::ServerHandle;
use anyhow::Result;
use clap::Parser;
use common::{
    sequencer_client::SequencerClient,
    transaction::{EncodedTransaction, NSSATransaction},
};
use log::{info, warn};
use nssa::PrivacyPreservingTransaction;
use nssa_core::Commitment;
use sequencer_core::config::SequencerConfig;
use sequencer_runner::startup_sequencer;
use tempfile::TempDir;
use tokio::task::JoinHandle;

use crate::test_suite_map::prepare_function_map;

#[macro_use]
extern crate proc_macro_test_attribute;

pub mod test_suite_map;

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    /// Path to configs
    home_dir: PathBuf,
    /// Test name
    test_name: String,
}

pub const ACC_SENDER: &str = "d07ad2e84b27fa00c262f0a1eea0ff35ca0973547e6a106f72f193c2dc838b44";
pub const ACC_RECEIVER: &str = "e7ae77c5ef1a05999344af499fc78a1705398d62ed06cf2e1479f6def89a39bc";

pub const ACC_SENDER_PRIVATE: &str =
    "d360d6b5763f71ac6af56253687fd7d556d5c6c64312e53c0b92ef039a4375df";
pub const ACC_RECEIVER_PRIVATE: &str =
    "f27087ffc29b99035303697dcf6c8e323b1847d4261e6afd49e0d71c6dfa31ea";

pub const TIME_TO_WAIT_FOR_BLOCK_SECONDS: u64 = 12;

pub const NSSA_PROGRAM_FOR_TEST_DATA_CHANGER: &[u8] = include_bytes!("data_changer.bin");

#[allow(clippy::type_complexity)]
pub async fn pre_test(
    home_dir: PathBuf,
) -> Result<(ServerHandle, JoinHandle<Result<()>>, TempDir)> {
    let home_dir_sequencer = home_dir.join("sequencer");

    let mut sequencer_config =
        sequencer_runner::config::from_file(home_dir_sequencer.join("sequencer_config.json"))
            .unwrap();

    let temp_dir_sequencer = replace_home_dir_with_temp_dir_in_configs(&mut sequencer_config);

    let (seq_http_server_handle, sequencer_loop_handle) =
        startup_sequencer(sequencer_config).await?;

    Ok((
        seq_http_server_handle,
        sequencer_loop_handle,
        temp_dir_sequencer,
    ))
}

pub fn replace_home_dir_with_temp_dir_in_configs(
    sequencer_config: &mut SequencerConfig,
) -> TempDir {
    let temp_dir_sequencer = tempfile::tempdir().unwrap();

    sequencer_config.home = temp_dir_sequencer.path().to_path_buf();

    temp_dir_sequencer
}

#[allow(clippy::type_complexity)]
pub async fn post_test(residual: (ServerHandle, JoinHandle<Result<()>>, TempDir)) {
    let (seq_http_server_handle, sequencer_loop_handle, _) = residual;

    info!("Cleanup");

    sequencer_loop_handle.abort();
    seq_http_server_handle.stop(true).await;

    let wallet_home = wallet::helperfunctions::get_home().unwrap();
    let persistent_data_home = wallet_home.join("curr_accounts.json");

    //Removing persistent accounts after run to not affect other executions
    //Not necessary an error, if fails as there is tests for failure scenario
    let _ = std::fs::remove_file(persistent_data_home)
        .inspect_err(|err| warn!("Failed to remove persistent data with err {err:#?}"));

    //At this point all of the references to sequencer_core must be lost.
    //So they are dropped and tempdirs will be dropped too,
}

pub async fn main_tests_runner() -> Result<()> {
    env_logger::init();

    let args = Args::parse();
    let Args {
        home_dir,
        test_name,
    } = args;

    let function_map = prepare_function_map();

    match test_name.as_str() {
        "all" => {
            for (_, fn_pointer) in function_map {
                fn_pointer(home_dir.clone()).await;
            }
        }
        _ => {
            let fn_pointer = function_map.get(&test_name).expect("Unknown test name");

            fn_pointer(home_dir.clone()).await;
        }
    }

    Ok(())
}

async fn fetch_privacy_preserving_tx(
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

async fn verify_commitment_is_in_state(
    commitment: Commitment,
    seq_client: &SequencerClient,
) -> bool {
    matches!(
        seq_client.get_proof_for_commitment(commitment).await,
        Ok(Some(_))
    )
}
