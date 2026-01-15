use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use actix_web::dev::ServerHandle;
use anyhow::Result;
use bedrock_client::BasicAuthCredentials;
use clap::Parser;
use common::rpc_primitives::RpcConfig;
use indexer::IndexerCore;
use log::info;
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
) -> Result<(ServerHandle, SocketAddr, JoinHandle<Result<()>>, JoinHandle<Result<()>>)> {
    let block_timeout = app_config.block_create_timeout_millis;
    let port = app_config.port;

    // ToDo: Maybe make buffer size configurable.
    let (sender, receiver) = tokio::sync::mpsc::channel(100);

    let indexer_core = IndexerCore::new(
        &app_config.bedrock_addr,
        Some(BasicAuthCredentials::new(
            app_config.bedrock_auth.0.clone(),
            Some(app_config.bedrock_auth.1.clone()),
        )),
        sender,
        app_config.indexer_config.clone(),
    )?;

    info!("Indexer core set up");

    let (sequencer_core, mempool_handle) = SequencerCore::start_from_config(app_config, receiver);

    info!("Sequencer core set up");

    let indexer_core_wrapped = Arc::new(Mutex::new(indexer_core));
    let seq_core_wrapped = Arc::new(Mutex::new(sequencer_core));

    let (http_server, addr) = new_http_server(
        RpcConfig::with_port(port),
        Arc::clone(&seq_core_wrapped),
        mempool_handle,
        Arc::clone(&indexer_core_wrapped),
    )?;
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

                state
                    .produce_new_block_and_post_to_settlement_layer()
                    .await?
            };

            info!("Block with id {id} created");

            info!("Waiting for new transactions");
        }
    });

    let indexer_loop_handle = tokio::spawn(async move {
        {
            let indexer_guard = indexer_core_wrapped.lock().await;
            let res = indexer_guard.subscribe_parse_block_stream().await;

            info!("Indexer loop res is {res:#?}");
        }

        Ok(())
    });

    Ok((http_server_handle, addr, main_loop_handle, indexer_loop_handle))
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
    let (_, _, main_loop_handle, indexer_loop_handle) = startup_sequencer(app_config).await?;

    main_loop_handle.await??;
    indexer_loop_handle.await??;

    Ok(())
}
