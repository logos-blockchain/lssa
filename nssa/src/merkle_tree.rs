use std::collections::{HashMap, HashSet};

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

#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
pub struct MerkleTree {
    values: Vec<Value>,
    index_map: HashMap<Value, usize>,
    node_map: HashMap<usize, Node>,
    capacity: usize,
}

impl MerkleTree {
    pub fn root(&self) -> Node {
        let root_index = self.root_index();
        *self.get_node(&root_index)
    }

    fn root_index(&self) -> usize {
        let capacity_depth = self.capacity.trailing_zeros() as usize;
        let diff = capacity_depth - self.depth();
        if diff == 0 { 0 } else { (1 << diff) - 1 }
    }

    fn depth(&self) -> usize {
        self.values.len().next_power_of_two().trailing_zeros() as usize
    }

    fn get_node(&self, index: &usize) -> &Node {
        self.node_map.get(&index).unwrap_or_else(|| {
            let index_depth = usize::BITS as usize - (index + 1).leading_zeros() as usize - 1;
            let total_levels = self.capacity.trailing_zeros() as usize;
            if total_levels >= index_depth {
                &default_values::DEFAULT_VALUES[total_levels - index_depth]
            } else {
                //TODO: implement error handling
                panic!();
            }
        })
    }

    fn set_node(&mut self, index: usize, node: Node) {
        self.node_map.insert(index, node);
    }

    pub fn with_capacity(capacity: usize) -> Self {
        let capacity = capacity.next_power_of_two();
        Self {
            values: Vec::with_capacity(capacity),
            index_map: HashMap::with_capacity(capacity),
            node_map: HashMap::with_capacity(capacity << 1),
            capacity,
        }
    }

    fn reallocate_to_double_capacity(&mut self) {
        let mut this = Self::with_capacity(self.capacity << 1);
        for value in self.values.iter() {
            this.insert(*value);
        }
        *self = this;
    }

    pub fn insert(&mut self, value: Value) -> bool {
        if self.index_map.contains_key(&value) {
            return false;
        }

        if self.capacity == self.values.len() {
            self.reallocate_to_double_capacity();
        }

        let new_index = self.values.len();
        self.values.push(value);
        self.index_map.insert(value, new_index);

        let base_length = self.capacity;
        let mut layer_node = hash_value(&value);
        let mut layer_index = new_index + base_length - 1;
        self.node_map.insert(layer_index, layer_node);

        let mut layer = 0;
        let mut top_layer = self.depth();
        while layer < top_layer {
            let is_left_child = layer_index & 1 == 1;

            let (parent_index, new_parent_node) = if is_left_child {
                let parent_index = (layer_index - 1) >> 1;
                let sibling = self.get_node(&(layer_index + 1));
                let new_parent_node = hash_two(&layer_node, sibling);
                (parent_index, new_parent_node)
            } else {
                let parent_index = (layer_index - 2) >> 1;
                let sibling = self.get_node(&(layer_index - 1));
                let new_parent_node = hash_two(sibling, &layer_node);
                (parent_index, new_parent_node)
            };

            self.set_node(parent_index, new_parent_node);

            layer += 1;
            layer_index = parent_index;
            layer_node = new_parent_node
        }

        true
    }

    pub fn new(values: Vec<Value>) -> Self {
        let mut deduplicated_values = Vec::with_capacity(values.len());
        let mut seen = HashSet::new();
        for value in values.into_iter() {
            if !seen.contains(&value) {
                deduplicated_values.push(value);
                seen.insert(value);
            }
        }
        let mut this = Self::with_capacity(deduplicated_values.len());
        for value in deduplicated_values.into_iter() {
            this.insert(value);
        }
        this
    }

    pub fn get_authentication_path_for(&self, value: &Value) -> Option<(usize, Vec<Node>)> {
        let mut result = Vec::with_capacity(self.depth());
        let value_index = self.index_map.get(value)?;
        let base_length = self.capacity;
        let mut layer_index = base_length + value_index - 1;
        let mut layer = 0;
        let top_layer = self.depth();
        while layer < top_layer {
            let is_left_child = layer_index & 1 == 1;
            let (sibling, parent_index) = if is_left_child {
                (self.get_node(&(layer_index + 1)), (layer_index - 1) >> 1)
            } else {
                (self.get_node(&(layer_index - 1)), (layer_index - 2) >> 1)
            };
            result.push(*sibling);

            layer += 1;
            layer_index = parent_index;
        }
        Some((*value_index, result))
    }
}

#[cfg(test)]
mod tests {
    use nssa_core::account::{Account, NullifierPublicKey};

    use super::*;

    #[test]
    fn test_merkle_tree_1() {
        let values = vec![[1; 32], [2; 32], [3; 32], [4; 32]];
        let tree = MerkleTree::new(values.clone());
        let expected_root = [
            72, 199, 63, 120, 33, 165, 138, 141, 42, 112, 62, 91, 57, 197, 113, 192, 170, 32, 207,
            20, 171, 205, 10, 248, 242, 185, 85, 188, 32, 41, 152, 222,
        ];

        assert_eq!(tree.root(), expected_root);
        assert_eq!(tree.values, values);
        assert_eq!(*tree.index_map.get(&[1; 32]).unwrap(), 0);
        assert_eq!(*tree.index_map.get(&[2; 32]).unwrap(), 1);
        assert_eq!(*tree.index_map.get(&[3; 32]).unwrap(), 2);
        assert_eq!(*tree.index_map.get(&[4; 32]).unwrap(), 3);
        assert_eq!(tree.capacity, 4);
    }

    #[test]
    fn test_merkle_tree_2() {
        let values = vec![[1; 32], [2; 32], [3; 32], [0; 32]];
        let tree = MerkleTree::new(values.clone());
        let expected_root = [
            201, 187, 184, 48, 150, 223, 133, 21, 122, 20, 110, 125, 119, 4, 85, 169, 132, 18, 222,
            224, 99, 49, 135, 238, 134, 254, 230, 200, 164, 91, 131, 26,
        ];

        assert_eq!(tree.root(), expected_root);
        assert_eq!(tree.values, values);
        assert_eq!(*tree.index_map.get(&[1; 32]).unwrap(), 0);
        assert_eq!(*tree.index_map.get(&[2; 32]).unwrap(), 1);
        assert_eq!(*tree.index_map.get(&[3; 32]).unwrap(), 2);
        assert_eq!(*tree.index_map.get(&[0; 32]).unwrap(), 3);
        assert_eq!(tree.capacity, 4);
    }

    #[test]
    fn test_merkle_tree_3() {
        let values = vec![[1; 32], [2; 32], [3; 32]];
        let tree = MerkleTree::new(values.clone());
        let expected_root = [
            200, 211, 216, 210, 177, 63, 39, 206, 236, 205, 198, 153, 17, 152, 113, 249, 243, 46,
            167, 237, 134, 255, 69, 208, 173, 17, 247, 123, 40, 205, 117, 104,
        ];

        assert_eq!(tree.root(), expected_root);
        assert_eq!(tree.values, values);
        assert_eq!(*tree.index_map.get(&[1; 32]).unwrap(), 0);
        assert_eq!(*tree.index_map.get(&[2; 32]).unwrap(), 1);
        assert_eq!(*tree.index_map.get(&[3; 32]).unwrap(), 2);
        assert!(tree.index_map.get(&[0; 32]).is_none());
        assert_eq!(tree.capacity, 4);
    }

    #[test]
    fn test_merkle_tree_4() {
        let values = vec![[11; 32], [12; 32], [13; 32], [14; 32], [15; 32]];
        let tree = MerkleTree::new(values.clone());
        let expected_root = [
            239, 65, 138, 237, 90, 162, 7, 2, 212, 217, 76, 146, 218, 121, 164, 1, 47, 46, 54, 241,
            0, 139, 253, 179, 205, 30, 56, 116, 157, 202, 36, 153,
        ];

        assert_eq!(tree.root(), expected_root);
        assert_eq!(tree.values, values);
        assert_eq!(*tree.index_map.get(&[11; 32]).unwrap(), 0);
        assert_eq!(*tree.index_map.get(&[12; 32]).unwrap(), 1);
        assert_eq!(*tree.index_map.get(&[13; 32]).unwrap(), 2);
        assert_eq!(*tree.index_map.get(&[14; 32]).unwrap(), 3);
        assert_eq!(*tree.index_map.get(&[15; 32]).unwrap(), 4);
        assert_eq!(tree.capacity, 8);
    }

    #[test]
    fn test_merkle_tree_5() {
        let values = vec![
            [11; 32], [12; 32], [12; 32], [13; 32], [14; 32], [15; 32], [15; 32], [13; 32],
            [13; 32], [15; 32], [11; 32],
        ];
        let tree = MerkleTree::new(values);
        let expected_root = [
            239, 65, 138, 237, 90, 162, 7, 2, 212, 217, 76, 146, 218, 121, 164, 1, 47, 46, 54, 241,
            0, 139, 253, 179, 205, 30, 56, 116, 157, 202, 36, 153,
        ];

        assert_eq!(tree.root(), expected_root);
        assert_eq!(
            tree.values,
            vec![[11; 32], [12; 32], [13; 32], [14; 32], [15; 32]]
        );
        assert_eq!(*tree.index_map.get(&[11; 32]).unwrap(), 0);
        assert_eq!(*tree.index_map.get(&[12; 32]).unwrap(), 1);
        assert_eq!(*tree.index_map.get(&[13; 32]).unwrap(), 2);
        assert_eq!(*tree.index_map.get(&[14; 32]).unwrap(), 3);
        assert_eq!(*tree.index_map.get(&[15; 32]).unwrap(), 4);
        assert_eq!(tree.capacity, 8);
    }

    #[test]
    fn test_merkle_tree_6() {
        let values = vec![[1; 32], [2; 32], [3; 32], [4; 32], [5; 32]];
        let tree = MerkleTree::new(values);
        let expected_root = [
            6, 156, 184, 37, 154, 6, 254, 110, 219, 63, 167, 255, 121, 51, 166, 221, 125, 202, 111,
            202, 41, 147, 20, 55, 151, 148, 166, 136, 146, 108, 55, 146,
        ];

        assert_eq!(tree.root(), expected_root);
    }

    #[test]
    fn test_with_capacity_4() {
        let tree = MerkleTree::with_capacity(4);

        assert!(tree.index_map.is_empty());
        assert!(tree.node_map.is_empty());
        for i in 3..7 {
            assert_eq!(*tree.get_node(&i), default_values::DEFAULT_VALUES[0], "{i}");
        }
        for i in 1..3 {
            assert_eq!(*tree.get_node(&i), default_values::DEFAULT_VALUES[1], "{i}");
        }
        assert_eq!(*tree.get_node(&0), default_values::DEFAULT_VALUES[2]);
    }

    #[test]
    fn test_with_capacity_5() {
        let tree = MerkleTree::with_capacity(5);

        assert!(tree.index_map.is_empty());
        assert!(tree.node_map.is_empty());
        for i in 7..15 {
            assert_eq!(*tree.get_node(&i), default_values::DEFAULT_VALUES[0])
        }
        for i in 3..7 {
            assert_eq!(*tree.get_node(&i), default_values::DEFAULT_VALUES[1])
        }
        for i in 1..3 {
            assert_eq!(*tree.get_node(&i), default_values::DEFAULT_VALUES[2])
        }
        assert_eq!(*tree.get_node(&0), default_values::DEFAULT_VALUES[3])
    }

    #[test]
    fn test_with_capacity_6() {
        let mut tree = MerkleTree::with_capacity(100);

        let values = vec![[1; 32], [2; 32], [3; 32], [4; 32]];

        let expected_root = [
            72, 199, 63, 120, 33, 165, 138, 141, 42, 112, 62, 91, 57, 197, 113, 192, 170, 32, 207,
            20, 171, 205, 10, 248, 242, 185, 85, 188, 32, 41, 152, 222,
        ];

        tree.insert(values[0]);
        tree.insert(values[1]);
        tree.insert(values[2]);
        tree.insert(values[3]);

        assert_eq!(tree.root(), expected_root);
    }

    #[test]
    fn test_with_capacity_7() {
        let mut tree = MerkleTree::with_capacity(599);

        let values = vec![[1; 32], [2; 32], [3; 32]];

        let expected_root = [
            200, 211, 216, 210, 177, 63, 39, 206, 236, 205, 198, 153, 17, 152, 113, 249, 243, 46,
            167, 237, 134, 255, 69, 208, 173, 17, 247, 123, 40, 205, 117, 104,
        ];

        tree.insert(values[0]);
        tree.insert(values[1]);
        tree.insert(values[2]);

        assert_eq!(tree.root(), expected_root);
    }

    #[test]
    fn test_insert_value_1() {
        let mut tree = MerkleTree::with_capacity(1);

        let values = vec![[1; 32], [2; 32], [3; 32]];
        let expected_tree = MerkleTree::new(values.clone());

        tree.insert(values[0]);
        tree.insert(values[1]);
        tree.insert(values[2]);

        assert_eq!(expected_tree, tree);
    }

    #[test]
    fn test_insert_value_2() {
        let mut tree = MerkleTree::with_capacity(1);

        let values = vec![[1; 32], [2; 32], [3; 32], [4; 32]];
        let expected_tree = MerkleTree::new(values.clone());

        tree.insert(values[0]);
        tree.insert(values[1]);
        tree.insert(values[2]);
        tree.insert(values[3]);

        assert_eq!(expected_tree, tree);
    }

    #[test]
    fn test_insert_value_3() {
        let mut tree = MerkleTree::with_capacity(1);

        let values = vec![[11; 32], [12; 32], [13; 32], [14; 32], [15; 32]];
        let expected_tree = MerkleTree::new(values.clone());

        tree.insert(values[0]);
        tree.insert(values[1]);
        tree.insert(values[2]);
        tree.insert(values[3]);
        tree.insert(values[4]);

        assert_eq!(expected_tree, tree);
    }

    #[test]
    fn test_insert_value_4() {
        let mut tree = MerkleTree::with_capacity(1);

        let values = vec![[11; 32], [12; 32], [13; 32], [14; 32], [15; 32]];
        let expected_tree = MerkleTree::new(values.clone());

        tree.insert(values[0]);
        tree.insert(values[0]);
        tree.insert(values[1]);
        tree.insert(values[1]);
        tree.insert(values[2]);
        tree.insert(values[3]);
        tree.insert(values[2]);
        tree.insert(values[0]);
        tree.insert(values[4]);
        tree.insert(values[2]);
        tree.insert(values[4]);

        assert_eq!(expected_tree, tree);
    }

    #[test]
    fn test_authentication_path_1() {
        let values = vec![[1; 32], [2; 32], [3; 32], [4; 32]];
        let tree = MerkleTree::new(values);
        let expected_authentication_path = (
            2,
            vec![
                [
                    159, 79, 182, 143, 62, 29, 172, 130, 32, 47, 154, 165, 129, 206, 11, 191, 31,
                    118, 93, 240, 233, 172, 60, 140, 87, 226, 15, 104, 90, 186, 184, 237,
                ],
                [
                    80, 162, 125, 71, 70, 243, 87, 203, 112, 12, 190, 157, 72, 131, 183, 127, 182,
                    79, 1, 40, 130, 138, 52, 137, 220, 106, 111, 33, 221, 191, 36, 20,
                ],
            ],
        );

        let authentication_path = tree.get_authentication_path_for(&[3; 32]).unwrap();
        assert_eq!(authentication_path, expected_authentication_path);
    }

    #[test]
    fn test_authentication_path_2() {
        let values = vec![[1; 32], [2; 32], [3; 32]];
        let tree = MerkleTree::new(values);
        let expected_authentication_path = (
            0,
            vec![
                [
                    117, 135, 123, 180, 29, 57, 59, 95, 184, 69, 92, 230, 14, 205, 141, 218, 0, 29,
                    6, 49, 100, 150, 177, 77, 250, 127, 137, 86, 86, 238, 202, 74,
                ],
                [
                    164, 27, 133, 93, 45, 180, 222, 144, 82, 205, 123, 229, 236, 103, 214, 88, 102,
                    41, 203, 159, 110, 50, 70, 164, 175, 165, 186, 49, 63, 7, 169, 197,
                ],
            ],
        );

        let authentication_path = tree.get_authentication_path_for(&[1; 32]).unwrap();
        assert_eq!(authentication_path, expected_authentication_path);
    }

    #[test]
    fn test_authentication_path_3() {
        let values = vec![[1; 32], [2; 32], [3; 32], [4; 32], [5; 32]];
        let tree = MerkleTree::new(values);
        let expected_authentication_path = (
            4,
            vec![
                [
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0,
                ],
                [
                    245, 165, 253, 66, 209, 106, 32, 48, 39, 152, 239, 110, 211, 9, 151, 155, 67,
                    0, 61, 35, 32, 217, 240, 232, 234, 152, 49, 169, 39, 89, 251, 75,
                ],
                [
                    72, 199, 63, 120, 33, 165, 138, 141, 42, 112, 62, 91, 57, 197, 113, 192, 170,
                    32, 207, 20, 171, 205, 10, 248, 242, 185, 85, 188, 32, 41, 152, 222,
                ],
            ],
        );

        let authentication_path = tree.get_authentication_path_for(&[5; 32]).unwrap();
        assert_eq!(authentication_path, expected_authentication_path);
    }
}

mod default_values {
    pub(crate) const DEFAULT_VALUES: [[u8; 32]; 32] = [
        [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0,
        ],
        [
            245, 165, 253, 66, 209, 106, 32, 48, 39, 152, 239, 110, 211, 9, 151, 155, 67, 0, 61,
            35, 32, 217, 240, 232, 234, 152, 49, 169, 39, 89, 251, 75,
        ],
        [
            219, 86, 17, 78, 0, 253, 212, 193, 248, 92, 137, 43, 243, 90, 201, 168, 146, 137, 170,
            236, 177, 235, 208, 169, 108, 222, 96, 106, 116, 139, 93, 113,
        ],
        [
            199, 128, 9, 253, 240, 127, 197, 106, 17, 241, 34, 55, 6, 88, 163, 83, 170, 165, 66,
            237, 99, 228, 76, 75, 193, 95, 244, 205, 16, 90, 179, 60,
        ],
        [
            83, 109, 152, 131, 127, 45, 209, 101, 165, 93, 94, 234, 233, 20, 133, 149, 68, 114,
            213, 111, 36, 109, 242, 86, 191, 60, 174, 25, 53, 42, 18, 60,
        ],
        [
            158, 253, 224, 82, 170, 21, 66, 159, 174, 5, 186, 212, 208, 177, 215, 198, 77, 166, 77,
            3, 215, 161, 133, 74, 88, 140, 44, 184, 67, 12, 13, 48,
        ],
        [
            216, 141, 223, 238, 212, 0, 168, 117, 85, 150, 178, 25, 66, 193, 73, 126, 17, 76, 48,
            46, 97, 24, 41, 15, 145, 230, 119, 41, 118, 4, 31, 161,
        ],
        [
            135, 235, 13, 219, 165, 126, 53, 246, 210, 134, 103, 56, 2, 164, 175, 89, 117, 226, 37,
            6, 199, 207, 76, 100, 187, 107, 229, 238, 17, 82, 127, 44,
        ],
        [
            38, 132, 100, 118, 253, 95, 197, 74, 93, 67, 56, 81, 103, 201, 81, 68, 242, 100, 63,
            83, 60, 200, 91, 185, 209, 107, 120, 47, 141, 125, 177, 147,
        ],
        [
            80, 109, 134, 88, 45, 37, 36, 5, 184, 64, 1, 135, 146, 202, 210, 191, 18, 89, 241, 239,
            90, 165, 248, 135, 225, 60, 178, 240, 9, 79, 81, 225,
        ],
        [
            255, 255, 10, 215, 230, 89, 119, 47, 149, 52, 193, 149, 200, 21, 239, 196, 1, 78, 241,
            225, 218, 237, 68, 4, 192, 99, 133, 209, 17, 146, 233, 43,
        ],
        [
            108, 240, 65, 39, 219, 5, 68, 28, 216, 51, 16, 122, 82, 190, 133, 40, 104, 137, 14, 67,
            23, 230, 160, 42, 180, 118, 131, 170, 117, 150, 66, 32,
        ],
        [
            183, 208, 95, 135, 95, 20, 0, 39, 239, 81, 24, 162, 36, 123, 187, 132, 206, 143, 47,
            15, 17, 35, 98, 48, 133, 218, 247, 150, 12, 50, 159, 95,
        ],
        [
            223, 106, 245, 245, 187, 219, 107, 233, 239, 138, 166, 24, 228, 191, 128, 115, 150, 8,
            103, 23, 30, 41, 103, 111, 139, 40, 77, 234, 106, 8, 168, 94,
        ],
        [
            181, 141, 144, 15, 94, 24, 46, 60, 80, 239, 116, 150, 158, 161, 108, 119, 38, 197, 73,
            117, 124, 194, 53, 35, 195, 105, 88, 125, 167, 41, 55, 132,
        ],
        [
            212, 154, 117, 2, 255, 207, 176, 52, 11, 29, 120, 133, 104, 133, 0, 202, 48, 129, 97,
            167, 249, 107, 98, 223, 157, 8, 59, 113, 252, 200, 242, 187,
        ],
        [
            143, 230, 177, 104, 146, 86, 192, 211, 133, 244, 47, 91, 190, 32, 39, 162, 44, 25, 150,
            225, 16, 186, 151, 193, 113, 211, 229, 148, 141, 233, 43, 235,
        ],
        [
            141, 13, 99, 195, 158, 186, 222, 133, 9, 224, 174, 60, 156, 56, 118, 251, 95, 161, 18,
            190, 24, 249, 5, 236, 172, 254, 203, 146, 5, 118, 3, 171,
        ],
        [
            149, 238, 200, 178, 229, 65, 202, 212, 233, 29, 227, 131, 133, 242, 224, 70, 97, 159,
            84, 73, 108, 35, 130, 203, 108, 172, 213, 185, 140, 38, 245, 164,
        ],
        [
            248, 147, 233, 8, 145, 119, 117, 182, 43, 255, 35, 41, 77, 187, 227, 161, 205, 142,
            108, 193, 195, 91, 72, 1, 136, 123, 100, 106, 111, 129, 241, 127,
        ],
        [
            205, 219, 167, 181, 146, 227, 19, 51, 147, 193, 97, 148, 250, 199, 67, 26, 191, 47, 84,
            133, 237, 113, 29, 178, 130, 24, 60, 129, 158, 8, 235, 170,
        ],
        [
            138, 141, 127, 227, 175, 140, 170, 8, 90, 118, 57, 168, 50, 0, 20, 87, 223, 185, 18,
            138, 128, 97, 20, 42, 208, 51, 86, 41, 255, 35, 255, 156,
        ],
        [
            254, 179, 195, 55, 215, 165, 26, 111, 191, 0, 185, 227, 76, 82, 225, 201, 25, 92, 150,
            155, 212, 231, 160, 191, 213, 29, 92, 91, 237, 156, 17, 103,
        ],
        [
            231, 31, 10, 168, 60, 195, 46, 223, 190, 250, 159, 77, 62, 1, 116, 202, 133, 24, 46,
            236, 159, 58, 9, 246, 166, 192, 223, 99, 119, 165, 16, 215,
        ],
        [
            49, 32, 111, 168, 10, 80, 187, 106, 190, 41, 8, 80, 88, 241, 98, 18, 33, 42, 96, 238,
            200, 240, 73, 254, 203, 146, 216, 200, 224, 168, 75, 192,
        ],
        [
            33, 53, 43, 254, 203, 237, 221, 233, 147, 131, 159, 97, 76, 61, 172, 10, 62, 227, 117,
            67, 249, 180, 18, 177, 97, 153, 220, 21, 142, 35, 181, 68,
        ],
        [
            97, 158, 49, 39, 36, 187, 109, 124, 49, 83, 237, 157, 231, 145, 215, 100, 163, 102,
            179, 137, 175, 19, 197, 139, 248, 168, 217, 4, 129, 164, 103, 101,
        ],
        [
            124, 221, 41, 134, 38, 130, 80, 98, 141, 12, 16, 227, 133, 197, 140, 97, 145, 230, 251,
            224, 81, 145, 188, 192, 79, 19, 63, 44, 234, 114, 193, 196,
        ],
        [
            132, 137, 48, 189, 123, 168, 202, 197, 70, 97, 7, 33, 19, 251, 39, 136, 105, 224, 123,
            184, 88, 127, 145, 57, 41, 51, 55, 77, 1, 123, 203, 225,
        ],
        [
            136, 105, 255, 44, 34, 178, 140, 193, 5, 16, 217, 133, 50, 146, 128, 51, 40, 190, 79,
            176, 232, 4, 149, 232, 187, 141, 39, 31, 91, 136, 150, 54,
        ],
        [
            181, 254, 40, 231, 159, 27, 133, 15, 134, 88, 36, 108, 233, 182, 161, 231, 180, 159,
            192, 109, 183, 20, 62, 143, 224, 180, 242, 176, 197, 82, 58, 92,
        ],
        [
            152, 94, 146, 159, 112, 175, 40, 208, 189, 209, 169, 10, 128, 143, 151, 127, 89, 124,
            124, 119, 140, 72, 158, 152, 211, 189, 137, 16, 211, 26, 192, 247,
        ],
    ];
}

//
