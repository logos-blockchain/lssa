use std::collections::HashMap;

use rs_merkle::{MerkleProof, MerkleTree};

use crate::transaction::Transaction;

use super::{hasher::OwnHasher, HashType};

pub struct PublicTransactionsMerkleTree {
    leaves: HashMap<usize, Transaction>,
    hash_to_id_map: HashMap<HashType, usize>,
    tree: MerkleTree<OwnHasher>,
}

impl PublicTransactionsMerkleTree {
    pub fn new(leaves_vec: Vec<Transaction>) -> Self {
        let mut leaves_map = HashMap::new();
        let mut hash_to_id_map = HashMap::new();

        let leaves_hashed: Vec<HashType> = leaves_vec
            .iter()
            .enumerate()
            .map(|(id, tx)| {
                leaves_map.insert(id, tx.clone());
                hash_to_id_map.insert(tx.hash, id);
                tx.hash
            })
            .collect();
        Self {
            leaves: leaves_map,
            hash_to_id_map,
            tree: MerkleTree::from_leaves(&leaves_hashed),
        }
    }

    pub fn get_tx(&self, hash: HashType) -> Option<&Transaction> {
        self.hash_to_id_map
            .get(&hash)
            .and_then(|id| self.leaves.get(id))
    }

    pub fn get_root(&self) -> Option<HashType> {
        self.tree.root()
    }

    pub fn get_proof(&self, hash: HashType) -> Option<MerkleProof<OwnHasher>> {
        self.hash_to_id_map
            .get(&hash)
            .map(|id| self.tree.proof(&[*id]))
    }

    pub fn get_proof_multiple(&self, hashes: &[HashType]) -> Option<MerkleProof<OwnHasher>> {
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

    pub fn add_tx(&mut self, tx: Transaction) {
        let last = self.leaves.len();

        self.leaves.insert(last, tx.clone());
        self.hash_to_id_map.insert(tx.hash, last);

        self.tree.insert(tx.hash);

        self.tree.commit();
    }

    pub fn add_tx_multiple(&mut self, txs: Vec<Transaction>) {
        for tx in txs.iter() {
            let last = self.leaves.len();

            self.leaves.insert(last, tx.clone());
            self.hash_to_id_map.insert(tx.hash, last);
        }

        self.tree
            .append(&mut txs.iter().map(|tx| tx.hash).collect());

        self.tree.commit();
    }
}
