use std::collections::BTreeMap;

use accounts::account_core::AccountPublicMask;
use common::merkle_tree_public::{merkle_tree::UTXOCommitmentsMerkleTree, TreeHashType};
use nssa::Address;
use serde::{ser::SerializeStruct, Serialize};

pub const PUBLIC_SC_CONTEXT: &str = "PublicSCContext";
pub const CALLER_ADDRESS: &str = "caller_address";
pub const CALLER_BALANCE: &str = "caller_balance";
pub const ACCOUNT_MASKS_KEYS_SORTED: &str = "account_masks_keys_sorted";
pub const ACCOUNT_MASKS_VALUES_SORTED: &str = "account_masks_values_sorted";
pub const COMMITMENT_STORE_ROOT: &str = "commitment_store_root";
pub const PUT_TX_STORE_ROOT: &str = "put_tx_store_root";
pub const COMMITMENT_TREE: &str = "commitments_tree";
pub const NULLIFIERS_SET: &str = "nullifiers_set";

///Strucutre, representing context, given to a smart contract on a call
pub struct PublicSCContext {
    pub caller_address: Address,
    pub caller_balance: u64,
    pub account_masks: BTreeMap<Address, AccountPublicMask>,
    pub comitment_store_root: TreeHashType,
    pub commitments_tree: UTXOCommitmentsMerkleTree,
}

impl Serialize for PublicSCContext {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut account_masks_keys: Vec<[u8; 32]> = self
            .account_masks
            .keys()
            .cloned()
            .map(|addr| *addr.value())
            .collect();
        account_masks_keys.sort();

        let mut account_mask_values: Vec<AccountPublicMask> =
            self.account_masks.values().cloned().collect();
        account_mask_values.sort_by(|left, right| left.address.cmp(&right.address));

        let mut s = serializer.serialize_struct(PUBLIC_SC_CONTEXT, 7)?;

        s.serialize_field(CALLER_ADDRESS, &self.caller_address)?;
        s.serialize_field(CALLER_BALANCE, &self.caller_balance)?;
        s.serialize_field(ACCOUNT_MASKS_KEYS_SORTED, &account_masks_keys)?;
        s.serialize_field(ACCOUNT_MASKS_VALUES_SORTED, &account_mask_values)?;
        s.serialize_field(COMMITMENT_STORE_ROOT, &self.comitment_store_root)?;
        s.serialize_field(COMMITMENT_TREE, &self.commitments_tree)?;

        s.end()
    }
}

impl PublicSCContext {
    ///Produces `u64` from bytes in a vector
    ///
    /// Assumes, that vector of le_bytes
    pub fn produce_u64_from_fit_vec(data: Vec<u8>) -> u64 {
        let data_len = data.len();

        assert!(data_len <= 8);
        let mut le_bytes: [u8; 8] = [0; 8];

        for (idx, item) in data.into_iter().enumerate() {
            le_bytes[idx] = item
        }

        u64::from_le_bytes(le_bytes)
    }

    ///Produces vector of `u64` from context
    pub fn produce_u64_list_from_context(&self) -> Result<Vec<u64>, serde_json::Error> {
        let mut u64_list = vec![];

        let ser_data = serde_json::to_vec(self)?;

        //`ToDo` Replace with `next_chunk` usage, when feature stabilizes in Rust
        for i in 0..=(ser_data.len() / 8) {
            let next_chunk: Vec<u8> = if (i + 1) * 8 < ser_data.len() {
                ser_data[(i * 8)..((i + 1) * 8)].to_vec()
            } else {
                ser_data[(i * 8)..(ser_data.len())].to_vec()
            };

            u64_list.push(PublicSCContext::produce_u64_from_fit_vec(next_chunk));
        }

        Ok(u64_list)
    }
}

#[cfg(test)]
mod tests {
    use accounts::account_core::Account;
    use common::utxo_commitment::UTXOCommitment;

    use super::*;

    fn create_test_context() -> PublicSCContext {
        let caller_address = Address::new([1; 32]);
        let comitment_store_root = [3; 32];

        let commitments_tree =
            UTXOCommitmentsMerkleTree::new(vec![UTXOCommitment { hash: [5; 32] }]);

        let mut account_masks = BTreeMap::new();

        let acc_1 = Account::new();
        let acc_2 = Account::new();
        let acc_3 = Account::new();

        account_masks.insert(acc_1.address, acc_1.make_account_public_mask());
        account_masks.insert(acc_2.address, acc_2.make_account_public_mask());
        account_masks.insert(acc_3.address, acc_3.make_account_public_mask());

        PublicSCContext {
            caller_address,
            caller_balance: 100,
            account_masks,
            comitment_store_root,
            commitments_tree,
        }
    }

    #[test]
    fn bin_ser_stability_test() {
        let test_context = create_test_context();

        let serialization_1 = serde_json::to_vec(&test_context).unwrap();
        let serialization_2 = serde_json::to_vec(&test_context).unwrap();

        assert_eq!(serialization_1, serialization_2);
    }

    #[test]
    fn correct_u64_production_from_fit_vec() {
        let le_vec = vec![1, 1, 1, 1, 2, 1, 1, 1];

        let num = PublicSCContext::produce_u64_from_fit_vec(le_vec);

        assert_eq!(num, 72340177133043969);
    }

    #[test]
    fn correct_u64_production_from_small_vec() {
        //7 items instead of 8
        let le_vec = vec![1, 1, 1, 1, 2, 1, 1];

        let num = PublicSCContext::produce_u64_from_fit_vec(le_vec);

        assert_eq!(num, 282583095116033);
    }

    #[test]
    fn correct_u64_production_from_small_vec_le_bytes() {
        //7 items instead of 8
        let le_vec = vec![1, 1, 1, 1, 2, 1, 1];
        let le_vec_res = [1, 1, 1, 1, 2, 1, 1, 0];

        let num = PublicSCContext::produce_u64_from_fit_vec(le_vec);

        assert_eq!(num.to_le_bytes(), le_vec_res);
    }

    #[test]
    #[should_panic]
    fn correct_u64_production_from_unfit_vec_should_panic() {
        //9 items instead of 8
        let le_vec = vec![1, 1, 1, 1, 2, 1, 1, 1, 1];

        PublicSCContext::produce_u64_from_fit_vec(le_vec);
    }

    #[test]
    fn consistent_len_of_context_commitments() {
        let test_context = create_test_context();

        let context_num_vec1 = test_context.produce_u64_list_from_context().unwrap();
        let context_num_vec2 = test_context.produce_u64_list_from_context().unwrap();

        assert_eq!(context_num_vec1.len(), context_num_vec2.len());
    }
}
