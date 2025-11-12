use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use anyhow::Result;
use common::sequencer_client::SequencerClient;
use serde::{Deserialize, Serialize};

use crate::key_management::{
    key_tree::{
        chain_index::ChainIndex, keys_private::ChildKeysPrivate, keys_public::ChildKeysPublic,
        traits::KeyNode,
    },
    secret_holders::SeedHolder,
};

pub mod chain_index;
pub mod keys_private;
pub mod keys_public;
pub mod traits;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct KeyTree<Node: KeyNode> {
    pub key_map: BTreeMap<ChainIndex, Node>,
    pub addr_map: HashMap<nssa::Address, ChainIndex>,
}

pub type KeyTreePublic = KeyTree<ChildKeysPublic>;
pub type KeyTreePrivate = KeyTree<ChildKeysPrivate>;

impl<Node: KeyNode> KeyTree<Node> {
    pub fn new(seed: &SeedHolder) -> Self {
        let seed_fit: [u8; 64] = seed.seed.clone().try_into().unwrap();

        let root_keys = Node::root(seed_fit);
        let address = root_keys.address();

        let mut key_map = BTreeMap::new();
        let mut addr_map = HashMap::new();

        key_map.insert(ChainIndex::root(), root_keys);
        addr_map.insert(address, ChainIndex::root());

        Self { key_map, addr_map }
    }

    pub fn new_from_root(root: Node) -> Self {
        let mut key_map = BTreeMap::new();
        let mut addr_map = HashMap::new();

        addr_map.insert(root.address(), ChainIndex::root());
        key_map.insert(ChainIndex::root(), root);

        Self { key_map, addr_map }
    }

    //ToDo: Add function to create a tree from list of nodes with consistency check.

    pub fn find_next_last_child_of_id(&self, parent_id: &ChainIndex) -> Option<u32> {
        if !self.key_map.contains_key(parent_id) {
            return None;
        }

        let leftmost_child = parent_id.n_th_child(u32::MIN);

        if !self.key_map.contains_key(&leftmost_child) {
            Some(0)
        } else {
            let mut right = u32::MAX - 1;
            let mut left_border = u32::MIN;
            let mut right_border = u32::MAX;

            loop {
                let rightmost_child = parent_id.n_th_child(right);

                let rightmost_ref = self.key_map.get(&rightmost_child);
                let rightmost_ref_next = self.key_map.get(&rightmost_child.next_in_line());

                match (&rightmost_ref, &rightmost_ref_next) {
                    (Some(_), Some(_)) => {
                        left_border = right;
                        right = (right + right_border) / 2;
                    }
                    (Some(_), None) => {
                        break Some(right + 1);
                    }
                    (None, None) => {
                        right_border = right;
                        right = (left_border + right) / 2;
                    }
                    (None, Some(_)) => {
                        unreachable!();
                    }
                }
            }
        }
    }

    pub fn generate_new_node(&mut self, parent_cci: ChainIndex) -> Option<nssa::Address> {
        if !self.key_map.contains_key(&parent_cci) {
            return None;
        }

        let father_keys = self.key_map.get(&parent_cci).unwrap();
        let next_child_id = self.find_next_last_child_of_id(&parent_cci).unwrap();
        let next_cci = parent_cci.n_th_child(next_child_id);

        let child_keys = father_keys.n_th_child(next_child_id);

        let address = child_keys.address();

        self.key_map.insert(next_cci.clone(), child_keys);
        self.addr_map.insert(address, next_cci);

        Some(address)
    }

    pub fn get_node(&self, addr: nssa::Address) -> Option<&Node> {
        self.addr_map
            .get(&addr)
            .and_then(|chain_id| self.key_map.get(chain_id))
    }

    pub fn get_node_mut(&mut self, addr: nssa::Address) -> Option<&mut Node> {
        self.addr_map
            .get(&addr)
            .and_then(|chain_id| self.key_map.get_mut(chain_id))
    }

    pub fn insert(&mut self, addr: nssa::Address, chain_index: ChainIndex, node: Node) {
        self.addr_map.insert(addr, chain_index.clone());
        self.key_map.insert(chain_index, node);
    }

    pub fn remove(&mut self, addr: nssa::Address) -> Option<Node> {
        let chain_index = self.addr_map.remove(&addr).unwrap();
        self.key_map.remove(&chain_index)
    }

    pub fn generate_tree_for_depth(&mut self, depth: u32) {
        let mut id_stack = vec![ChainIndex::root()];

        while let Some(curr_id) = id_stack.pop() {
            self.generate_new_node(curr_id.clone());

            let mut next_id = curr_id.n_th_child(0);

            while (next_id.depth()) < depth - 1 {
                id_stack.push(next_id.clone());
                next_id = next_id.next_in_line();
            }
        }
    }
}

impl KeyTree<ChildKeysPrivate> {
    pub fn cleanup_tree_for_depth(&mut self, depth: u32) {
        let mut id_stack = vec![ChainIndex::root()];

        while let Some(curr_id) = id_stack.pop() {
            if let Some(node) = self.key_map.get(&curr_id)
                && node.value.1 == nssa::Account::default()
                && curr_id != ChainIndex::root()
            {
                let addr = node.address();
                self.remove(addr);
            }

            let mut next_id = curr_id.n_th_child(0);

            while (next_id.depth()) < depth - 1 {
                id_stack.push(next_id.clone());
                next_id = next_id.next_in_line();
            }
        }
    }
}

impl KeyTree<ChildKeysPublic> {
    pub async fn cleanup_tree_for_depth(
        &mut self,
        depth: u32,
        client: Arc<SequencerClient>,
    ) -> Result<()> {
        let mut id_stack = vec![ChainIndex::root()];

        while let Some(curr_id) = id_stack.pop() {
            if let Some(node) = self.key_map.get(&curr_id) {
                let address = node.address();
                let node_acc = client.get_account(address.to_string()).await?.account;

                if node_acc == nssa::Account::default() && curr_id != ChainIndex::root() {
                    self.remove(address);
                }
            }

            let mut next_id = curr_id.n_th_child(0);

            while (next_id.depth()) < depth - 1 {
                id_stack.push(next_id.clone());
                next_id = next_id.next_in_line();
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use nssa::Address;

    use super::*;

    fn seed_holder_for_tests() -> SeedHolder {
        SeedHolder {
            seed: [42; 64].to_vec(),
        }
    }

    #[test]
    fn test_simple_key_tree() {
        let seed_holder = seed_holder_for_tests();

        let tree = KeyTreePublic::new(&seed_holder);

        assert!(tree.key_map.contains_key(&ChainIndex::root()));
        assert!(tree.addr_map.contains_key(&Address::new([
            46, 223, 229, 177, 59, 18, 189, 219, 153, 31, 249, 90, 112, 230, 180, 164, 80, 25, 106,
            159, 14, 238, 1, 192, 91, 8, 210, 165, 199, 41, 60, 104,
        ])));
    }

    #[test]
    fn test_small_key_tree() {
        let seed_holder = seed_holder_for_tests();

        let mut tree = KeyTreePublic::new(&seed_holder);

        let next_last_child_for_parent_id = tree
            .find_next_last_child_of_id(&ChainIndex::root())
            .unwrap();

        assert_eq!(next_last_child_for_parent_id, 0);

        tree.generate_new_node(ChainIndex::root()).unwrap();

        assert!(
            tree.key_map
                .contains_key(&ChainIndex::from_str("/0").unwrap())
        );

        let next_last_child_for_parent_id = tree
            .find_next_last_child_of_id(&ChainIndex::root())
            .unwrap();

        assert_eq!(next_last_child_for_parent_id, 1);

        tree.generate_new_node(ChainIndex::root()).unwrap();
        tree.generate_new_node(ChainIndex::root()).unwrap();
        tree.generate_new_node(ChainIndex::root()).unwrap();
        tree.generate_new_node(ChainIndex::root()).unwrap();
        tree.generate_new_node(ChainIndex::root()).unwrap();
        tree.generate_new_node(ChainIndex::root()).unwrap();

        let next_last_child_for_parent_id = tree
            .find_next_last_child_of_id(&ChainIndex::root())
            .unwrap();

        assert_eq!(next_last_child_for_parent_id, 7);
    }

    #[test]
    fn test_key_tree_can_not_make_child_keys() {
        let seed_holder = seed_holder_for_tests();

        let mut tree = KeyTreePublic::new(&seed_holder);

        let next_last_child_for_parent_id = tree
            .find_next_last_child_of_id(&ChainIndex::root())
            .unwrap();

        assert_eq!(next_last_child_for_parent_id, 0);

        tree.generate_new_node(ChainIndex::root()).unwrap();

        assert!(
            tree.key_map
                .contains_key(&ChainIndex::from_str("/0").unwrap())
        );

        let next_last_child_for_parent_id = tree
            .find_next_last_child_of_id(&ChainIndex::root())
            .unwrap();

        assert_eq!(next_last_child_for_parent_id, 1);

        let key_opt = tree.generate_new_node(ChainIndex::from_str("/3").unwrap());

        assert_eq!(key_opt, None);
    }

    #[test]
    fn test_key_tree_complex_structure() {
        let seed_holder = seed_holder_for_tests();

        let mut tree = KeyTreePublic::new(&seed_holder);

        let next_last_child_for_parent_id = tree
            .find_next_last_child_of_id(&ChainIndex::root())
            .unwrap();

        assert_eq!(next_last_child_for_parent_id, 0);

        tree.generate_new_node(ChainIndex::root()).unwrap();

        assert!(
            tree.key_map
                .contains_key(&ChainIndex::from_str("/0").unwrap())
        );

        let next_last_child_for_parent_id = tree
            .find_next_last_child_of_id(&ChainIndex::root())
            .unwrap();

        assert_eq!(next_last_child_for_parent_id, 1);

        tree.generate_new_node(ChainIndex::root()).unwrap();

        assert!(
            tree.key_map
                .contains_key(&ChainIndex::from_str("/1").unwrap())
        );

        let next_last_child_for_parent_id = tree
            .find_next_last_child_of_id(&ChainIndex::root())
            .unwrap();

        assert_eq!(next_last_child_for_parent_id, 2);

        tree.generate_new_node(ChainIndex::from_str("/0").unwrap())
            .unwrap();

        let next_last_child_for_parent_id = tree
            .find_next_last_child_of_id(&ChainIndex::from_str("/0").unwrap())
            .unwrap();

        assert_eq!(next_last_child_for_parent_id, 1);

        assert!(
            tree.key_map
                .contains_key(&ChainIndex::from_str("/0/0").unwrap())
        );

        tree.generate_new_node(ChainIndex::from_str("/0").unwrap())
            .unwrap();

        let next_last_child_for_parent_id = tree
            .find_next_last_child_of_id(&ChainIndex::from_str("/0").unwrap())
            .unwrap();

        assert_eq!(next_last_child_for_parent_id, 2);

        assert!(
            tree.key_map
                .contains_key(&ChainIndex::from_str("/0/1").unwrap())
        );

        tree.generate_new_node(ChainIndex::from_str("/0").unwrap())
            .unwrap();

        let next_last_child_for_parent_id = tree
            .find_next_last_child_of_id(&ChainIndex::from_str("/0").unwrap())
            .unwrap();

        assert_eq!(next_last_child_for_parent_id, 3);

        assert!(
            tree.key_map
                .contains_key(&ChainIndex::from_str("/0/2").unwrap())
        );

        tree.generate_new_node(ChainIndex::from_str("/0/1").unwrap())
            .unwrap();

        assert!(
            tree.key_map
                .contains_key(&ChainIndex::from_str("/0/1/0").unwrap())
        );

        let next_last_child_for_parent_id = tree
            .find_next_last_child_of_id(&ChainIndex::from_str("/0/1").unwrap())
            .unwrap();

        assert_eq!(next_last_child_for_parent_id, 1);
    }
}
