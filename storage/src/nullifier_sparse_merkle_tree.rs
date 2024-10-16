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
            Err(monotree::Errors::new("Is a memeber"))
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
