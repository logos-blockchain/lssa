use std::{path::PathBuf, sync::Arc};

use anyhow::Result;
use clap::Parser;
use common::rpc_primitives::RpcConfig;
use consensus::ConsensusManager;
use log::info;
use networking::peer_manager::PeerManager;
use node_core::NodeCore;
use node_rpc::new_http_server;
use tokio::sync::Mutex;

pub mod config;

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    /// Path to configs
    home_dir: PathBuf,
}

pub async fn main_runner() -> Result<()> {
    env_logger::init();

    let args = Args::parse();
    let Args { home_dir } = args;

    let app_config = config::from_file(home_dir.join("node_config.json"))?;

    let port = app_config.port;

    let node_core = NodeCore::start_from_config_update_chain(app_config.clone()).await?;
    let wrapped_node_core = Arc::new(Mutex::new(node_core));

    let http_server = new_http_server(
        RpcConfig::with_port(port),
        app_config.clone(),
        wrapped_node_core.clone(),
    )?;
    info!("HTTP server started");
    let _http_server_handle = http_server.handle();
    tokio::spawn(http_server);

    let peer_manager = PeerManager::start_peer_manager(4, 0).await?;
    info!("Peer manager mock started");

    let peer_manager_shared = Arc::new(Mutex::new(peer_manager));

    let _consensus_manager = ConsensusManager::new(peer_manager_shared.clone());
    info!("Consensus manger mock started");

    #[allow(clippy::empty_loop)]
    loop {
        //ToDo: Insert activity into main loop
    }
}
