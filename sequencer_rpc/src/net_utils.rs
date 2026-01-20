use std::{io, net::SocketAddr, sync::Arc};

use actix_cors::Cors;
use actix_web::{App, Error as HttpError, HttpResponse, HttpServer, http, middleware, web};
use common::{
    rpc_primitives::{RpcConfig, message::Message},
    transaction::EncodedTransaction,
};
use futures::{Future, FutureExt};
use indexer::state::IndexerState;
use log::info;
use mempool::MemPoolHandle;
use sequencer_core::SequencerCore;
use tokio::sync::Mutex;

use super::JsonHandler;

pub const SHUTDOWN_TIMEOUT_SECS: u64 = 10;

pub const NETWORK: &str = "network";

pub(crate) fn rpc_handler(
    message: web::Json<Message>,
    handler: web::Data<JsonHandler>,
) -> impl Future<Output = Result<HttpResponse, HttpError>> {
    let response = async move {
        let message = handler.process(message.0).await?;
        Ok(HttpResponse::Ok().json(&message))
    };
    response.boxed()
}

fn get_cors(cors_allowed_origins: &[String]) -> Cors {
    let mut cors = Cors::permissive();
    if cors_allowed_origins != ["*".to_string()] {
        for origin in cors_allowed_origins {
            cors = cors.allowed_origin(origin);
        }
    }
    cors.allowed_methods(vec!["GET", "POST"])
        .allowed_headers(vec![http::header::AUTHORIZATION, http::header::ACCEPT])
        .allowed_header(http::header::CONTENT_TYPE)
        .max_age(3600)
}

pub fn new_http_server(
    config: RpcConfig,
    seuquencer_core: Arc<Mutex<SequencerCore>>,
    mempool_handle: MemPoolHandle<EncodedTransaction>,
    indexer_core: Option<IndexerState>,
) -> io::Result<(actix_web::dev::Server, SocketAddr)> {
    let RpcConfig {
        addr,
        cors_allowed_origins,
        limits_config,
    } = config;
    info!(target:NETWORK, "Starting HTTP server at {addr}");
    let handler = web::Data::new(JsonHandler {
        sequencer_state: seuquencer_core.clone(),
        indexer_state: indexer_core.clone(),
        mempool_handle,
    });

    // HTTP server
    let http_server = HttpServer::new(move || {
        App::new()
            .wrap(get_cors(&cors_allowed_origins))
            .app_data(handler.clone())
            .app_data(web::JsonConfig::default().limit(limits_config.json_payload_max_size))
            .wrap(middleware::Logger::default())
            .service(web::resource("/").route(web::post().to(rpc_handler)))
    })
    .bind(addr)?
    .shutdown_timeout(SHUTDOWN_TIMEOUT_SECS)
    .disable_signals();

    let [addr] = http_server
        .addrs()
        .try_into()
        .expect("Exactly one address bound is expected for sequencer HTTP server");

    info!(target:NETWORK, "HTTP server started at {addr}");

    Ok((http_server.run(), addr))
}
