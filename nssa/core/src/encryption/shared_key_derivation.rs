use serde::{Deserialize, Serialize};

use k256::{
    AffinePoint, EncodedPoint, FieldBytes, ProjectivePoint, Scalar,
    elliptic_curve::{
        PrimeField,
        sec1::{FromEncodedPoint, ToEncodedPoint},
    },
};

use crate::SharedSecretKey;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Secp256k1Point(pub(crate) Vec<u8>);

impl Secp256k1Point {
    pub fn from_scalar(value: [u8; 32]) -> Secp256k1Point {
        let x_bytes: FieldBytes = value.into();
        let x = Scalar::from_repr(x_bytes).unwrap();

        let p = ProjectivePoint::GENERATOR * x;
        let q = AffinePoint::from(p);
        let enc = q.to_encoded_point(true);

        Self(enc.as_bytes().to_vec())
    }
}

pub type EphemeralSecretKey = [u8; 32];
pub type EphemeralPublicKey = Secp256k1Point;
pub type IncomingViewingPublicKey = Secp256k1Point;
impl From<&EphemeralSecretKey> for EphemeralPublicKey {
    fn from(value: &EphemeralSecretKey) -> Self {
        Secp256k1Point::from_scalar(*value)
    }
}

impl SharedSecretKey {
    pub fn new(scalar: &[u8; 32], point: &Secp256k1Point) -> Self {
        let scalar = Scalar::from_repr((*scalar).into()).unwrap();
        let point: [u8; 33] = point.0.clone().try_into().unwrap();

        let encoded = EncodedPoint::from_bytes(point).unwrap();
        let pubkey_affine = AffinePoint::from_encoded_point(&encoded).unwrap();

        let shared = ProjectivePoint::from(pubkey_affine) * scalar;
        let shared_affine = shared.to_affine();

        let encoded = shared_affine.to_encoded_point(false);
        let x_bytes_slice = encoded.x().unwrap();
        let mut x_bytes = [0u8; 32];
        x_bytes.copy_from_slice(x_bytes_slice);

        Self(x_bytes)
    }
}
