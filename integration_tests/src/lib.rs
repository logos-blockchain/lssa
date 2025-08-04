use std::{path::PathBuf, sync::Arc, time::Duration};

use actix_web::dev::ServerHandle;
use anyhow::Result;
use clap::Parser;
use common::rpc_primitives::RpcConfig;
use log::info;
use node_core::{NodeCore, config::NodeConfig};
use sequencer_core::config::SequencerConfig;
use sequencer_runner::startup_sequencer;
use tempfile::TempDir;
use tokio::{sync::Mutex, task::JoinHandle};

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    /// Path to configs
    home_dir: PathBuf,
    /// Test name
    test_name: String,
}

pub const ACC_SENDER: &str = "0d96dfcc414019380c9dde0cd3dce5aac90fb5443bf871108741aeafde552ad7";
pub const ACC_RECEIVER: &str = "974870e9be8d0ac08aa83b3fc7a7a686291d8732508aba98b36080f39c2cf364";

pub const TIME_TO_WAIT_FOR_BLOCK_SECONDS: u64 = 12;

#[allow(clippy::type_complexity)]
pub async fn pre_test(
    home_dir: PathBuf,
) -> Result<(
    ServerHandle,
    JoinHandle<Result<()>>,
    ServerHandle,
    TempDir,
    TempDir,
    Arc<Mutex<NodeCore>>,
)> {
    let home_dir_sequencer = home_dir.join("sequencer");
    let home_dir_node = home_dir.join("node");

    let mut sequencer_config =
        sequencer_runner::config::from_file(home_dir_sequencer.join("sequencer_config.json"))
            .unwrap();
    let mut node_config =
        node_runner::config::from_file(home_dir_node.join("node_config.json")).unwrap();

    let (temp_dir_node, temp_dir_sequencer) =
        replace_home_dir_with_temp_dir_in_configs(&mut node_config, &mut sequencer_config);

    let (seq_http_server_handle, sequencer_loop_handle) =
        startup_sequencer(sequencer_config).await?;

    let node_port = node_config.port;

    let node_core = NodeCore::start_from_config_update_chain(node_config.clone()).await?;

    let wrapped_node_core = Arc::new(Mutex::new(node_core));

    let http_server = node_rpc::new_http_server(
        RpcConfig::with_port(node_port),
        node_config.clone(),
        wrapped_node_core.clone(),
    )?;
    info!("HTTP server started");
    let node_http_server_handle = http_server.handle();
    tokio::spawn(http_server);

    Ok((
        seq_http_server_handle,
        sequencer_loop_handle,
        node_http_server_handle,
        temp_dir_node,
        temp_dir_sequencer,
        wrapped_node_core,
    ))
}

pub fn replace_home_dir_with_temp_dir_in_configs(
    node_config: &mut NodeConfig,
    sequencer_config: &mut SequencerConfig,
) -> (TempDir, TempDir) {
    let temp_dir_node = tempfile::tempdir().unwrap();
    let temp_dir_sequencer = tempfile::tempdir().unwrap();

    node_config.home = temp_dir_node.path().to_path_buf();
    sequencer_config.home = temp_dir_sequencer.path().to_path_buf();

    (temp_dir_node, temp_dir_sequencer)
}

#[allow(clippy::type_complexity)]
pub async fn post_test(
    residual: (
        ServerHandle,
        JoinHandle<Result<()>>,
        ServerHandle,
        TempDir,
        TempDir,
        Arc<Mutex<NodeCore>>,
    ),
) {
    let (seq_http_server_handle, sequencer_loop_handle, node_http_server_handle, _, _, _) =
        residual;

    info!("Cleanup");

    node_http_server_handle.stop(true).await;
    sequencer_loop_handle.abort();
    seq_http_server_handle.stop(true).await;

    //At this point all of the references to node_core and sequencer_core must be lost.
    //So they are dropped and tempdirs will be dropped too,
}

pub async fn test_success(wrapped_node_core: Arc<Mutex<NodeCore>>) {
    let acc_sender = hex::decode(ACC_SENDER).unwrap().try_into().unwrap();
    let acc_receiver = hex::decode(ACC_RECEIVER).unwrap().try_into().unwrap();

    let guard = wrapped_node_core.lock().await;

    let _res = guard
        .send_public_native_token_transfer(acc_sender, 0, acc_receiver, 100)
        .await
        .unwrap();

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    info!("Checking correct balance move");
    let acc_1_balance = guard
        .sequencer_client
        .get_account_balance(ACC_SENDER.to_string())
        .await
        .unwrap();
    let acc_2_balance = guard
        .sequencer_client
        .get_account_balance(ACC_RECEIVER.to_string())
        .await
        .unwrap();

    info!("Balance of sender : {acc_1_balance:#?}");
    info!("Balance of receiver : {acc_2_balance:#?}");

    assert_eq!(acc_1_balance.balance, 9900);
    assert_eq!(acc_2_balance.balance, 20100);

    info!("Success!");
}

pub async fn test_success_move_to_another_account(wrapped_node_core: Arc<Mutex<NodeCore>>) {
    let acc_sender = hex::decode(ACC_SENDER).unwrap().try_into().unwrap();
    let acc_receiver_new_acc = [42; 32];

    let hex_acc_receiver_new_acc = hex::encode(acc_receiver_new_acc);

    let guard = wrapped_node_core.lock().await;

    let _res = guard
        .send_public_native_token_transfer(acc_sender, 0, acc_receiver_new_acc, 100)
        .await
        .unwrap();

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    info!("Checking correct balance move");
    let acc_1_balance = guard
        .sequencer_client
        .get_account_balance(ACC_SENDER.to_string())
        .await
        .unwrap();
    let acc_2_balance = guard
        .sequencer_client
        .get_account_balance(hex_acc_receiver_new_acc)
        .await
        .unwrap();

    info!("Balance of sender : {acc_1_balance:#?}");
    info!("Balance of receiver : {acc_2_balance:#?}");

    assert_eq!(acc_1_balance.balance, 9900);
    assert_eq!(acc_2_balance.balance, 100);

    info!("Success!");
}

pub async fn test_failure(wrapped_node_core: Arc<Mutex<NodeCore>>) {
    let acc_sender = hex::decode(ACC_SENDER).unwrap().try_into().unwrap();
    let acc_receiver = hex::decode(ACC_RECEIVER).unwrap().try_into().unwrap();

    let guard = wrapped_node_core.lock().await;

    let _res = guard
        .send_public_native_token_transfer(acc_sender, 0, acc_receiver, 100000)
        .await
        .unwrap();

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    info!("Checking correct balance move");
    let acc_1_balance = guard
        .sequencer_client
        .get_account_balance(ACC_SENDER.to_string())
        .await
        .unwrap();
    let acc_2_balance = guard
        .sequencer_client
        .get_account_balance(ACC_RECEIVER.to_string())
        .await
        .unwrap();

    info!("Balance of sender : {acc_1_balance:#?}");
    info!("Balance of receiver : {acc_2_balance:#?}");

    assert_eq!(acc_1_balance.balance, 10000);
    assert_eq!(acc_2_balance.balance, 20000);

    info!("Success!");
}

macro_rules! test_cleanup_wrap {
    ($home_dir:ident, $test_func:ident) => {{
        let res = pre_test($home_dir.clone()).await.unwrap();

        let wrapped_node_core = res.5.clone();

        info!("Waiting for first block creation");
        tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

        $test_func(wrapped_node_core.clone()).await;

        post_test(res).await;
    }};
}

pub async fn main_tests_runner() -> Result<()> {
    env_logger::init();

    let args = Args::parse();
    let Args {
        home_dir,
        test_name,
    } = args;

    match test_name.as_str() {
        "test_success_move_to_another_account" => {
            test_cleanup_wrap!(home_dir, test_success_move_to_another_account);
        }
        "test_success" => {
            test_cleanup_wrap!(home_dir, test_success);
        }
        "test_failure" => {
            test_cleanup_wrap!(home_dir, test_failure);
        }
        "all" => {
            test_cleanup_wrap!(home_dir, test_success_move_to_another_account);
            test_cleanup_wrap!(home_dir, test_success);
            test_cleanup_wrap!(home_dir, test_failure);
        }
        _ => {
            anyhow::bail!("Unknown test name");
        }
    }

    Ok(())
}
