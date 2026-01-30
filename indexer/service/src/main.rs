use std::{net::SocketAddr, path::PathBuf};

use anyhow::{Context as _, Result};
use clap::Parser;
use indexer_core::config::IndexerConfig;
use indexer_service_rpc::RpcServer as _;
use jsonrpsee::server::Server;
use log::{error, info};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Parser)]
#[clap(version)]
struct Args {
    #[clap(name = "config")]
    config_path: PathBuf,
    #[clap(short, long, default_value = "8779")]
    port: u16,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let Args { config_path, port } = Args::parse();

    let cancellation_token = listen_for_shutdown_signal();

    let handle = run_server(config_path, port).await?;
    let handle_clone = handle.clone();

    tokio::select! {
        _ = cancellation_token.cancelled() => {
            info!("Shutting down server...");
        }
        _ = handle_clone.stopped() => {
            error!("Server stopped unexpectedly");
        }
    }

    info!("Server shutdown complete");

    Ok(())
}

async fn run_server(config_path: PathBuf, port: u16) -> Result<jsonrpsee::server::ServerHandle> {
    let config = IndexerConfig::from_path(&config_path)?;
    #[cfg(feature = "mock-responses")]
    let _ = config;

    let server = Server::builder()
        .build(SocketAddr::from(([0, 0, 0, 0], port)))
        .await
        .context("Failed to build RPC server")?;

    let addr = server
        .local_addr()
        .context("Failed to get local address of RPC server")?;

    info!("Starting Indexer Service RPC server on {addr}");

    #[cfg(not(feature = "mock-responses"))]
    let handle = {
        let service = indexer_service::service::IndexerService::new(config)
            .context("Failed to initialize indexer service")?;
        server.start(service.into_rpc())
    };
    #[cfg(feature = "mock-responses")]
    let handle = server.start(
        indexer_service::mock_service::MockIndexerService::new_with_mock_blocks().into_rpc(),
    );

    Ok(handle)
}

fn listen_for_shutdown_signal() -> CancellationToken {
    let cancellation_token = CancellationToken::new();
    let cancellation_token_clone = cancellation_token.clone();

    tokio::spawn(async move {
        if let Err(err) = tokio::signal::ctrl_c().await {
            error!("Failed to listen for Ctrl-C signal: {err}");
            return;
        }
        info!("Received Ctrl-C signal");
        cancellation_token_clone.cancel();
    });

    cancellation_token
}
