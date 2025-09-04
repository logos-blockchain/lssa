use std::{path::PathBuf, sync::Arc};

use actix_web::dev::ServerHandle;
use anyhow::Result;
use clap::Parser;
use common::rpc_primitives::RpcConfig;
use log::info;
use sequencer_core::{SequencerCore, config::SequencerConfig};
use sequencer_rpc::new_http_server;
use tokio::{sync::Mutex, task::JoinHandle};

pub mod config;

pub const RUST_LOG: &str = "RUST_LOG";

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    /// Path to configs
    home_dir: PathBuf,
}

pub async fn startup_sequencer(
    app_config: SequencerConfig,
) -> Result<(ServerHandle, JoinHandle<Result<()>>)> {
    let block_timeout = app_config.block_create_timeout_millis;
    let port = app_config.port;

    let sequencer_core = SequencerCore::start_from_config(app_config);

    info!("Sequencer core set up");

    let seq_core_wrapped = Arc::new(Mutex::new(sequencer_core));

    let http_server = new_http_server(RpcConfig::with_port(port), seq_core_wrapped.clone())?;
    info!("HTTP server started");
    let http_server_handle = http_server.handle();
    tokio::spawn(http_server);

    info!("Starting main sequencer loop");

    let main_loop_handle = tokio::spawn(async move {
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
    });

    Ok((http_server_handle, main_loop_handle))
}

pub async fn main_runner() -> Result<()> {
    env_logger::init();

    let args = Args::parse();
    let Args { home_dir } = args;

    let app_config = config::from_file(home_dir.join("sequencer_config.json"))?;

    if let Some(ref rust_log) = app_config.override_rust_log {
        info!("RUST_LOG env var set to {rust_log:?}");

        unsafe {
            std::env::set_var(RUST_LOG, rust_log);
        }
    }

    //ToDo: Add restart on failures
    let (_, _) = startup_sequencer(app_config).await?;

    Ok(())
}
