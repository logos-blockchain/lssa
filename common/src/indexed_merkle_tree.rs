use crate::merkle_tree_public::TreeHashType;

use anyhow::Result;
use indexed_merkle_tree::node::Node;
use indexed_merkle_tree::tree::{IndexedMerkleTree, NonMembershipProof};
use indexed_merkle_tree::Hash;

pub struct IndexedMerkleTreeWrapper {
    pub tree: IndexedMerkleTree,
    pub leaf_len: usize,
    //pub leafs: BTreeMap<u64, TreeBlockWithTxId>,
}

impl IndexedMerkleTreeWrapper {
    pub fn new() -> Self {
        Self {
            //Will not panic
            //Deterministic operation
            tree: IndexedMerkleTree::new(vec![]).unwrap(),
            leaf_len: 0,
        }
    }

    pub fn get_curr_root(&self) -> Result<TreeHashType> {
        //HELP
        // self.tree.get_root().map(|node|
        //    serde_json::from_str::<Vec<[TreeHashType]>>(&serde_json::to_string(node).unwrap())[0]
        // )
        Ok([0; 32])
    }

    pub fn insert_item(&mut self, hash: TreeHashType) -> Result<()> {
        let left_parity = self.leaf_len / 2;
        let mut node = match left_parity {
            0 => Node::new_leaf(true, Hash::new(hash), Hash::new(hash), Node::TAIL),
            1 => Node::new_leaf(false, Hash::new(hash), Hash::new(hash), Node::TAIL),
            _ => unreachable!(),
        };

        self.tree.insert_node(&mut node)?;

        self.leaf_len += 1;

        Ok(())
    }

    pub fn insert_items(&mut self, tree_nullifiers: Vec<TreeHashType>) -> Result<()> {
        for tree_nullifier in tree_nullifiers {
            self.insert_item(tree_nullifier)?;
        }

        Ok(())
    }

    pub fn search_item_inclusion(&mut self, nullifier_hash: TreeHashType) -> bool {
        self.tree
            .find_leaf_by_label(&Hash::new(nullifier_hash))
            .is_some()
    }

    pub fn search_item_inclusions(&mut self, nullifier_hashes: &[TreeHashType]) -> Vec<bool> {
        let mut inclusions = vec![];

        for nullifier_hash in nullifier_hashes {
            let is_included = self.search_item_inclusion(*nullifier_hash);

            inclusions.push(is_included);
        }

        inclusions
    }

    pub fn get_non_membership_proof(
        &mut self,
        nullifier_hash: TreeHashType,
    ) -> Result<NonMembershipProof> {
        let node = Node::new_leaf(
            false,
            Hash::new(nullifier_hash),
            Hash::new(nullifier_hash),
            Node::TAIL,
        );

        self.tree.generate_non_membership_proof(&node)
    }

    #[allow(clippy::type_complexity)]
    pub fn get_non_membership_proofs(
        &mut self,
        nullifier_hashes: &[TreeHashType],
    ) -> Result<Vec<NonMembershipProof>> {
        let mut non_membership_proofs = vec![];

        for nullifier_hash in nullifier_hashes {
            let non_mem_proof = self.get_non_membership_proof(*nullifier_hash)?;

            non_membership_proofs.push(non_mem_proof);
        }

        Ok(non_membership_proofs)
    }
}

impl Default for IndexedMerkleTreeWrapper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    // use super::*;
    // use crate::nullifier::UTXONullifier;

    // fn create_nullifier(hash: TreeHashType) -> UTXONullifier {
    //     UTXONullifier { utxo_hash: hash }
    // }

    // fn create_nullifier_input(
    //     hash: TreeHashType,
    //     nullifier_id: u64,
    //     tx_id: u64,
    //     block_id: u64,
    // ) -> NullifierTreeInput {
    //     NullifierTreeInput {
    //         nullifier_id,
    //         tx_id,
    //         block_id,
    //         nullifier: create_nullifier(hash),
    //     }
    // }

    // #[test]
    // fn test_new_tree_initialization() {
    //     let tree = IndexedMerkleTree::new();
    //     assert!(tree.curr_root.is_none());
    // }

    // #[test]
    // fn test_insert_single_item() {
    //     let mut tree = IndexedMerkleTree::new();
    //     let tree_nullifier = create_nullifier_input([1u8; 32], 1, 1, 1); // Sample 32-byte hash

    //     let result = tree.insert_item(tree_nullifier);
    //     assert!(result.is_ok());
    //     assert!(tree.curr_root.is_some());
    // }

    // #[test]
    // fn test_insert_multiple_items() {
    //     let mut tree = IndexedMerkleTree::new();
    //     let tree_nullifiers = vec![
    //         create_nullifier_input([1u8; 32], 1, 1, 1),
    //         create_nullifier_input([2u8; 32], 2, 1, 1),
    //         create_nullifier_input([3u8; 32], 3, 1, 1),
    //     ];

    //     let result = tree.insert_items(tree_nullifiers);
    //     assert!(result.is_ok());
    //     assert!(tree.curr_root.is_some());
    // }

    // #[test]
    // fn test_search_item_inclusion() {
    //     let mut tree = IndexedMerkleTree::new();
    //     let tree_nullifier = create_nullifier_input([1u8; 32], 1, 1, 1);

    //     tree.insert_item(tree_nullifier.clone()).unwrap();

    //     let result = tree.search_item_inclusion([1u8; 32]);
    //     assert!(result.is_ok());
    //     assert_eq!(result.unwrap(), true);

    //     let non_existing = tree.search_item_inclusion([99u8; 32]);
    //     assert!(non_existing.is_ok());
    //     assert_eq!(non_existing.unwrap(), false);
    // }

    // #[test]
    // fn test_search_multiple_item_inclusions() {
    //     let mut tree = IndexedMerkleTree::new();
    //     let tree_nullifiers = vec![
    //         create_nullifier_input([1u8; 32], 1, 1, 1),
    //         create_nullifier_input([2u8; 32], 2, 1, 1),
    //         create_nullifier_input([3u8; 32], 3, 1, 1),
    //     ];

    //     tree.insert_items(tree_nullifiers).unwrap();

    //     let search_hashes = vec![[1u8; 32], [2u8; 32], [99u8; 32]];
    //     let result = tree.search_item_inclusions(&search_hashes);
    //     assert!(result.is_ok());

    //     let expected_results = vec![true, true, false];
    //     assert_eq!(result.unwrap(), expected_results);
    // }

    // #[test]
    // fn test_non_membership_proof() {
    //     let mut tree = IndexedMerkleTree::new();
    //     let non_member_hash = [5u8; 32];

    //     let result = tree.get_non_membership_proof(non_member_hash);
    //     assert!(result.is_ok());

    //     let (proof, root) = result.unwrap();
    //     assert!(root.is_none());
    // }

    // #[test]
    // fn test_non_membership_proofs_multiple() {
    //     let mut tree = IndexedMerkleTree::new();
    //     let non_member_hashes = vec![[5u8; 32], [6u8; 32], [7u8; 32]];

    //     let result = tree.get_non_membership_proofs(&non_member_hashes);
    //     assert!(result.is_ok());

    //     let proofs = result.unwrap();
    //     for (proof, root) in proofs {
    //         assert!(root.is_none());
    //     }
    // }

    // #[test]
    // fn test_insert_and_get_proof_of_existing_item() {
    //     let mut tree = IndexedMerkleTree::new();
    //     let tree_nullifier = create_nullifier_input([1u8; 32], 1, 1, 1);

    //     tree.insert_item(tree_nullifier.clone()).unwrap();

    //     let proof_result = tree.get_non_membership_proof([1u8; 32]);
    //     assert!(proof_result.is_err());
    // }

    // #[test]
    // fn test_insert_and_get_proofs_of_existing_items() {
    //     let mut tree = IndexedMerkleTree::new();
    //     let tree_nullifiers = vec![
    //         create_nullifier_input([1u8; 32], 1, 1, 1),
    //         create_nullifier_input([2u8; 32], 2, 1, 1),
    //     ];

    //     tree.insert_items(tree_nullifiers).unwrap();

    //     let proof_result = tree.get_non_membership_proofs(&[[1u8; 32], [2u8; 32]]);
    //     assert!(proof_result.is_err());
    // }
}
