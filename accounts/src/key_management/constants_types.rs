use elliptic_curve::{
    consts::{B0, B1},
    generic_array::GenericArray,
};
use sha2::digest::typenum::{UInt, UTerm};

pub const NULLIFIER_SECRET_CONST: [u8; 32] = [
    38, 29, 97, 210, 148, 172, 75, 220, 36, 249, 27, 111, 73, 14, 250, 38, 55, 87, 164, 169, 95,
    101, 135, 28, 212, 241, 107, 46, 162, 60, 59, 93,
];
pub const VIEVING_SECRET_CONST: [u8; 32] = [
    97, 23, 175, 117, 11, 48, 215, 162, 150, 103, 46, 195, 179, 178, 93, 52, 137, 190, 202, 60,
    254, 87, 112, 250, 57, 242, 117, 206, 195, 149, 213, 206,
];

pub type CipherText = Vec<u8>;
pub type Nonce = GenericArray<u8, UInt<UInt<UInt<UInt<UTerm, B1>, B1>, B0>, B0>>;
