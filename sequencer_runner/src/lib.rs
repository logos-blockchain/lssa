use std::{path::PathBuf, sync::Arc};

use anyhow::Result;
use clap::Parser;
use common::rpc_primitives::RpcConfig;
use log::info;
use sequencer_core::SequencerCore;
use sequencer_rpc::new_http_server;
use tokio::sync::Mutex;

pub mod config;

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    /// Path to configs
    home_dir: PathBuf,
}

pub async fn main_runner() -> Result<()> {
    let args = Args::parse();
    let Args { home_dir } = args;

    let app_config = config::from_file(home_dir.join("sequencer_config.json"))?;

    let block_timeout = app_config.block_create_timeout_millis;
    let port = app_config.port;

    if let Some(ref rust_log) = app_config.override_rust_log {
        info!("RUST_LOG env var set to {rust_log:?}");

        std::env::set_var("RUST_LOG", rust_log);
    }

    env_logger::init();

    let sequencer_core = SequencerCore::start_from_config(app_config);

    info!("Sequncer core set up");

    let seq_core_wrapped = Arc::new(Mutex::new(sequencer_core));

    let http_server = new_http_server(RpcConfig::with_port(port), seq_core_wrapped.clone())?;
    info!("HTTP server started");
    let _http_server_handle = http_server.handle();
    tokio::spawn(http_server);

    info!("Starting main sequencer loop");

    #[allow(clippy::empty_loop)]
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(block_timeout)).await;

        info!("Collecting transactions from mempool, block creation");

        let id = {
            let mut state = seq_core_wrapped.lock().await;

            state.produce_new_block_with_mempool_transactions()?
        };

        info!("Block with id {id} created");

        info!("Waiting for new transactions");
    }
}
