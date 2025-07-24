use std::io;
use std::sync::Arc;

use actix_cors::Cors;
use actix_web::{http, middleware, web, App, Error as HttpError, HttpResponse, HttpServer};
use futures::Future;
use futures::FutureExt;
use log::info;

use common::rpc_primitives::message::Message;
use common::rpc_primitives::RpcConfig;
use node_core::config::NodeConfig;
use node_core::NodeCore;
use tokio::sync::Mutex;

use super::JsonHandler;

pub const SHUTDOWN_TIMEOUT_SECS: u64 = 10;

fn rpc_handler(
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

#[allow(clippy::too_many_arguments)]
pub fn new_http_server(
    config: RpcConfig,
    node_config: NodeConfig,
    node_chain_store: Arc<Mutex<NodeCore>>,
) -> io::Result<actix_web::dev::Server> {
    let RpcConfig {
        addr,
        cors_allowed_origins,
        polling_config,
        limits_config,
    } = config;
    info!(target:"network", "Starting http server at {addr}");
    let handler = web::Data::new(JsonHandler {
        polling_config,
        node_core_config: node_config,
        node_chain_store,
    });

    // HTTP server
    Ok(HttpServer::new(move || {
        App::new()
            .wrap(get_cors(&cors_allowed_origins))
            .app_data(handler.clone())
            .app_data(web::JsonConfig::default().limit(limits_config.json_payload_max_size))
            .wrap(middleware::Logger::default())
            .service(web::resource("/").route(web::post().to(rpc_handler)))
    })
    .bind(addr)?
    .shutdown_timeout(SHUTDOWN_TIMEOUT_SECS)
    .disable_signals()
    .run())
}
