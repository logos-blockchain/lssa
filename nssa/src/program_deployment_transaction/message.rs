#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub(crate) bytecode: Vec<u8>,
}

impl Message {
    pub fn new(bytecode: Vec<u8>) -> Self {
        Self { bytecode }
    }
}
