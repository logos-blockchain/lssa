use std::net::SocketAddr;

use anyhow::Result;
use indexer_service::{BackoffConfig, BedrockClientConfig, ChannelId, IndexerConfig};
use url::Url;

pub fn indexer_config(bedrock_addr: SocketAddr) -> IndexerConfig {
    let channel_id: [u8; 32] = [0u8, 1]
        .repeat(16)
        .try_into()
        .unwrap_or_else(|_| unreachable!());
    let channel_id = ChannelId::try_from(channel_id).expect("Failed to create channel ID");

    IndexerConfig {
        resubscribe_interval_millis: 1000,
        backoff: BackoffConfig {
            start_delay_millis: 100,
            max_retries: 10,
        },
        bedrock_client_config: BedrockClientConfig {
            addr: addr_to_http_url(bedrock_addr).expect("Failed to convert bedrock addr to URL"),
            auth: None,
        },
        channel_id,
    }
}

fn addr_to_http_url(addr: SocketAddr) -> Result<Url> {
    // Convert 0.0.0.0 to 127.0.0.1 for client connections
    // When binding to port 0, the server binds to 0.0.0.0:<random_port>
    // but clients need to connect to 127.0.0.1:<port> to work reliably
    let url_string = if addr.ip().is_unspecified() {
        format!("http://127.0.0.1:{}", addr.port())
    } else {
        format!("http://{addr}")
    };

    url_string.parse().map_err(Into::into)
}
