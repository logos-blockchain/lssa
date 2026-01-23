//! This crate contains core data structures and utilities for the Token Program.

use borsh::{BorshDeserialize, BorshSerialize};
use nssa_core::account::{AccountId, Data};
use serde::{Deserialize, Serialize};

pub const CURRENT_VERSION: u8 = 1;


// The AMM program has five functions (four directly accessible via instructions):
// 1. New AMM definition. Arguments to this function are:
//      * Seven accounts: [amm_pool, vault_holding_a, vault_holding_b, pool_lp, user_holding_a,
//        user_holding_b, user_holding_lp]. For new AMM Pool: amm_pool, vault_holding_a,
//        vault_holding_b, pool_lp and user_holding_lp are default accounts. amm_pool is a default
//        account that will initiate the amm definition account values vault_holding_a is a token
//        holding account for token a vault_holding_b is a token holding account for token b pool_lp
//        is a token holding account for the pool's lp token user_holding_a is a token holding
//        account for token a user_holding_b is a token holding account for token b user_holding_lp
//        is a token holding account for lp token
//      * PDA remark: Accounts amm_pool, vault_holding_a, vault_holding_b and pool_lp are PDA. The
//        AccountId for these accounts must be computed using: amm_pool AccountId <-
//        compute_pool_pda vault_holding_a, vault_holding_b <- compute_vault_pda pool_lp
//        <-compute_liquidity_token_pda
//      * Requires authorization: user_holding_a, user_holding_b
//      * An instruction data of 65-bytes, indicating the initial amm reserves' balances and
//        token_program_id with the following layout: [0x00 || array of balances (little-endian 16
//        bytes) || AMM_PROGRAM_ID)]
//      * Internally, calls compute_liquidity_token_pda_seed, compute_vault_pda_seed to authorize
//        transfers.
//      * Internally, calls compute_pool_da, compute_vault_pda and compute_vault_pda to check
//        various AccountIds are correct.
// 3. Add liquidity Arguments to this function are:
//      * Seven accounts: [amm_pool, vault_holding_a, vault_holding_b, pool_lp, user_holding_a,
//        user_holding_a, user_holding_lp].
//      * Requires authorization: user_holding_a, user_holding_b
//      * An instruction data byte string of length 49, amounts for minimum amount of liquidity from
//        add (min_amount_lp),
//      * max amount added for each token (max_amount_a and max_amount_b); indicate [0x02 || array
//        of of balances (little-endian 16 bytes)].
//      * Internally, calls compute_liquidity_token_pda_seed to compute liquidity pool PDA seed.
// 4. Remove liquidity
//      * Seven accounts: [amm_pool, vault_holding_a, vault_holding_b, pool_lp, user_holding_a,
//        user_holding_a, user_holding_lp].
//      * Requires authorization: user_holding_lp
//      * An instruction data byte string of length 49, amounts for minimum amount of liquidity to
//        redeem (balance_lp),
//      * minimum balance of each token to remove (min_amount_a and min_amount_b); indicate [0x03 ||
//        array of balances (little-endian 16 bytes)].
//      * Internally, calls compute_vault_pda_seed to compute vault_a and vault_b's PDA seed.



/// AMM Program Instruction.
#[derive(Serialize, Deserialize)]
pub enum Instruction {

    /// Create a new fungible token definition without metadata.
    ///
    /// Required accounts:
    /// - Token Definition account (uninitialized),
    /// - Token Holding account (uninitialized).
    NewDefinition { name: String, total_supply: u128 },

    /// Create a new fungible or non-fungible token definition with metadata.
    ///
    /// Required accounts:
    /// - Token Definition account (uninitialized),
    /// - Token Holding account (uninitialized),
    /// - Token Metadata account (uninitialized).
    NewDefinitionWithMetadata {
        new_definition: NewTokenDefinition,
        /// Boxed to avoid large enum variant size
        metadata: Box<NewTokenMetadata>,
    },


    
    /// Initialize a token holding account for a given token definition.
    ///
    /// Required accounts:
    /// - Token Definition account (initialized),
    /// - Token Holding account (uninitialized),
    InitializeAccount,


// 2. Swap assets Arguments to this function are:
//      * Five accounts: [amm_pool, vault_holding_a, vault_holding_b, user_holding_a,
//        user_holding_b].
//      * Requires authorization: user holding account associated to TOKEN_DEFINITION_ID (either
//        user_holding_a or user_holding_b)
//      * An instruction data byte string of length 65, indicating which token type to swap,
//        quantity of tokens put into the swap (of type TOKEN_DEFINITION_ID) and min_amount_out.
//        [0x01 || amount (little-endian 16 bytes) || TOKEN_DEFINITION_ID].
//      * Internally, calls swap logic.
//              * Four accounts: [user_deposit, vault_deposit, vault_withdraw, user_withdraw].
//                user_deposit and vault_deposit define deposit transaction. vault_withdraw and
//                user_withdraw define withdraw transaction.
//              * deposit_amount is the amount for user_deposit -> vault_deposit transfer.
//              * reserve_amounts is the pool's reserves; used to compute the withdraw amount.
//              * Outputs the token transfers as a Vec<ChainedCall> and the withdraw amount.


    ///TODO update description
    /// Burn tokens from the holder's account.
    ///
    /// Required accounts:
    /// - Token Definition account (initialized),
    /// - Token Holding account (authorized).
    Burn { amount_to_burn: u128 },


/*
fn add_liquidity(
    pre_states: &[AccountWithMetadata],
    balances: &[u128],
) -> (Vec<AccountPostState>, Vec<ChainedCall>) {
    if pre_states.len() != 7 {
        panic!("Invalid number of input accounts");
    }

    let pool = &pre_states[0];
    let vault_a = &pre_states[1];
    let vault_b = &pre_states[2];
    let pool_definition_lp = &pre_states[3];
    let user_holding_a = &pre_states[4];
    let user_holding_b = &pre_states[5];
    let user_holding_lp = &pre_states[6];

        let min_amount_lp = balances[0];
    let max_amount_a = balances[1];
    let max_amount_b = balances[2];

*/

    ///TODO: update for add
    /// Mint new tokens to the holder's account.
    ///
    /// Required accounts:
    /// - Token Definition account (authorized),
    /// - Token Holding account.
    AddLiquidity { min_amount_liquidity: u128, max_amount_to_add_token_a: u128, max_amount_to_add_token_b: u128 },


    /*
    fn remove_liquidity(
    pre_states: &[AccountWithMetadata],
    amounts: &[u128],
) -> (Vec<AccountPostState>, Vec<ChainedCall>) {
    if pre_states.len() != 7 {
        panic!("Invalid number of input accounts");
    }

    let pool = &pre_states[0];
    let vault_a = &pre_states[1];
    let vault_b = &pre_states[2];
    let pool_definition_lp = &pre_states[3];
    let user_holding_a = &pre_states[4];
    let user_holding_b = &pre_states[5];
    let user_holding_lp = &pre_states[6];

    if amounts.len() != 3 {
        panic!("Invalid number of balances");
    }

    let amount_lp = amounts[0];
    let amount_min_a = amounts[1];
    let amount_min_b = amounts[2];
    
     */
    //TODO types
    RemoveLiquidity { remove_liquidity_amount: u128, min_amount_to_remove_token_a: u128, min_amount_to_remove_token_b: u128 }
}

/*

#[derive(Serialize, Deserialize)]
pub enum NewTokenDefinition {
    Fungible { name: String, total_supply: u128 },
    NonFungible { name: String, print_balance: u128 },
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum TokenDefinition {
    Fungible {
        name: String,
        total_supply: u128,
        metadata_id: Option<AccountId>,
    },
    NonFungible {
        name: String,
        metadata_id: AccountId,
    },
}

impl TryFrom<&Data> for TokenDefinition {
    type Error = std::io::Error;

    fn try_from(data: &Data) -> Result<Self, Self::Error> {
        TokenDefinition::try_from_slice(data.as_ref())
    }
}

impl From<&TokenDefinition> for Data {
    fn from(definition: &TokenDefinition) -> Self {
        // Using size_of_val as size hint for Vec allocation
        let mut data = Vec::with_capacity(std::mem::size_of_val(definition));

        BorshSerialize::serialize(definition, &mut data)
            .expect("Serialization to Vec should not fail");

        Data::try_from(data).expect("Token definition encoded data should fit into Data")
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum TokenHolding {
    Fungible {
        definition_id: AccountId,
        balance: u128,
    },
    NftMaster {
        definition_id: AccountId,
        /// The amount of printed copies left - 1 (1 reserved for master copy itself).
        print_balance: u128,
    },
    NftPrintedCopy {
        definition_id: AccountId,
        /// Whether nft is owned by the holder.
        owned: bool,
    },
}

impl TokenHolding {
    pub fn zeroized_clone_from(other: &Self) -> Self {
        match other {
            TokenHolding::Fungible { definition_id, .. } => TokenHolding::Fungible {
                definition_id: *definition_id,
                balance: 0,
            },
            TokenHolding::NftMaster { definition_id, .. } => TokenHolding::NftMaster {
                definition_id: *definition_id,
                print_balance: 0,
            },
            TokenHolding::NftPrintedCopy { definition_id, .. } => TokenHolding::NftPrintedCopy {
                definition_id: *definition_id,
                owned: false,
            },
        }
    }

    pub fn zeroized_from_definition(
        definition_id: AccountId,
        definition: &TokenDefinition,
    ) -> Self {
        match definition {
            TokenDefinition::Fungible { .. } => TokenHolding::Fungible {
                definition_id,
                balance: 0,
            },
            TokenDefinition::NonFungible { .. } => TokenHolding::NftPrintedCopy {
                definition_id,
                owned: false,
            },
        }
    }

    pub fn definition_id(&self) -> AccountId {
        match self {
            TokenHolding::Fungible { definition_id, .. } => *definition_id,
            TokenHolding::NftMaster { definition_id, .. } => *definition_id,
            TokenHolding::NftPrintedCopy { definition_id, .. } => *definition_id,
        }
    }
}

impl TryFrom<&Data> for TokenHolding {
    type Error = std::io::Error;

    fn try_from(data: &Data) -> Result<Self, Self::Error> {
        TokenHolding::try_from_slice(data.as_ref())
    }
}

impl From<&TokenHolding> for Data {
    fn from(holding: &TokenHolding) -> Self {
        // Using size_of_val as size hint for Vec allocation
        let mut data = Vec::with_capacity(std::mem::size_of_val(holding));

        BorshSerialize::serialize(holding, &mut data)
            .expect("Serialization to Vec should not fail");

        Data::try_from(data).expect("Token holding encoded data should fit into Data")
    }
}

#[derive(Serialize, Deserialize)]
pub struct NewTokenMetadata {
    pub uri: String,
    pub creators: String,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct TokenMetadata {
    pub version: u8,
    pub definition_id: AccountId,
    pub uri: String,
    pub creators: String,
    /// Block id
    pub primary_sale_date: u64,
}

impl TryFrom<&Data> for TokenMetadata {
    type Error = std::io::Error;

    fn try_from(data: &Data) -> Result<Self, Self::Error> {
        TokenMetadata::try_from_slice(data.as_ref())
    }
}

impl From<&TokenMetadata> for Data {
    fn from(metadata: &TokenMetadata) -> Self {
        // Using size_of_val as size hint for Vec allocation
        let mut data = Vec::with_capacity(std::mem::size_of_val(metadata));

        BorshSerialize::serialize(metadata, &mut data)
            .expect("Serialization to Vec should not fail");

        Data::try_from(data).expect("Token metadata encoded data should fit into Data")
    }
}*/