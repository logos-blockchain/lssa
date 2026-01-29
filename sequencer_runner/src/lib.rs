use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use actix_web::dev::ServerHandle;
use anyhow::Result;
use clap::Parser;
use common::rpc_primitives::RpcConfig;
use log::{info, warn};
use sequencer_core::{SequencerCore, config::SequencerConfig};
use sequencer_rpc::new_http_server;
use tokio::{sync::Mutex, task::JoinHandle};

pub const RUST_LOG: &str = "RUST_LOG";

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    /// Path to configs
    home_dir: PathBuf,
}

pub async fn startup_sequencer(
    app_config: SequencerConfig,
) -> Result<(
    ServerHandle,
    SocketAddr,
    JoinHandle<Result<()>>,
    JoinHandle<Result<()>>,
)> {
    let block_timeout = app_config.block_create_timeout_millis;
    let retry_pending_blocks_timeout = app_config.retry_pending_blocks_timeout_millis;
    let port = app_config.port;

    let (sequencer_core, mempool_handle) = SequencerCore::start_from_config(app_config);

    info!("Sequencer core set up");

    let seq_core_wrapped = Arc::new(Mutex::new(sequencer_core));

    let (http_server, addr) = new_http_server(
        RpcConfig::with_port(port),
        Arc::clone(&seq_core_wrapped),
        mempool_handle,
    )?;
    info!("HTTP server started");
    let http_server_handle = http_server.handle();
    tokio::spawn(http_server);

    info!("Starting pending block retry loop");
    let seq_core_wrapped_for_block_retry = seq_core_wrapped.clone();
    let retry_pending_blocks_handle = tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(
                retry_pending_blocks_timeout,
            ))
            .await;

            let (pending_blocks, block_settlement_client) = {
                let sequencer_core = seq_core_wrapped_for_block_retry.lock().await;
                let client = sequencer_core.block_settlement_client();
                let pending_blocks = sequencer_core
                    .get_pending_blocks()
                    .expect("Sequencer should be able to retrieve pending blocks");
                (pending_blocks, client)
            };

            let Some(client) = block_settlement_client else {
                continue;
            };

            info!("Resubmitting {} pending blocks", pending_blocks.len());
            for block in pending_blocks.iter() {
                if let Err(e) = client.submit_block_to_bedrock(block).await {
                    warn!(
                        "Failed to resubmit block with id {} with error {}",
                        block.header.block_id, e
                    );
                }
            }
        }
    });

    info!("Starting main sequencer loop");
    let main_loop_handle = tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(block_timeout)).await;

            info!("Collecting transactions from mempool, block creation");

            let id = {
                let mut state = seq_core_wrapped.lock().await;

                state
                    .produce_new_block_and_post_to_settlement_layer()
                    .await?
            };

            info!("Block with id {id} created");

            info!("Waiting for new transactions");
        }
    });

    Ok((
        http_server_handle,
        addr,
        main_loop_handle,
        retry_pending_blocks_handle,
    ))
}

pub async fn main_runner() -> Result<()> {
    env_logger::init();

    let args = Args::parse();
    let Args { home_dir } = args;

    let app_config = SequencerConfig::from_path(&home_dir.join("sequencer_config.json"))?;

    if let Some(ref rust_log) = app_config.override_rust_log {
        info!("RUST_LOG env var set to {rust_log:?}");

        unsafe {
            std::env::set_var(RUST_LOG, rust_log);
        }
    }

    // ToDo: Add restart on failures
    let (_, _, main_loop_handle, retry_loop_handle) = startup_sequencer(app_config).await?;

    info!("Sequencer running. Monitoring concurrent tasks...");

    tokio::select! {
        res = main_loop_handle => {
            match res {
                Ok(inner_res) => warn!("Main loop exited unexpectedly: {:?}", inner_res),
                Err(e) => warn!("Main loop task panicked: {:?}", e),
            }
        }
        res = retry_loop_handle => {
            match res {
                Ok(inner_res) => warn!("Retry loop exited unexpectedly: {:?}", inner_res),
                Err(e) => warn!("Retry loop task panicked: {:?}", e),
            }
        }
    }

    info!("Shutting down sequencer...");

    Ok(())
}
