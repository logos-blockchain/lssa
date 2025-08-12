use std::io::{Cursor, Read};

use crate::{PublicKey, Signature};


impl PublicKey {
    // TODO: remove unwraps and return Result
    pub(crate) fn from_cursor(cursor: &mut Cursor<&[u8]>) -> Self {
        let mut value = [0u8; 32];
        cursor.read_exact(&mut value).unwrap();
        Self(value)
    }
}

impl Signature {
    // TODO: remove unwraps and return Result
    pub(crate) fn from_cursor(cursor: &mut Cursor<&[u8]>) -> Self {
        let mut value = [0u8; 64];
        cursor.read_exact(&mut value).unwrap();
        Self { value }
    }
}
