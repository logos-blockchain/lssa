use rs_merkle::Hasher;
use sha2::{digest::FixedOutput, Digest, Sha256};

use super::HashType;

#[derive(Debug, Clone)]
///Our own hasher.
/// Currently it is SHA256 hasher wrapper. May change in a future.
pub struct OwnHasher {}

impl Hasher for OwnHasher {
    type Hash = HashType;

    fn hash(data: &[u8]) -> HashType {
        let mut hasher = Sha256::new();

        hasher.update(data);
        <HashType>::from(hasher.finalize_fixed())
    }
}
