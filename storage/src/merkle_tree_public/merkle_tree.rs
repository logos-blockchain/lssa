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
