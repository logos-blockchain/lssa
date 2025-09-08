use std::sync::Arc;

use anyhow::Result;
use common::sequencer_client::SequencerClient;
use log::{info, warn};

use crate::config::WalletConfig;

#[derive(Clone)]
///Helperstruct to poll transactions
pub struct TxPoller {
    pub polling_max_blocks_to_query: usize,
    pub polling_max_error_attempts: u64,
    pub polling_error_delay_millis: u64,
    pub polling_delay_millis: u64,
    pub client: Arc<SequencerClient>,
}

impl TxPoller {
    pub fn new(config: WalletConfig, client: Arc<SequencerClient>) -> Self {
        Self {
            polling_delay_millis: config.seq_poll_timeout_millis,
            polling_max_blocks_to_query: config.seq_poll_max_blocks,
            polling_max_error_attempts: config.seq_poll_max_retries,
            polling_error_delay_millis: config.seq_poll_retry_delay_millis,
            client: client.clone(),
        }
    }

    pub async fn poll_tx(&self, tx_hash: String) -> Result<String> {
        let max_blocks_to_query = self.polling_max_blocks_to_query;

        info!("Starting poll for transaction {tx_hash:#?}");
        for poll_id in 1..max_blocks_to_query {
            info!("Poll {poll_id}");

            let mut try_error_counter = 0;

            let tx_obj = loop {
                let tx_obj = self
                    .client
                    .get_transaction_by_hash(tx_hash.clone())
                    .await
                    .inspect_err(|err| {
                        warn!("Failed to get transaction by hash {tx_hash:#?} with error: {err:#?}")
                    });

                if let Ok(tx_obj) = tx_obj {
                    break tx_obj;
                } else {
                    try_error_counter += 1;
                }

                if try_error_counter > self.polling_max_error_attempts {
                    anyhow::bail!("Number of retries exceeded");
                }
            };

            if tx_obj.transaction.is_some() {
                return Ok(tx_obj.transaction.unwrap());
            }

            tokio::time::sleep(std::time::Duration::from_millis(self.polling_delay_millis)).await;
        }

        anyhow::bail!("Transaction not found in preconfigured amount of blocks");
    }
}
