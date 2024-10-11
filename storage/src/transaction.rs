use serde::{Deserialize, Serialize};

pub type TxHash = [u8; 32];

//ToDo: Update Tx model, when it is clear
#[derive(Debug, Serialize, Deserialize, Clone)]
///General transaction object
pub struct Transaction {
    pub hash: TxHash,
}
