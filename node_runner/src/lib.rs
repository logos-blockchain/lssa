use anyhow::Result;
use log::info;
use node_rpc::new_http_server;
use rpc_primitives::RpcConfig;

pub async fn main_runner() -> Result<()> {
    env_logger::init();

    let http_server = new_http_server(RpcConfig::default())?;
    info!("HTTP server started");
    let _http_server_handle = http_server.handle();
    tokio::spawn(http_server);

    #[allow(clippy::empty_loop)]
    loop {
        //ToDo: Insert activity into main loop
    }
}
