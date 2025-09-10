use elliptic_curve::{
    consts::{B0, B1},
    generic_array::GenericArray,
};
use sha2::digest::typenum::{UInt, UTerm};

pub type CipherText = Vec<u8>;
pub type Nonce = GenericArray<u8, UInt<UInt<UInt<UInt<UTerm, B1>, B1>, B0>, B0>>;
