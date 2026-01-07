use serde::{Deserialize, Serialize};
use serde_with::serde_as;

use crate::structs::header_id::HeaderId;

#[derive(Clone, Debug, Eq, PartialEq, Copy, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Slot(u64);

#[derive(Clone, Debug, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum State {
    Bootstrapping,
    Online,
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CryptarchiaInfo {
    pub lib: HeaderId,
    pub tip: HeaderId,
    pub slot: Slot,
    pub height: u64,
    pub mode: State,
}