use monotree::database::MemoryDB;
use monotree::hasher::Blake3;
use monotree::{Hasher, Monotree, Proof};

use crate::commitment::Commitment;
use crate::merkle_tree_public::CommitmentHashType;
use crate::nullifier::UTXONullifier;

pub struct CommitmentsSparseMerkleTree {
    pub curr_root: Option<CommitmentHashType>,
    pub tree: Monotree<MemoryDB, Blake3>,
    pub hasher: Blake3,
}

impl CommitmentsSparseMerkleTree {
    pub fn new() -> Self {
        CommitmentsSparseMerkleTree {
            curr_root: None,
            tree: Monotree::default(),
            hasher: Blake3::new(),
        }
    }

    pub fn insert_item(&mut self, commitment: Commitment) -> Result<(), monotree::Errors> {
        let root = self
            .curr_root
            .as_ref()
            .map(|val| val[0..32].try_into().unwrap());

        let new_root = self.tree.insert(
            root,
            &commitment.commitment_hash[0..32].try_into().unwrap(),
            &commitment.commitment_hash[0..32].try_into().unwrap(),
        )?;

        self.curr_root = new_root.map(|val| val.to_vec());

        Ok(())
    }

    pub fn insert_items(&mut self, commitments: Vec<Commitment>) -> Result<(), monotree::Errors> {
        let root = self
            .curr_root
            .as_ref()
            .map(|val| val[0..32].try_into().unwrap());

        let hashes: Vec<_> = commitments
            .iter()
            .map(|val| val.commitment_hash[0..32].try_into().unwrap())
            .collect::<Vec<_>>();

        let new_root = self.tree.inserts(root, &hashes, &hashes)?;

        self.curr_root = new_root.map(|val| val[0..32].try_into().unwrap());

        Ok(())
    }

    pub fn search_item_inclusion(
        &mut self,
        commitment_hash: CommitmentHashType,
    ) -> Result<bool, monotree::Errors> {
        self.tree
            .get(
                self.curr_root
                    .as_ref()
                    .map(|val| val[0..32].try_into().unwrap()),
                &commitment_hash[0..32].try_into().unwrap(),
            )
            .map(|data| data.is_some())
    }

    pub fn search_item_inclusions(
        &mut self,
        commitment_hashes: &[CommitmentHashType],
    ) -> Result<Vec<bool>, monotree::Errors> {
        let mut inclusions = vec![];

        for nullifier_hash in commitment_hashes {
            let is_included = self
                .tree
                .get(
                    self.curr_root
                        .as_ref()
                        .map(|val| val[0..32].try_into().unwrap()),
                    nullifier_hash[0..32].try_into().unwrap(),
                )
                .map(|data| data.is_some())?;

            inclusions.push(is_included);
        }

        Ok(inclusions)
    }

    pub fn get_non_membership_proof(
        &mut self,
        commitment_hash: CommitmentHashType,
    ) -> Result<(Option<Proof>, Option<CommitmentHashType>), monotree::Errors> {
        let is_member = self.search_item_inclusion(commitment_hash.clone())?;

        if is_member {
            Err(monotree::Errors::new("Is a member"))
        } else {
            Ok((
                self.tree.get_merkle_proof(
                    self.curr_root
                        .as_ref()
                        .map(|val| val[0..32].try_into().unwrap()),
                    &commitment_hash,
                )?,
                self.curr_root.clone(),
            ))
        }
    }

    #[allow(clippy::type_complexity)]
    pub fn get_non_membership_proofs(
        &mut self,
        commitment_hashes: &[CommitmentHashType],
    ) -> Result<Vec<(Option<Proof>, Option<CommitmentHashType>)>, monotree::Errors> {
        let mut non_membership_proofs = vec![];

        for commitment_hash in commitment_hashes {
            let is_member = self.search_item_inclusion(commitment_hash.clone())?;

            if is_member {
                return Err(monotree::Errors::new(
                    format!("{commitment_hash:?} Is a member").as_str(),
                ));
            } else {
                non_membership_proofs.push((
                    self.tree.get_merkle_proof(
                        self.curr_root
                            .as_ref()
                            .map(|val| val[0..32].try_into().unwrap()),
                        commitment_hash,
                    )?,
                    self.curr_root.clone(),
                ))
            };
        }

        Ok(non_membership_proofs)
    }
}

impl Default for CommitmentsSparseMerkleTree {
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

    fn create_nullifier(hash: CommitmentHashType) -> Commitment {
        Commitment {
            commitment_hash: hash,
        }
    }

    #[test]
    fn test_new_tree_initialization() {
        let tree = CommitmentsSparseMerkleTree::new();
        assert!(tree.curr_root.is_none());
    }

    #[test]
    fn test_insert_single_item() {
        let mut tree = CommitmentsSparseMerkleTree::new();
        let nullifier = create_nullifier([1u8; 32].to_vec()); // Sample 32-byte hash

        let result = tree.insert_item(nullifier);
        assert!(result.is_ok());
        assert!(tree.curr_root.is_some());
    }

    #[test]
    fn test_insert_multiple_items() {
        let mut tree = CommitmentsSparseMerkleTree::new();
        let nullifiers = vec![
            create_nullifier([1u8; 32].to_vec()),
            create_nullifier([2u8; 32].to_vec()),
            create_nullifier([3u8; 32].to_vec()),
        ];

        let result = tree.insert_items(nullifiers);
        assert!(result.is_ok());
        assert!(tree.curr_root.is_some());
    }

    #[test]
    fn test_search_item_inclusion() {
        let mut tree = CommitmentsSparseMerkleTree::new();
        let nullifier = create_nullifier([1u8; 32].to_vec());

        tree.insert_item(nullifier.clone()).unwrap();

        let result = tree.search_item_inclusion([1u8; 32].to_vec());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), true);

        let non_existing = tree.search_item_inclusion([99u8; 32].to_vec());
        assert!(non_existing.is_ok());
        assert_eq!(non_existing.unwrap(), false);
    }

    #[test]
    fn test_search_multiple_item_inclusions() {
        let mut tree = CommitmentsSparseMerkleTree::new();
        let nullifiers = vec![
            create_nullifier([1u8; 32].to_vec()),
            create_nullifier([2u8; 32].to_vec()),
            create_nullifier([3u8; 32].to_vec()),
        ];

        tree.insert_items(nullifiers).unwrap();

        let search_hashes = vec![[1u8; 32].to_vec(), [2u8; 32].to_vec(), [99u8; 32].to_vec()];
        let result = tree.search_item_inclusions(&search_hashes);
        assert!(result.is_ok());

        let expected_results = vec![true, true, false];
        assert_eq!(result.unwrap(), expected_results);
    }

    #[test]
    fn test_non_membership_proof() {
        let mut tree = CommitmentsSparseMerkleTree::new();
        let non_member_hash = [5u8; 32].to_vec();

        let result = tree.get_non_membership_proof(non_member_hash);
        assert!(result.is_ok());

        let (proof, root) = result.unwrap();
        assert!(root.is_none());
    }

    #[test]
    fn test_non_membership_proofs_multiple() {
        let mut tree = CommitmentsSparseMerkleTree::new();
        let non_member_hashes = vec![[5u8; 32].to_vec(), [6u8; 32].to_vec(), [7u8; 32].to_vec()];

        let result = tree.get_non_membership_proofs(&non_member_hashes);
        assert!(result.is_ok());

        let proofs = result.unwrap();
        for (proof, root) in proofs {
            assert!(root.is_none());
        }
    }

    #[test]
    fn test_insert_and_get_proof_of_existing_item() {
        let mut tree = CommitmentsSparseMerkleTree::new();
        let nullifier = create_nullifier([1u8; 32].to_vec());

        tree.insert_item(nullifier.clone()).unwrap();

        let proof_result = tree.get_non_membership_proof([1u8; 32].to_vec());
        assert!(proof_result.is_err());
    }

    #[test]
    fn test_insert_and_get_proofs_of_existing_items() {
        let mut tree = CommitmentsSparseMerkleTree::new();
        let nullifiers = vec![
            create_nullifier([1u8; 32].to_vec()),
            create_nullifier([2u8; 32].to_vec()),
        ];

        tree.insert_items(nullifiers).unwrap();

        let proof_result =
            tree.get_non_membership_proofs(&[[1u8; 32].to_vec(), [2u8; 32].to_vec()]);
        assert!(proof_result.is_err());
    }
}
