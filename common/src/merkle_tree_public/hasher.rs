use rs_merkle::Hasher;
use sha2::{Digest, Sha256, digest::FixedOutput};

use super::TreeHashType;

#[derive(Debug, Clone)]
///Our own hasher.
/// Currently it is SHA256 hasher wrapper. May change in a future.
pub struct OwnHasher {}

impl Hasher for OwnHasher {
    type Hash = TreeHashType;

    fn hash(data: &[u8]) -> TreeHashType {
        let mut hasher = Sha256::new();

        hasher.update(data);
        <TreeHashType>::from(hasher.finalize_fixed())
    }
}
