use std::{collections::HashMap, fmt, marker::PhantomData};

use rs_merkle::{MerkleProof, MerkleTree};
use serde::{
    de::{SeqAccess, Visitor},
    ser::SerializeSeq,
    Deserialize, Deserializer, Serialize,
};

use crate::{transaction::Transaction, utxo_commitment::UTXOCommitment};

use super::{hasher::OwnHasher, tree_leav_item::TreeLeavItem, TreeHashType};

#[derive(Clone)]
pub struct HashStorageMerkleTree<Leav: TreeLeavItem + Clone> {
    leaves: HashMap<usize, Leav>,
    hash_to_id_map: HashMap<TreeHashType, usize>,
    tree: MerkleTree<OwnHasher>,
}

impl<Leav: TreeLeavItem + Clone + Serialize> Serialize for HashStorageMerkleTree<Leav> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut vector = self.leaves.iter().collect::<Vec<_>>();
        vector.sort_by(|a, b| a.0.cmp(b.0));

        let mut seq = serializer.serialize_seq(Some(self.leaves.len()))?;
        for element in vector.iter() {
            seq.serialize_element(element.1)?;
        }
        seq.end()
    }
}

struct HashStorageMerkleTreeDeserializer<Leav: TreeLeavItem + Clone> {
    marker: PhantomData<fn() -> HashStorageMerkleTree<Leav>>,
}

impl<Leaf: TreeLeavItem + Clone> HashStorageMerkleTreeDeserializer<Leaf> {
    fn new() -> Self {
        HashStorageMerkleTreeDeserializer {
            marker: PhantomData,
        }
    }
}

impl<'de, Leav: TreeLeavItem + Clone + Deserialize<'de>> Visitor<'de>
    for HashStorageMerkleTreeDeserializer<Leav>
{
    type Value = HashStorageMerkleTree<Leav>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("HashStorageMerkleTree key value sequence.")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut vector = vec![];

        loop {
            let opt_key = seq.next_element::<Leav>()?;
            if let Some(value) = opt_key {
                vector.push(value);
            } else {
                break;
            }
        }

        Ok(HashStorageMerkleTree::new(vector))
    }
}

impl<'de, Leav: TreeLeavItem + Clone + Deserialize<'de>> serde::Deserialize<'de>
    for HashStorageMerkleTree<Leav>
{
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_seq(HashStorageMerkleTreeDeserializer::new())
    }
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

    pub fn add_tx(&mut self, tx: &Leav) {
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
    #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    fn test_new_merkle_tree_serialize() {
        let tx1 = MockTransaction {
            hash: get_first_32_bytes("tx1"),
        };
        let tx2 = MockTransaction {
            hash: get_first_32_bytes("tx2"),
        };

        let tree = HashStorageMerkleTree::new(vec![tx1.clone(), tx2.clone()]);

        let binding = serde_json::to_vec(&tree).unwrap();

        let obj: HashStorageMerkleTree<MockTransaction> = serde_json::from_slice(&binding).unwrap();

        assert_eq!(tree.leaves, obj.leaves);
        assert_eq!(tree.hash_to_id_map, obj.hash_to_id_map);
        assert_eq!(tree.tree.root(), obj.tree.root());
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

        tree.add_tx(&tx2);
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
