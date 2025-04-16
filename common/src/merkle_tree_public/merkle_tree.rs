use std::collections::HashMap;

use rs_merkle::{MerkleProof, MerkleTree};

use crate::{transaction::Transaction, utxo_commitment::UTXOCommitment};

use super::{hasher::OwnHasher, tree_leav_item::TreeLeavItem, TreeHashType};

pub struct HashStorageMerkleTree<Leav: TreeLeavItem + Clone> {
    leaves: HashMap<usize, Leav>,
    hash_to_id_map: HashMap<TreeHashType, usize>,
    tree: MerkleTree<OwnHasher>,
}

pub type PublicTransactionMerkleTree = HashStorageMerkleTree<Transaction>;

pub type UTXOCommitmentsMerkleTree = HashStorageMerkleTree<UTXOCommitment>;

impl<Leav: TreeLeavItem + Clone> HashStorageMerkleTree<Leav> {
    pub fn new(leaves_vec: Vec<Leav>) -> Self {
        let mut leaves_map = HashMap::new();
        let mut hash_to_id_map = HashMap::new();

        let leaves_hashed: Vec<TreeHashType> = leaves_vec
            .iter()
            .enumerate()
            .map(|(id, tx)| {
                leaves_map.insert(id, tx.clone());
                hash_to_id_map.insert(tx.hash(), id);
                tx.hash()
            })
            .collect();
        Self {
            leaves: leaves_map,
            hash_to_id_map,
            tree: MerkleTree::from_leaves(&leaves_hashed),
        }
    }

    pub fn get_tx(&self, hash: TreeHashType) -> Option<&Leav> {
        self.hash_to_id_map
            .get(&hash)
            .and_then(|id| self.leaves.get(id))
    }

    pub fn get_root(&self) -> Option<TreeHashType> {
        self.tree.root()
    }

    pub fn get_proof(&self, hash: TreeHashType) -> Option<MerkleProof<OwnHasher>> {
        self.hash_to_id_map
            .get(&hash)
            .map(|id| self.tree.proof(&[*id]))
    }

    pub fn get_proof_multiple(&self, hashes: &[TreeHashType]) -> Option<MerkleProof<OwnHasher>> {
        let ids_opt: Vec<Option<&usize>> = hashes
            .iter()
            .map(|hash| self.hash_to_id_map.get(hash))
            .collect();

        let is_valid = ids_opt.iter().all(|el| el.is_some());

        if is_valid {
            let ids: Vec<usize> = ids_opt.into_iter().map(|el| *el.unwrap()).collect();

            Some(self.tree.proof(&ids))
        } else {
            None
        }
    }

    pub fn add_tx(&mut self, tx: Leav) {
        let last = self.leaves.len();

        self.leaves.insert(last, tx.clone());
        self.hash_to_id_map.insert(tx.hash(), last);

        self.tree.insert(tx.hash());

        self.tree.commit();
    }

    pub fn add_tx_multiple(&mut self, txs: Vec<Leav>) {
        for tx in txs.iter() {
            let last = self.leaves.len();

            self.leaves.insert(last, tx.clone());
            self.hash_to_id_map.insert(tx.hash(), last);
        }

        self.tree
            .append(&mut txs.iter().map(|tx| tx.hash()).collect());

        self.tree.commit();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mock implementation of TreeLeavItem trait for testing
    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    struct MockTransaction {
        pub hash: TreeHashType,
    }

    impl TreeLeavItem for MockTransaction {
        fn hash(&self) -> TreeHashType {
            self.hash
        }
    }

    fn get_first_32_bytes(s: &str) -> [u8; 32] {
        let mut buffer = [0u8; 32];
        let bytes = s.as_bytes();
        let len = std::cmp::min(32, bytes.len());

        buffer[..len].copy_from_slice(&bytes[..len]);
        buffer
    }

    #[test]
    fn test_new_merkle_tree() {
        let tx1 = MockTransaction {
            hash: get_first_32_bytes("tx1"),
        };
        let tx2 = MockTransaction {
            hash: get_first_32_bytes("tx2"),
        };

        let tree = HashStorageMerkleTree::new(vec![tx1.clone(), tx2.clone()]);

        assert_eq!(tree.leaves.len(), 2);
        assert!(tree.get_root().is_some());
    }

    #[test]
    fn test_get_tx() {
        let tx1 = MockTransaction {
            hash: get_first_32_bytes("tx1"),
        };
        let tx2 = MockTransaction {
            hash: get_first_32_bytes("tx2"),
        };

        let tree = HashStorageMerkleTree::new(vec![tx1.clone(), tx2.clone()]);

        assert_eq!(tree.get_tx(tx1.hash()), Some(&tx1));
        assert_eq!(tree.get_tx(tx2.hash()), Some(&tx2));
    }

    #[test]
    fn test_get_proof() {
        let tx1 = MockTransaction {
            hash: get_first_32_bytes("tx1"),
        };
        let tx2 = MockTransaction {
            hash: get_first_32_bytes("tx2"),
        };

        let tree = HashStorageMerkleTree::new(vec![tx1.clone(), tx2.clone()]);

        let proof = tree.get_proof(tx1.hash());
        assert!(proof.is_some());
    }

    #[test]
    fn test_add_tx() {
        let tx1 = MockTransaction {
            hash: get_first_32_bytes("tx1"),
        };
        let tx2 = MockTransaction {
            hash: get_first_32_bytes("tx2"),
        };

        let mut tree = HashStorageMerkleTree::new(vec![tx1.clone()]);

        tree.add_tx(tx2.clone());
        assert_eq!(tree.leaves.len(), 2);
        assert_eq!(tree.get_tx(tx2.hash()), Some(&tx2));
    }

    #[test]
    fn test_add_tx_multiple() {
        let tx1 = MockTransaction {
            hash: get_first_32_bytes("tx1"),
        };
        let tx2 = MockTransaction {
            hash: get_first_32_bytes("tx2"),
        };
        let tx3 = MockTransaction {
            hash: get_first_32_bytes("tx3"),
        };

        let mut tree = HashStorageMerkleTree::new(vec![tx1.clone()]);
        tree.add_tx_multiple(vec![tx2.clone(), tx3.clone()]);

        assert_eq!(tree.leaves.len(), 3);
        assert_eq!(tree.get_tx(tx2.hash()), Some(&tx2));
        assert_eq!(tree.get_tx(tx3.hash()), Some(&tx3));
    }

    #[test]
    fn test_get_proof_multiple() {
        let tx1 = MockTransaction {
            hash: get_first_32_bytes("tx1"),
        };
        let tx2 = MockTransaction {
            hash: get_first_32_bytes("tx2"),
        };
        let tx3 = MockTransaction {
            hash: get_first_32_bytes("tx3"),
        };

        let tree = HashStorageMerkleTree::new(vec![tx1.clone(), tx2.clone(), tx3.clone()]);
        let proof = tree.get_proof_multiple(&[tx1.hash(), tx2.hash()]);

        assert!(proof.is_some());
    }
}
