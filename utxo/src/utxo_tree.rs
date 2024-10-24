use std::collections::HashMap;

use monotree::database::MemoryDB;
use monotree::hasher::Blake3;
use monotree::{Hasher, Monotree, Proof};
use storage::merkle_tree_public::TreeHashType;

use crate::utxo_core::UTXO;

pub struct UTXOSparseMerkleTree {
    pub curr_root: Option<TreeHashType>,
    pub tree: Monotree<MemoryDB, Blake3>,
    pub hasher: Blake3,
    pub store: HashMap<TreeHashType, UTXO>,
}

impl UTXOSparseMerkleTree {
    pub fn new() -> Self {
        UTXOSparseMerkleTree {
            curr_root: None,
            tree: Monotree::default(),
            hasher: Blake3::new(),
            store: HashMap::new(),
        }
    }

    pub fn insert_item(&mut self, utxo: UTXO) -> Result<(), monotree::Errors> {
        let root = self.curr_root.as_ref();

        let new_root = self.tree.insert(root, &utxo.hash, &utxo.hash)?;

        self.store.insert(utxo.hash, utxo);

        self.curr_root = new_root;

        Ok(())
    }

    pub fn insert_items(&mut self, utxos: Vec<UTXO>) -> Result<(), monotree::Errors> {
        let root = self.curr_root.as_ref();

        let hashes: Vec<TreeHashType> = utxos.iter().map(|item| item.hash).collect();

        let new_root = self.tree.inserts(root, &hashes, &hashes)?;

        for utxo in utxos {
            self.store.insert(utxo.hash, utxo);
        }

        self.curr_root = new_root;

        Ok(())
    }

    pub fn get_item(&mut self, hash: TreeHashType) -> Result<Option<&UTXO>, monotree::Errors> {
        let hash = self.tree.get(self.curr_root.as_ref(), &hash)?;

        Ok(hash.and_then(|hash| self.store.get(&hash)))
    }

    pub fn get_membership_proof(
        &mut self,
        nullifier_hash: TreeHashType,
    ) -> Result<Option<Proof>, monotree::Errors> {
        self.tree
            .get_merkle_proof(self.curr_root.as_ref(), &nullifier_hash)
    }
}

impl Default for UTXOSparseMerkleTree {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use storage::{merkle_tree_public::TreeHashType, AccountId};
    use crate::utxo_core::{UTXO, UTXOPayload};

    fn sample_utxo_payload() -> UTXOPayload {
        UTXOPayload {
            owner: AccountId::default(),
            asset: vec![1, 2, 3],
        }
    }

    fn sample_utxo() -> UTXO {
        UTXO::create_utxo_from_payload(sample_utxo_payload())
    }

    #[test]
    fn test_utxo_sparse_merkle_tree_new() {
        let smt = UTXOSparseMerkleTree::new();
        assert!(smt.curr_root.is_none());
        assert_eq!(smt.store.len(), 0);
    }

    #[test]
    fn test_insert_item() {
        let mut smt = UTXOSparseMerkleTree::new();
        let utxo = sample_utxo();

        let result = smt.insert_item(utxo.clone());

        // Test insertion is successful
        assert!(result.is_ok());

        // Test UTXO is now stored in the tree
        assert_eq!(smt.store.get(&utxo.hash).unwrap().hash, utxo.hash);

        // Test curr_root is updated
        assert!(smt.curr_root.is_some());
    }

    #[test]
    fn test_insert_items() {
        let mut smt = UTXOSparseMerkleTree::new();
        let utxo1 = sample_utxo();
        let utxo2 = sample_utxo();

        let result = smt.insert_items(vec![utxo1.clone(), utxo2.clone()]);

        // Test insertion of multiple items is successful
        assert!(result.is_ok());

        // Test UTXOs are now stored in the tree
        assert!(smt.store.get(&utxo1.hash).is_some());
        assert!(smt.store.get(&utxo2.hash).is_some());

        // Test curr_root is updated
        assert!(smt.curr_root.is_some());
    }

}
