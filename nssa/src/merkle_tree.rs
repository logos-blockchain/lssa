use std::collections::HashMap;

use sha2::{Digest, Sha256};

type Value = [u8; 32];
type Node = [u8; 32];

/// Compute parent as the hash of two child nodes
fn hash_two(left: &Node, right: &Node) -> Node {
    let mut hasher = Sha256::new();
    hasher.update(left);
    hasher.update(right);
    hasher.finalize().into()
}

fn hash_value(value: &Value) -> Node {
    let mut hasher = Sha256::new();
    hasher.update(value);
    hasher.finalize().into()
}

#[derive(Debug)]
pub struct MerkleTree {
    index_map: HashMap<Value, usize>,
    node_map: HashMap<usize, Node>,
    capacity: usize,
    length: usize,
}

impl MerkleTree {
    pub fn root(&self) -> Node {
        *self.node_map.get(&0).unwrap()
    }

    pub fn new(mut values: Vec<Value>) -> Self {
        Self::deduplicate_values(&mut values);

        let capacity = values.len().next_power_of_two();
        let length = values.len();

        let base_length = capacity;

        let mut node_map: HashMap<usize, Node> = values
            .iter()
            .enumerate()
            .map(|(index, value)| (index + base_length - 1, hash_value(value)))
            .collect();
        node_map.extend(
            (values.len()..base_length)
                .map(|index| (index + base_length - 1, [0; 32]))
                .collect::<HashMap<_, _>>(),
        );

        let mut current_layer_length = base_length;
        let mut current_layer_first_index = base_length - 1;

        while current_layer_length > 1 {
            let next_layer_length = current_layer_length >> 1;
            let next_layer_first_index = current_layer_first_index >> 1;

            let next_layer = (next_layer_first_index..(next_layer_first_index + next_layer_length))
                .map(|index| {
                    let left_child = node_map.get(&((index << 1) + 1)).unwrap();
                    let right_child = node_map.get(&((index << 1) + 2)).unwrap();
                    (index, hash_two(&left_child, &right_child))
                })
                .collect::<HashMap<_, _>>();

            node_map.extend(&next_layer);

            current_layer_length = next_layer_length;
            current_layer_first_index = next_layer_first_index;
        }

        let index_map = values
            .into_iter()
            .enumerate()
            .map(|(index, value)| (value, index))
            .collect();

        Self {
            index_map,
            node_map,
            capacity,
            length,
        }
    }

    fn deduplicate_values(values: &mut [Value]) {
        // TODO: implement
    }
}

#[cfg(test)]
mod tests {
    use nssa_core::account::{Account, NullifierPublicKey};

    use super::*;

    #[test]
    fn test_merkle_tree_1() {
        let values = vec![[1; 32], [2; 32], [3; 32], [4; 32]];
        let tree = MerkleTree::new(values);
        let expected_root = [
            72, 199, 63, 120, 33, 165, 138, 141, 42, 112, 62, 91, 57, 197, 113, 192, 170, 32, 207,
            20, 171, 205, 10, 248, 242, 185, 85, 188, 32, 41, 152, 222,
        ];

        assert_eq!(tree.root(), expected_root);
        assert_eq!(*tree.index_map.get(&[1; 32]).unwrap(), 0);
        assert_eq!(*tree.index_map.get(&[2; 32]).unwrap(), 1);
        assert_eq!(*tree.index_map.get(&[3; 32]).unwrap(), 2);
        assert_eq!(*tree.index_map.get(&[4; 32]).unwrap(), 3);
        assert_eq!(tree.capacity, 4);
        assert_eq!(tree.length, 4);
    }

    #[test]
    fn test_merkle_tree_2() {
        let values = vec![[1; 32], [2; 32], [3; 32], [0; 32]];
        let tree = MerkleTree::new(values);
        let expected_root = [
            201, 187, 184, 48, 150, 223, 133, 21, 122, 20, 110, 125, 119, 4, 85, 169, 132, 18, 222,
            224, 99, 49, 135, 238, 134, 254, 230, 200, 164, 91, 131, 26,
        ];

        assert_eq!(tree.root(), expected_root);
        assert_eq!(*tree.index_map.get(&[1; 32]).unwrap(), 0);
        assert_eq!(*tree.index_map.get(&[2; 32]).unwrap(), 1);
        assert_eq!(*tree.index_map.get(&[3; 32]).unwrap(), 2);
        assert_eq!(*tree.index_map.get(&[0; 32]).unwrap(), 3);
        assert_eq!(tree.capacity, 4);
        assert_eq!(tree.length, 4);
    }

    #[test]
    fn test_merkle_tree_3() {
        let values = vec![[1; 32], [2; 32], [3; 32]];
        let tree = MerkleTree::new(values);
        let expected_root = [
            200, 211, 216, 210, 177, 63, 39, 206, 236, 205, 198, 153, 17, 152, 113, 249, 243, 46,
            167, 237, 134, 255, 69, 208, 173, 17, 247, 123, 40, 205, 117, 104,
        ];

        assert_eq!(tree.root(), expected_root);
        assert_eq!(*tree.index_map.get(&[1; 32]).unwrap(), 0);
        assert_eq!(*tree.index_map.get(&[2; 32]).unwrap(), 1);
        assert_eq!(*tree.index_map.get(&[3; 32]).unwrap(), 2);
        assert!(tree.index_map.get(&[0; 32]).is_none());
        assert_eq!(tree.capacity, 4);
        assert_eq!(tree.length, 3);
    }

    #[test]
    fn test_merkle_tree_4() {
        let values = vec![[11; 32], [12; 32], [13; 32], [14; 32], [15; 32]];
        let tree = MerkleTree::new(values);
        let expected_root = [
            239, 65, 138, 237, 90, 162, 7, 2, 212, 217, 76, 146, 218, 121, 164, 1, 47, 46, 54, 241,
            0, 139, 253, 179, 205, 30, 56, 116, 157, 202, 36, 153,
        ];

        assert_eq!(tree.root(), expected_root);
        assert_eq!(*tree.index_map.get(&[11; 32]).unwrap(), 0);
        assert_eq!(*tree.index_map.get(&[12; 32]).unwrap(), 1);
        assert_eq!(*tree.index_map.get(&[13; 32]).unwrap(), 2);
        assert_eq!(*tree.index_map.get(&[14; 32]).unwrap(), 3);
        assert_eq!(*tree.index_map.get(&[15; 32]).unwrap(), 4);
        assert_eq!(tree.capacity, 8);
        assert_eq!(tree.length, 5);
    }
}

//
