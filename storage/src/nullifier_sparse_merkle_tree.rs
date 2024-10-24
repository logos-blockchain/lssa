use monotree::database::MemoryDB;
use monotree::hasher::Blake3;
use monotree::{Hasher, Monotree, Proof};

use crate::merkle_tree_public::TreeHashType;
use crate::nullifier::UTXONullifier;

pub struct NullifierSparseMerkleTree {
    pub curr_root: Option<TreeHashType>,
    pub tree: Monotree<MemoryDB, Blake3>,
    pub hasher: Blake3,
}

impl NullifierSparseMerkleTree {
    pub fn new() -> Self {
        NullifierSparseMerkleTree {
            curr_root: None,
            tree: Monotree::default(),
            hasher: Blake3::new(),
        }
    }

    pub fn insert_item(&mut self, nullifier: UTXONullifier) -> Result<(), monotree::Errors> {
        let root = self.curr_root.as_ref();

        let new_root = self
            .tree
            .insert(root, &nullifier.utxo_hash, &nullifier.utxo_hash)?;

        self.curr_root = new_root;

        Ok(())
    }

    pub fn insert_items(&mut self, nullifiers: Vec<UTXONullifier>) -> Result<(), monotree::Errors> {
        let root = self.curr_root.as_ref();

        let hashes: Vec<TreeHashType> = nullifiers.iter().map(|nu| nu.utxo_hash).collect();

        let new_root = self.tree.inserts(root, &hashes, &hashes)?;

        self.curr_root = new_root;

        Ok(())
    }

    pub fn search_item_inclusion(
        &mut self,
        nullifier_hash: TreeHashType,
    ) -> Result<bool, monotree::Errors> {
        self.tree
            .get(self.curr_root.as_ref(), &nullifier_hash)
            .map(|data| data.is_some())
    }

    pub fn search_item_inclusions(
        &mut self,
        nullifier_hashes: &[TreeHashType],
    ) -> Result<Vec<bool>, monotree::Errors> {
        let mut inclusions = vec![];

        for nullifier_hash in nullifier_hashes {
            let is_included = self
                .tree
                .get(self.curr_root.as_ref(), nullifier_hash)
                .map(|data| data.is_some())?;

            inclusions.push(is_included);
        }

        Ok(inclusions)
    }

    pub fn get_non_membership_proof(
        &mut self,
        nullifier_hash: TreeHashType,
    ) -> Result<(Option<Proof>, Option<TreeHashType>), monotree::Errors> {
        let is_member = self.search_item_inclusion(nullifier_hash)?;

        if is_member {
            Err(monotree::Errors::new("Is a member"))
        } else {
            Ok((
                self.tree
                    .get_merkle_proof(self.curr_root.as_ref(), &nullifier_hash)?,
                self.curr_root,
            ))
        }
    }

    #[allow(clippy::type_complexity)]
    pub fn get_non_membership_proofs(
        &mut self,
        nullifier_hashes: &[TreeHashType],
    ) -> Result<Vec<(Option<Proof>, Option<TreeHashType>)>, monotree::Errors> {
        let mut non_membership_proofs = vec![];

        for nullifier_hash in nullifier_hashes {
            let is_member = self.search_item_inclusion(*nullifier_hash)?;

            if is_member {
                return Err(monotree::Errors::new(
                    format!("{nullifier_hash:?} Is a member").as_str(),
                ));
            } else {
                non_membership_proofs.push((
                    self.tree
                        .get_merkle_proof(self.curr_root.as_ref(), nullifier_hash)?,
                    self.curr_root,
                ))
            };
        }

        Ok(non_membership_proofs)
    }
}

impl Default for NullifierSparseMerkleTree {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nullifier::UTXONullifier;
    use monotree::database::MemoryDB;
    use monotree::hasher::Blake3;
    use monotree::Monotree;

    fn create_nullifier(hash: TreeHashType) -> UTXONullifier {
        UTXONullifier { utxo_hash: hash }
    }

    #[test]
    fn test_new_tree_initialization() {
        let tree = NullifierSparseMerkleTree::new();
        assert!(tree.curr_root.is_none());
    }

    #[test]
    fn test_insert_single_item() {
        let mut tree = NullifierSparseMerkleTree::new();
        let nullifier = create_nullifier([1u8; 32]); // Sample 32-byte hash

        let result = tree.insert_item(nullifier);
        assert!(result.is_ok());
        assert!(tree.curr_root.is_some());
    }

    #[test]
    fn test_insert_multiple_items() {
        let mut tree = NullifierSparseMerkleTree::new();
        let nullifiers = vec![
            create_nullifier([1u8; 32]),
            create_nullifier([2u8; 32]),
            create_nullifier([3u8; 32]),
        ];

        let result = tree.insert_items(nullifiers);
        assert!(result.is_ok());
        assert!(tree.curr_root.is_some());
    }

    #[test]
    fn test_search_item_inclusion() {
        let mut tree = NullifierSparseMerkleTree::new();
        let nullifier = create_nullifier([1u8; 32]);

        tree.insert_item(nullifier.clone()).unwrap();

        let result = tree.search_item_inclusion([1u8; 32]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), true);

        let non_existing = tree.search_item_inclusion([99u8; 32]);
        assert!(non_existing.is_ok());
        assert_eq!(non_existing.unwrap(), false);
    }

    #[test]
    fn test_search_multiple_item_inclusions() {
        let mut tree = NullifierSparseMerkleTree::new();
        let nullifiers = vec![
            create_nullifier([1u8; 32]),
            create_nullifier([2u8; 32]),
            create_nullifier([3u8; 32]),
        ];

        tree.insert_items(nullifiers).unwrap();

        let search_hashes = vec![[1u8; 32], [2u8; 32], [99u8; 32]];
        let result = tree.search_item_inclusions(&search_hashes);
        assert!(result.is_ok());

        let expected_results = vec![true, true, false];
        assert_eq!(result.unwrap(), expected_results);
    }

    #[test]
    fn test_non_membership_proof() {
        let mut tree = NullifierSparseMerkleTree::new();
        let non_member_hash = [5u8; 32];

        let result = tree.get_non_membership_proof(non_member_hash);
        assert!(result.is_ok());

        let (proof, root) = result.unwrap();
        assert!(root.is_none());
    }

    #[test]
    fn test_non_membership_proofs_multiple() {
        let mut tree = NullifierSparseMerkleTree::new();
        let non_member_hashes = vec![[5u8; 32], [6u8; 32], [7u8; 32]];

        let result = tree.get_non_membership_proofs(&non_member_hashes);
        assert!(result.is_ok());

        let proofs = result.unwrap();
        for (proof, root) in proofs {
            assert!(root.is_none());
        }
    }

    #[test]
    fn test_insert_and_get_proof_of_existing_item() {
        let mut tree = NullifierSparseMerkleTree::new();
        let nullifier = create_nullifier([1u8; 32]);

        tree.insert_item(nullifier.clone()).unwrap();

        let proof_result = tree.get_non_membership_proof([1u8; 32]);
        assert!(proof_result.is_err());
    }

    #[test]
    fn test_insert_and_get_proofs_of_existing_items() {
        let mut tree = NullifierSparseMerkleTree::new();
        let nullifiers = vec![create_nullifier([1u8; 32]), create_nullifier([2u8; 32])];

        tree.insert_items(nullifiers).unwrap();

        let proof_result = tree.get_non_membership_proofs(&[[1u8; 32], [2u8; 32]]);
        assert!(proof_result.is_err());
    }
}
