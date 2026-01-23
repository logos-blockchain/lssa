//! This crate contains core data structures and utilities for the AMM Program.

use nssa_core::{
    account::{AccountId, Data},
    program::{PdaSeed, ProgramId},
};
use serde::{Deserialize, Serialize};

/// AMM Program Instruction.
#[derive(Serialize, Deserialize)]
pub enum Instruction {
    /// Initializes a new Pool (or re-initializes an inactive Pool).
    ///
    /// Required accounts:
    /// - AMM Pool
    /// - Vault Holding Account for Token A
    /// - Vault Holding Account for Token B
    /// - Pool Liquidity Token Definition
    /// - User Holding Account for Token A (authorized)
    /// - User Holding Account for Token B (authorized)
    /// - User Holding Account for Pool Liquidity
    NewDefinition {
        token_a_amount: u128,
        token_b_amount: u128,
        amm_program_id: ProgramId,
    },

    /// Adds liquidity to the Pool
    ///
    /// Required accounts:
    /// - AMM Pool (initialized)
    /// - Vault Holding Account for Token A (initialized)
    /// - Vault Holding Account for Token B (initialized)
    /// - Pool Liquidity Token Definition (initialized)
    /// - User Holding Account for Token A (authorized)
    /// - User Holding Account for Token B (authorized)
    /// - User Holding Account for Pool Liquidity
    AddLiquidity {
        min_amount_liquidity: u128,
        max_amount_to_add_token_a: u128,
        max_amount_to_add_token_b: u128,
    },

    /// Removes liquidity from the Pool
    ///
    /// Required accounts:
    /// - AMM Pool (initialized)
    /// - Vault Holding Account for Token A (initialized)
    /// - Vault Holding Account for Token B (initialized)
    /// - Pool Liquidity Token Definition (initialized)
    /// - User Holding Account for Token A (initialized)
    /// - User Holding Account for Token B (initialized)
    /// - User Holding Account for Pool Liquidity (authorized)
    RemoveLiquidity {
        remove_liquidity_amount: u128,
        min_amount_to_remove_token_a: u128,
        min_amount_to_remove_token_b: u128,
    },

    /// Swap some quantity of Tokens (either Token A or Token B)
    /// while maintaining the Pool constant product.
    ///
    /// Required accounts:
    /// - AMM Pool (initialized)
    /// - Vault Holding Account for Token A (initialized)
    /// - Vault Holding Account for Token B (initialized)
    /// - User Holding Account for Token A
    /// - User Holding Account for Token B Either User Holding Account for Token A or Token B is
    ///   authorized.
    Swap {
        swap_amount_in: u128,
        min_amount_out: u128,
        token_definition_id_in: AccountId,
    },
}

const POOL_DEFINITION_DATA_SIZE: usize = 225;

#[derive(Clone, Default)]
pub struct PoolDefinition {
    pub definition_token_a_id: AccountId,
    pub definition_token_b_id: AccountId,
    pub vault_a_id: AccountId,
    pub vault_b_id: AccountId,
    pub liquidity_pool_id: AccountId,
    pub liquidity_pool_supply: u128,
    pub reserve_a: u128,
    pub reserve_b: u128,
    /// Fees are currently not used
    pub fees: u128,
    /// A pool becomes inactive (active = false)
    /// once all of its liquidity has been removed (e.g., reserves are emptied and
    /// liquidity_pool_supply = 0)
    pub active: bool,
}

impl PoolDefinition {
    pub fn into_data(self) -> Data {
        let mut bytes = [0; POOL_DEFINITION_DATA_SIZE];
        bytes[0..32].copy_from_slice(&self.definition_token_a_id.to_bytes());
        bytes[32..64].copy_from_slice(&self.definition_token_b_id.to_bytes());
        bytes[64..96].copy_from_slice(&self.vault_a_id.to_bytes());
        bytes[96..128].copy_from_slice(&self.vault_b_id.to_bytes());
        bytes[128..160].copy_from_slice(&self.liquidity_pool_id.to_bytes());
        bytes[160..176].copy_from_slice(&self.liquidity_pool_supply.to_le_bytes());
        bytes[176..192].copy_from_slice(&self.reserve_a.to_le_bytes());
        bytes[192..208].copy_from_slice(&self.reserve_b.to_le_bytes());
        bytes[208..224].copy_from_slice(&self.fees.to_le_bytes());
        bytes[224] = self.active as u8;

        bytes
            .to_vec()
            .try_into()
            .expect("225 bytes should fit into Data")
    }

    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() != POOL_DEFINITION_DATA_SIZE {
            None
        } else {
            let definition_token_a_id = AccountId::new(data[0..32].try_into().expect("Parse data: The AMM program must be provided a valid AccountId for Token A definition"));
            let definition_token_b_id = AccountId::new(data[32..64].try_into().expect("Parse data: The AMM program must be provided a valid AccountId for Vault B definition"));
            let vault_a_id = AccountId::new(data[64..96].try_into().expect(
                "Parse data: The AMM program must be provided a valid AccountId for Vault A",
            ));
            let vault_b_id = AccountId::new(data[96..128].try_into().expect(
                "Parse data: The AMM program must be provided a valid AccountId for Vault B",
            ));
            let liquidity_pool_id = AccountId::new(data[128..160].try_into().expect("Parse data: The AMM program must be provided a valid AccountId for Token liquidity pool definition"));
            let liquidity_pool_supply = u128::from_le_bytes(data[160..176].try_into().expect(
                "Parse data: The AMM program must be provided a valid u128 for liquidity cap",
            ));
            let reserve_a = u128::from_le_bytes(data[176..192].try_into().expect(
                "Parse data: The AMM program must be provided a valid u128 for reserve A balance",
            ));
            let reserve_b = u128::from_le_bytes(data[192..208].try_into().expect(
                "Parse data: The AMM program must be provided a valid u128 for reserve B balance",
            ));
            let fees = u128::from_le_bytes(
                data[208..224]
                    .try_into()
                    .expect("Parse data: The AMM program must be provided a valid u128 for fees"),
            );

            let active = match data[224] {
                0 => false,
                1 => true,
                _ => panic!("Parse data: The AMM program must be provided a valid bool for active"),
            };

            Some(Self {
                definition_token_a_id,
                definition_token_b_id,
                vault_a_id,
                vault_b_id,
                liquidity_pool_id,
                liquidity_pool_supply,
                reserve_a,
                reserve_b,
                fees,
                active,
            })
        }
    }
}

pub fn compute_pool_pda(
    amm_program_id: ProgramId,
    definition_token_a_id: AccountId,
    definition_token_b_id: AccountId,
) -> AccountId {
    AccountId::from((
        &amm_program_id,
        &compute_pool_pda_seed(definition_token_a_id, definition_token_b_id),
    ))
}

pub fn compute_pool_pda_seed(
    definition_token_a_id: AccountId,
    definition_token_b_id: AccountId,
) -> PdaSeed {
    use risc0_zkvm::sha::{Impl, Sha256};

    let (token_1, token_2) = match definition_token_a_id
        .value()
        .cmp(definition_token_b_id.value())
    {
        std::cmp::Ordering::Less => (definition_token_b_id, definition_token_a_id),
        std::cmp::Ordering::Greater => (definition_token_a_id, definition_token_b_id),
        std::cmp::Ordering::Equal => panic!("Definitions match"),
    };

    let mut bytes = [0; 64];
    bytes[0..32].copy_from_slice(&token_1.to_bytes());
    bytes[32..].copy_from_slice(&token_2.to_bytes());

    PdaSeed::new(
        Impl::hash_bytes(&bytes)
            .as_bytes()
            .try_into()
            .expect("Hash output must be exactly 32 bytes long"),
    )
}

pub fn compute_vault_pda(
    amm_program_id: ProgramId,
    pool_id: AccountId,
    definition_token_id: AccountId,
) -> AccountId {
    AccountId::from((
        &amm_program_id,
        &compute_vault_pda_seed(pool_id, definition_token_id),
    ))
}

pub fn compute_vault_pda_seed(pool_id: AccountId, definition_token_id: AccountId) -> PdaSeed {
    use risc0_zkvm::sha::{Impl, Sha256};

    let mut bytes = [0; 64];
    bytes[0..32].copy_from_slice(&pool_id.to_bytes());
    bytes[32..].copy_from_slice(&definition_token_id.to_bytes());

    PdaSeed::new(
        Impl::hash_bytes(&bytes)
            .as_bytes()
            .try_into()
            .expect("Hash output must be exactly 32 bytes long"),
    )
}

pub fn compute_liquidity_token_pda(amm_program_id: ProgramId, pool_id: AccountId) -> AccountId {
    AccountId::from((&amm_program_id, &compute_liquidity_token_pda_seed(pool_id)))
}

pub fn compute_liquidity_token_pda_seed(pool_id: AccountId) -> PdaSeed {
    use risc0_zkvm::sha::{Impl, Sha256};

    let mut bytes = [0; 64];
    bytes[0..32].copy_from_slice(&pool_id.to_bytes());
    bytes[32..].copy_from_slice(&[0; 32]);

    PdaSeed::new(
        Impl::hash_bytes(&bytes)
            .as_bytes()
            .try_into()
            .expect("Hash output must be exactly 32 bytes long"),
    )
}
