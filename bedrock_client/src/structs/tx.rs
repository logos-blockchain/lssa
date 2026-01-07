use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SignedMantleTx {
    pub mantle_tx: MantleTx,
    pub ops_proofs: Vec<OpProof>,
    // Not sure, if we need this.
    // ledger_tx_proof: ZkSignature,
}