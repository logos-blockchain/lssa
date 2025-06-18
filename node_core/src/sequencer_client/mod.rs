use accounts::account_core::Account;
use anyhow::Result;
use common::rpc_primitives::requests::{
    GetBlockDataRequest, GetBlockDataResponse, GetGenesisIdRequest, GetGenesisIdResponse,
    RegisterAccountRequest, RegisterAccountResponse,
};
use common::transaction::Transaction;
use common::{SequencerClientError, SequencerRpcError};
use json::{SendTxRequest, SendTxResponse, SequencerRpcRequest, SequencerRpcResponse};
use k256::elliptic_curve::group::GroupEncoding;
use reqwest::Client;
use serde_json::Value;

use crate::config::NodeConfig;

pub mod json;

#[derive(Clone)]
pub struct SequencerClient {
    pub client: reqwest::Client,
    pub config: NodeConfig,
}

impl SequencerClient {
    pub fn new(config: NodeConfig) -> Result<Self> {
        Ok(Self {
            client: Client::builder()
                //Add more fiedls if needed
                .timeout(std::time::Duration::from_secs(60))
                .build()?,
            config,
        })
    }

    pub async fn call_method_with_payload(
        &self,
        method: &str,
        payload: Value,
    ) -> Result<Value, SequencerClientError> {
        let request = SequencerRpcRequest::from_payload_version_2_0(method.to_string(), payload);

        let call_builder = self.client.post(&self.config.sequencer_addr);

        let call_res = call_builder.json(&request).send().await?;

        let response_vall = call_res.json::<Value>().await?;

        if let Ok(response) = serde_json::from_value::<SequencerRpcResponse>(response_vall.clone())
        {
            Ok(response.result)
        } else {
            let err_resp = serde_json::from_value::<SequencerRpcError>(response_vall)?;

            Err(err_resp.into())
        }
    }

    pub async fn get_block(
        &self,
        block_id: u64,
    ) -> Result<GetBlockDataResponse, SequencerClientError> {
        let block_req = GetBlockDataRequest { block_id };

        let req = serde_json::to_value(block_req)?;

        let resp = self.call_method_with_payload("get_block", req).await?;

        let resp_deser = serde_json::from_value(resp)?;

        Ok(resp_deser)
    }

    pub async fn send_tx(
        &self,
        transaction: Transaction,
        tx_roots: [[u8; 32]; 2],
    ) -> Result<SendTxResponse, SequencerClientError> {
        let tx_req = SendTxRequest {
            transaction,
            tx_roots,
        };

        let req = serde_json::to_value(tx_req)?;

        let resp = self.call_method_with_payload("send_tx", req).await?;

        let resp_deser = serde_json::from_value(resp)?;

        Ok(resp_deser)
    }

    pub async fn register_account(
        &self,
        account: &Account,
    ) -> Result<RegisterAccountResponse, SequencerClientError> {
        let acc_req = RegisterAccountRequest {
            nullifier_public_key: account.key_holder.nullifer_public_key.to_bytes().to_vec(),
            viewing_public_key: account.key_holder.viewing_public_key.to_bytes().to_vec(),
            address: account.address,
        };

        let req = serde_json::to_value(acc_req)?;

        let resp = self
            .call_method_with_payload("register_account", req)
            .await?;

        let resp_deser = serde_json::from_value(resp)?;

        Ok(resp_deser)
    }

    pub async fn get_genesis_id(&self) -> Result<GetGenesisIdResponse, SequencerClientError> {
        let genesis_req = GetGenesisIdRequest {};

        let req = serde_json::to_value(genesis_req).unwrap();

        let resp = self
            .call_method_with_payload("get_genesis", req)
            .await
            .unwrap();

        let resp_deser = serde_json::from_value(resp).unwrap();

        Ok(resp_deser)
    }
}
