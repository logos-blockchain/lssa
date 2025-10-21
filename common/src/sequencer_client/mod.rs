use std::collections::HashMap;

use super::rpc_primitives::requests::{
    GetAccountBalanceRequest, GetAccountBalanceResponse, GetBlockDataRequest, GetBlockDataResponse,
    GetGenesisIdRequest, GetGenesisIdResponse, GetInitialTestnetAccountsRequest,
};
use anyhow::Result;
use json::{SendTxRequest, SendTxResponse, SequencerRpcRequest, SequencerRpcResponse};
use nssa_core::program::ProgramId;
use reqwest::Client;
use serde_json::Value;

use crate::error::{SequencerClientError, SequencerRpcError};
use crate::rpc_primitives::requests::{
    GetAccountRequest, GetAccountResponse, GetAccountsNoncesRequest, GetAccountsNoncesResponse,
    GetLastBlockRequest, GetLastBlockResponse, GetProgramIdsRequest, GetProgramIdsResponse,
    GetProofForCommitmentRequest, GetProofForCommitmentResponse, GetTransactionByHashRequest,
    GetTransactionByHashResponse,
};
use crate::sequencer_client::json::AccountInitialData;
use crate::transaction::{EncodedTransaction, NSSATransaction};

pub mod json;

#[derive(Clone)]
pub struct SequencerClient {
    pub client: reqwest::Client,
    pub sequencer_addr: String,
}

impl SequencerClient {
    pub fn new(sequencer_addr: String) -> Result<Self> {
        Ok(Self {
            client: Client::builder()
                //Add more fiedls if needed
                .timeout(std::time::Duration::from_secs(60))
                .build()?,
            sequencer_addr,
        })
    }

    pub async fn call_method_with_payload(
        &self,
        method: &str,
        payload: Value,
    ) -> Result<Value, SequencerClientError> {
        let request = SequencerRpcRequest::from_payload_version_2_0(method.to_string(), payload);

        let call_builder = self.client.post(&self.sequencer_addr);

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

    ///Get block data at `block_id` from sequencer
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

    ///Get last known `blokc_id` from sequencer
    pub async fn get_last_block(&self) -> Result<GetLastBlockResponse, SequencerClientError> {
        let block_req = GetLastBlockRequest {};

        let req = serde_json::to_value(block_req)?;

        let resp = self.call_method_with_payload("get_last_block", req).await?;

        let resp_deser = serde_json::from_value(resp)?;

        Ok(resp_deser)
    }

    ///Get account public balance for `address`. `address` must be a valid hex-string for 32 bytes.
    pub async fn get_account_balance(
        &self,
        address: String,
    ) -> Result<GetAccountBalanceResponse, SequencerClientError> {
        let block_req = GetAccountBalanceRequest { address };

        let req = serde_json::to_value(block_req)?;

        let resp = self
            .call_method_with_payload("get_account_balance", req)
            .await?;

        let resp_deser = serde_json::from_value(resp)?;

        Ok(resp_deser)
    }

    ///Get accounts nonces for `addresses`. `addresses` must be a list of valid hex-strings for 32 bytes.
    pub async fn get_accounts_nonces(
        &self,
        addresses: Vec<String>,
    ) -> Result<GetAccountsNoncesResponse, SequencerClientError> {
        let block_req = GetAccountsNoncesRequest { addresses };

        let req = serde_json::to_value(block_req)?;

        let resp = self
            .call_method_with_payload("get_accounts_nonces", req)
            .await?;

        let resp_deser = serde_json::from_value(resp)?;

        Ok(resp_deser)
    }

    pub async fn get_account(
        &self,
        address: String,
    ) -> Result<GetAccountResponse, SequencerClientError> {
        let block_req = GetAccountRequest { address };

        let req = serde_json::to_value(block_req)?;

        let resp = self.call_method_with_payload("get_account", req).await?;

        let resp_deser = serde_json::from_value(resp)?;

        Ok(resp_deser)
    }

    ///Get transaction details for `hash`.
    pub async fn get_transaction_by_hash(
        &self,
        hash: String,
    ) -> Result<GetTransactionByHashResponse, SequencerClientError> {
        let block_req = GetTransactionByHashRequest { hash };

        let req = serde_json::to_value(block_req)?;

        let resp = self
            .call_method_with_payload("get_transaction_by_hash", req)
            .await?;

        let resp_deser = serde_json::from_value(resp)?;

        Ok(resp_deser)
    }

    ///Send transaction to sequencer
    pub async fn send_tx_public(
        &self,
        transaction: nssa::PublicTransaction,
    ) -> Result<SendTxResponse, SequencerClientError> {
        let transaction = EncodedTransaction::from(NSSATransaction::Public(transaction));

        let tx_req = SendTxRequest {
            transaction: borsh::to_vec(&transaction).unwrap(),
        };

        let req = serde_json::to_value(tx_req)?;

        let resp = self.call_method_with_payload("send_tx", req).await?;

        let resp_deser = serde_json::from_value(resp)?;

        Ok(resp_deser)
    }

    ///Send transaction to sequencer
    pub async fn send_tx_private(
        &self,
        transaction: nssa::PrivacyPreservingTransaction,
    ) -> Result<SendTxResponse, SequencerClientError> {
        let transaction = EncodedTransaction::from(NSSATransaction::PrivacyPreserving(transaction));

        let tx_req = SendTxRequest {
            transaction: borsh::to_vec(&transaction).unwrap(),
        };

        let req = serde_json::to_value(tx_req)?;

        let resp = self.call_method_with_payload("send_tx", req).await?;

        let resp_deser = serde_json::from_value(resp)?;

        Ok(resp_deser)
    }

    ///Get genesis id from sequencer
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

    ///Get initial testnet accounts from sequencer
    pub async fn get_initial_testnet_accounts(
        &self,
    ) -> Result<Vec<AccountInitialData>, SequencerClientError> {
        let acc_req = GetInitialTestnetAccountsRequest {};

        let req = serde_json::to_value(acc_req).unwrap();

        let resp = self
            .call_method_with_payload("get_initial_testnet_accounts", req)
            .await
            .unwrap();

        let resp_deser = serde_json::from_value(resp).unwrap();

        Ok(resp_deser)
    }

    ///Get proof for commitment
    pub async fn get_proof_for_commitment(
        &self,
        commitment: nssa_core::Commitment,
    ) -> Result<Option<nssa_core::MembershipProof>, SequencerClientError> {
        let acc_req = GetProofForCommitmentRequest { commitment };

        let req = serde_json::to_value(acc_req).unwrap();

        let resp = self
            .call_method_with_payload("get_proof_for_commitment", req)
            .await
            .unwrap();

        let resp_deser = serde_json::from_value::<GetProofForCommitmentResponse>(resp)
            .unwrap()
            .membership_proof;

        Ok(resp_deser)
    }

    pub async fn send_tx_program(
        &self,
        transaction: nssa::ProgramDeploymentTransaction,
    ) -> Result<SendTxResponse, SequencerClientError> {
        let transaction = EncodedTransaction::from(NSSATransaction::ProgramDeployment(transaction));

        let tx_req = SendTxRequest {
            transaction: borsh::to_vec(&transaction).unwrap(),
        };

        let req = serde_json::to_value(tx_req)?;

        let resp = self.call_method_with_payload("send_tx", req).await?;

        let resp_deser = serde_json::from_value(resp)?;

        Ok(resp_deser)
    }

    /// Get Ids of the programs used by the node
    pub async fn get_program_ids(
        &self,
    ) -> Result<HashMap<String, ProgramId>, SequencerClientError> {
        let acc_req = GetProgramIdsRequest {};

        let req = serde_json::to_value(acc_req).unwrap();

        let resp = self
            .call_method_with_payload("get_program_ids", req)
            .await
            .unwrap();

        let resp_deser = serde_json::from_value::<GetProgramIdsResponse>(resp)
            .unwrap()
            .program_ids;

        Ok(resp_deser)
    }
}
