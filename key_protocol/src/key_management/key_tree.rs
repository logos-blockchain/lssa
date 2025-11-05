use std::{
    collections::{BTreeMap, HashMap},
    str::FromStr,
    u32,
};

use crate::key_management::secret_holders::SeedHolder;

#[derive(Debug)]
pub struct ChildKeysPublic {
    pub csk: nssa::PrivateKey,
    pub cpk: nssa::PublicKey,
    pub ccc: [u8; 32],
    ///Can be None if root
    pub cci: Option<u32>,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct ChainIndex(Vec<u32>);

impl FromStr for ChainIndex {
    type Err = hex::FromHexError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "" {
            return Ok(Self(vec![]));
        }

        let hex_decoded = hex::decode(s)?;

        if !hex_decoded.len().is_multiple_of(4) {
            Err(hex::FromHexError::InvalidStringLength)
        } else {
            let mut res_vec = vec![];

            for i in 0..(hex_decoded.len() / 4) {
                res_vec.push(u32::from_le_bytes([
                    hex_decoded[4 * i],
                    hex_decoded[4 * i + 1],
                    hex_decoded[4 * i + 2],
                    hex_decoded[4 * i + 3],
                ]));
            }

            Ok(Self(res_vec))
        }
    }
}

impl ToString for ChainIndex {
    fn to_string(&self) -> String {
        if self.0.is_empty() {
            return "".to_string();
        }

        let mut res_vec = vec![];

        for index in &self.0 {
            res_vec.extend_from_slice(&index.to_le_bytes());
        }

        hex::encode(res_vec)
    }
}

impl ChainIndex {
    pub fn next_in_line(&self) -> ChainIndex {
        let mut chain = self.0.clone();
        //ToDo: Add overflow check
        chain.last_mut().map(|last_p| *last_p += 1);

        ChainIndex(chain)
    }

    pub fn n_th_son(&self, son_id: u32) -> ChainIndex {
        let mut chain = self.0.clone();
        chain.push(son_id);

        ChainIndex(chain)
    }
}

#[derive(Debug)]
pub struct KeyTreePublic {
    pub key_map: BTreeMap<ChainIndex, ChildKeysPublic>,
    pub addr_map: HashMap<nssa::Address, ChainIndex>,
}

impl KeyTreePublic {
    pub fn new(seed: &SeedHolder) -> Self {
        let seed_fit: [u8; 64] = seed.seed.clone().try_into().unwrap();
        let hash_value = hmac_sha512::HMAC::mac(&seed_fit, "NSSA_master_pub");

        let csk = nssa::PrivateKey::try_new(*hash_value.first_chunk::<32>().unwrap()).unwrap();
        let ccc = *hash_value.last_chunk::<32>().unwrap();
        let cpk = nssa::PublicKey::new_from_private_key(&csk);
        let address = nssa::Address::from(&cpk);

        let root_keys = ChildKeysPublic {
            csk,
            cpk,
            ccc,
            cci: None,
        };

        let mut key_map = BTreeMap::new();
        let mut addr_map = HashMap::new();

        key_map.insert(ChainIndex::from_str("").unwrap(), root_keys);
        addr_map.insert(address, ChainIndex::from_str("").unwrap());

        Self { key_map, addr_map }
    }

    pub fn find_last_son_of_id(&self, father_id: &ChainIndex) -> Option<u32> {
        if !self.key_map.contains_key(father_id) {
            return None;
        }

        let leftmost_son = father_id.n_th_son(u32::MIN);

        if !self.key_map.contains_key(&leftmost_son) {
            Some(0)
        } else {
            let mut right = u32::MAX - 1;
            let mut left_border = u32::MIN;
            let mut right_border = u32::MAX;

            loop {
                let rightmost_son = father_id.n_th_son(right);

                let rightmost_ref = self.key_map.get(&rightmost_son);
                let rightmost_ref_next = self.key_map.get(&rightmost_son.next_in_line());

                match (&rightmost_ref, &rightmost_ref_next) {
                    (Some(_), Some(_)) => {
                        left_border = right;
                        right = (right + right_border) / 2;
                    }
                    (Some(_), None) => {
                        break Some(right);
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

    pub fn generate_new_pub_keys(&mut self, father_cci: ChainIndex) -> Option<nssa::Address> {
        if !self.key_map.contains_key(&father_cci) {
            return None;
        }

        let father_keys = self.key_map.get(&father_cci).unwrap();
        let next_son_id = self.find_last_son_of_id(&father_cci).unwrap();
        let next_son_cci = father_cci.n_th_son(next_son_id);

        let mut hash_input = vec![];
        hash_input.extend_from_slice(father_keys.csk.value());
        hash_input.extend_from_slice(&next_son_id.to_le_bytes());

        let hash_value = hmac_sha512::HMAC::mac(&hash_input, father_keys.ccc);

        let csk = nssa::PrivateKey::try_new(*hash_value.first_chunk::<32>().unwrap()).unwrap();
        let ccc = *hash_value.last_chunk::<32>().unwrap();
        let cpk = nssa::PublicKey::new_from_private_key(&csk);
        let address = nssa::Address::from(&cpk);

        let child_keys = ChildKeysPublic {
            csk,
            cpk,
            ccc,
            cci: None,
        };

        self.key_map.insert(next_son_cci.clone(), child_keys);
        self.addr_map.insert(address, next_son_cci);

        Some(address)
    }

    pub fn get_pub_keys(&self, addr: nssa::Address) -> Option<&ChildKeysPublic> {
        self.addr_map
            .get(&addr)
            .map(|chain_id| self.key_map.get(chain_id))
            .flatten()
    }

    pub fn topology_hexdump(&self) -> String {
        let mut hex_dump = String::new();

        //Very inefficient
        for chain_id in self.key_map.keys() {
            hex_dump = format!("{hex_dump}{}", chain_id.to_string());
        }

        hex_dump
    }
}
