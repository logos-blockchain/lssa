use elliptic_curve::{
    consts::{B0, B1},
    generic_array::GenericArray,
};
use lazy_static::lazy_static;
use sha2::digest::typenum::{UInt, UTerm};

lazy_static! {
    pub static ref NULLIFIER_SECRET_CONST: [u8; 32] =
        hex::decode(std::env::var("NULLIFIER_SECRET_CONST").unwrap())
            .unwrap()
            .try_into()
            .unwrap();
    pub static ref VIEWING_SECRET_CONST: [u8; 32] =
        hex::decode(std::env::var("VIEWING_SECRET_CONST").unwrap())
            .unwrap()
            .try_into()
            .unwrap();
}

pub type CipherText = Vec<u8>;
pub type Nonce = GenericArray<u8, UInt<UInt<UInt<UInt<UTerm, B1>, B1>, B0>, B0>>;
