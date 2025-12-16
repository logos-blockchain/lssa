use nssa_core::{
    account::{Account, AccountId, AccountWithMetadata, Data},
    program::{ProgramId, ProgramInput, ChainedCall, AccountPostState, PdaSeed, read_nssa_inputs, write_nssa_outputs_with_chained_call},
};

// The AMM program has five functions (four directly accessible via instructions):
// 1. New AMM definition.
//    Arguments to this function are:
//      * Seven **default** accounts: [amm_pool, vault_holding_a, vault_holding_b, pool_lp, user_holding_a, user_holding_b, user_holding_lp].
//        amm_pool is a default account that will initiate the amm definition account values
//        vault_holding_a is a token holding account for token a
//        vault_holding_b is a token holding account for token b
//        pool_lp is a token holding account for the pool's lp token 
//        user_holding_a is a token holding account for token a
//        user_holding_b is a token holding account for token b
//        user_holding_lp is a token holding account for lp token
//      * Requires authorization: user_holding_a, user_holding_b
//      * An instruction data of 65-bytes, indicating the initial amm reserves' balances and token_program_id with
//        the following layout:
//        [0x00 || array of balances (little-endian 16 bytes) || AMM_PROGRAM_ID)]
// 2. Swap assets
//    Arguments to this function are:
//      * Five accounts: [amm_pool, vault_holding_1, vault_holding_2, user_holding_a, user_holding_b].
//      * Requires authorization: user holding account associated to TOKEN_DEFINITION_ID (either user_holding_a or user_holding_b)
//      * An instruction data byte string of length 49, indicating which token type to swap, quantity of tokens put into the swap 
//        (of type TOKEN_DEFINITION_ID) and min_amount_out.
//        [0x01 || amount (little-endian 16 bytes) || TOKEN_DEFINITION_ID].
// 3. Add liquidity
//    Arguments to this function are:
//      * Seven accounts: [amm_pool, vault_holding_a, vault_holding_b, pool_lp, user_holding_a, UserHouser_holding_a, user_holding_lp].
//      * Requires authorization: user_holding_a, user_holding_b
//      * An instruction data byte string of length 49, amounts for minimum amount of liquidity from add (min_amount_lp),
//      * max amount added for each token (max_amount_a and max_amount_b); indicate 
//        [0x02 || array of of balances (little-endian 16 bytes)].
// 4. Remove liquidity
//      * Seven accounts: [amm_pool, vault_holding_a, vault_holding_b, pool_lp, user_holding_a, UserHouser_holding_a, user_holding_lp].
//      * Requires authorization: user_holding_lp
//      * An instruction data byte string of length 49, amounts for minimum amount of liquidity to redeem (balance_lp),
//      * minimum balance of each token to remove (min_amount_a and min_amount_b); indicate 
//        [0x03 || array of balances (little-endian 16 bytes)].
// - Internal functions:
// - Swap logic
//    Arguments of this function are:
//      * Four accounts: [user_deposit_tx, vault_deposit_tx, vault_withdraw_tx, user_withdraw_tx].
//        user_deposit_tx and vault_deposit_tx define deposit transaction.
//        vault_withdraw_tx and user_withdraw_tx define withdraw transaction.
//      * deposit_amount is the amount for user_deposit_tx -> vault_deposit_tx transfer.
//      * reserve_amounts is the pool's reserves; used to compute the withdraw amount.
//      * Outputs the token transfers as a Vec<ChainedCall> and the withdraw amount.
// - PDA computations:
//      * compute_pool_pda: AMM_PROGRAM_ID, token definitions for the pool pair
//      * compute_vault_pda: AMM_PROGRAM_ID, pool definition id, definition token id
//      * compute_liquidity_token_pda: AMM_PROGRAM, pool definition id, pool definition id
// - PDA seed computations:
//      * compute_pool_pda_seed: token definitions for the pool pair
//      * compute_vault_pda_seed: pool definition id, definition token id,
//      * compute_liquidity_token_pda_seed: pool definition id

const POOL_DEFINITION_DATA_SIZE: usize = 225;

#[derive(Default)]
struct PoolDefinition{
    definition_token_a_id: AccountId,
    definition_token_b_id: AccountId,
    vault_a_id: AccountId,
    vault_b_id: AccountId,
    liquidity_pool_id: AccountId,
    liquidity_pool_supply: u128,
    reserve_a: u128,
    reserve_b: u128,
    fees: u128,
    active: bool
}

impl PoolDefinition {
    fn into_data(self) -> Data {
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

    fn parse(data: &[u8]) -> Option<Self> {
        if data.len() != POOL_DEFINITION_DATA_SIZE {
            None
        } else {
            let definition_token_a_id = AccountId::new(data[0..32].try_into().expect("Parse data: The AMM program must be provided a valid AccountId for Token A definition"));
            let definition_token_b_id = AccountId::new(data[32..64].try_into().expect("Parse data: The AMM program must be provided a valid AccountId for Vault B definition"));
            let vault_a_id = AccountId::new(data[64..96].try_into().expect("Parse data: The AMM program must be provided a valid AccountId for Vault A"));
            let vault_b_id = AccountId::new(data[96..128].try_into().expect("Parse data: The AMM program must be provided a valid AccountId for Vault B"));
            let liquidity_pool_id = AccountId::new(data[128..160].try_into().expect("Parse data: The AMM program must be provided a valid AccountId for Token liquidity pool definition"));
            let liquidity_pool_supply = u128::from_le_bytes(data[160..176].try_into().expect("Parse data: The AMM program must be provided a valid u128 for liquidity cap"));
            let reserve_a = u128::from_le_bytes(data[176..192].try_into().expect("Parse data: The AMM program must be provided a valid u128 for reserve A balance"));
            let reserve_b = u128::from_le_bytes(data[192..208].try_into().expect("Parse data: The AMM program must be provided a valid u128 for reserve B balance"));
            let fees = u128::from_le_bytes(data[208..224].try_into().expect("Parse data: The AMM program must be provided a valid u128 for fees"));

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

//TODO: remove repeated code for Token_Definition and TokenHoldling
const TOKEN_DEFINITION_TYPE: u8 = 0;
const TOKEN_DEFINITION_DATA_SIZE: usize = 23;

const TOKEN_HOLDING_TYPE: u8 = 1;
const TOKEN_HOLDING_DATA_SIZE: usize = 49;

struct TokenDefinition {
    account_type: u8,
    name: [u8; 6],
    total_supply: u128,
}

struct TokenHolding {
    account_type: u8,
    definition_id: AccountId,
    balance: u128,
}

impl TokenDefinition {
    fn into_data(self) -> Data {
        let mut bytes = [0; TOKEN_DEFINITION_DATA_SIZE];
        bytes[0] = self.account_type;
        bytes[1..7].copy_from_slice(&self.name);
        bytes[7..].copy_from_slice(&self.total_supply.to_le_bytes());
        bytes
            .to_vec()
            .try_into()
            .expect("23 bytes should fit into Data")
    }


    fn parse(data: &[u8]) -> Option<Self> {
        if data.len() != TOKEN_DEFINITION_DATA_SIZE || data[0] != TOKEN_DEFINITION_TYPE {
            None
        } else {
            let account_type = data[0];
            let name = data[1..7].try_into().unwrap();
            let total_supply = u128::from_le_bytes(
                data[7..]
                    .try_into()
                    .expect("Total supply must be 16 bytes little-endian"),
            );
            Some(Self {
                account_type,
                name,
                total_supply,
            })
        }
    }
}

impl TokenHolding {
    fn new(definition_id: &AccountId) -> Self {
        Self {
            account_type: TOKEN_HOLDING_TYPE,
            definition_id: definition_id.clone(),
            balance: 0,
        }
    }

    fn parse(data: &[u8]) -> Option<Self> {
        if data.len() != TOKEN_HOLDING_DATA_SIZE || data[0] != TOKEN_HOLDING_TYPE {
            None
        } else {
            let account_type = data[0];
            let definition_id = AccountId::new(
                data[1..33]
                    .try_into()
                    .expect("Defintion ID must be 32 bytes long"),
            );
            let balance = u128::from_le_bytes(
                data[33..]
                    .try_into()
                    .expect("balance must be 16 bytes little-endian"),
            );
            Some(Self {
                definition_id,
                balance,
                account_type,
            })
        }
    }

    fn into_data(self) -> Data {
        let mut bytes = [0; TOKEN_HOLDING_DATA_SIZE];
        bytes[0] = self.account_type;
        bytes[1..33].copy_from_slice(&self.definition_id.to_bytes());
        bytes[33..].copy_from_slice(&self.balance.to_le_bytes());
        
        bytes
            .to_vec()
            .try_into()
            .expect("49 bytes should fit into Data")
    }
}


type Instruction = Vec<u8>;
fn main() {
    let (ProgramInput {
        pre_states,
        instruction,
    }, instruction_words) = read_nssa_inputs::<Instruction>();

    let (post_states, chained_calls) = match instruction[0] {
        0 => {
            let balance_a: u128 = u128::from_le_bytes(instruction[1..17].try_into().expect("New definition: AMM Program expects u128 for balance a"));
            let balance_b: u128 = u128::from_le_bytes(instruction[17..33].try_into().expect("New definition: AMM Program expects u128 for balance b"));

            // Convert Vec<u8> to ProgramId ([u32;8])
            let mut amm_program_id: [u32;8] = [0;8];
            amm_program_id[0] = u32::from_le_bytes(instruction[33..37].try_into().expect("New definition: AMM Program expects valid u32"));
            amm_program_id[1] = u32::from_le_bytes(instruction[37..41].try_into().expect("New definition: AMM Program expects valid u32"));
            amm_program_id[2] = u32::from_le_bytes(instruction[41..45].try_into().expect("New definition: AMM Program expects valid u32"));
            amm_program_id[3] = u32::from_le_bytes(instruction[45..49].try_into().expect("New definition: AMM Program expects valid u32"));
            amm_program_id[4] = u32::from_le_bytes(instruction[49..53].try_into().expect("New definition: AMM Program expects valid u32"));
            amm_program_id[5] = u32::from_le_bytes(instruction[53..57].try_into().expect("New definition: AMM Program expects valid u32"));
            amm_program_id[6] = u32::from_le_bytes(instruction[57..61].try_into().expect("New definition: AMM Program expects valid u32"));
            amm_program_id[7] = u32::from_le_bytes(instruction[61..65].try_into().expect("New definition: AMM Program expects valid u32"));

            new_definition(&pre_states, &[balance_a, balance_b], amm_program_id)
        }
        1 => {
            let mut token_in_id: [u8;32] = [0;32];
            token_in_id[0..].copy_from_slice(&instruction[33..65]);
            let token_in_id = AccountId::new(token_in_id);
            
            let amount_in = u128::from_le_bytes(instruction[1..17].try_into().expect("Swap: AMM Program expects valid u128 for balance to move"));
            let min_amount_out = u128::from_le_bytes(instruction[17..33].try_into().expect("Swap: AMM Program expects valid u128 for balance to move"));

            swap(&pre_states, &[amount_in, min_amount_out], token_in_id)
        }
        2 => {
            let min_amount_lp = u128::from_le_bytes(instruction[1..17].try_into().expect("Add liquidity: AMM Program expects valid u128 for min amount liquidity")); 
            let max_amount_a = u128::from_le_bytes(instruction[17..33].try_into().expect("Add liquidity: AMM Program expects valid u128 for max amount a"));
            let max_amount_b = u128::from_le_bytes(instruction[33..49].try_into().expect("Add liquidity: AMM Program expects valid u128 for max amount b"));
            
            add_liquidity(&pre_states, &[min_amount_lp, max_amount_a, max_amount_b])
        }
        3 => {
            let balance_lp = u128::from_le_bytes(instruction[1..17].try_into().expect("Remove liquidity: AMM Program expects valid u128 for balance liquidity"));
            let min_amount_a = u128::from_le_bytes(instruction[17..33].try_into().expect("Remove liquidity: AMM Program expects valid u128 for balance a"));
            let min_amount_b = u128::from_le_bytes(instruction[33..49].try_into().expect("Remove liquidity: AMM Program expects valid u128 for balance b"));

            remove_liquidity(&pre_states, &[balance_lp, min_amount_a, min_amount_b])
        }
        _ => panic!("Invalid instruction"),
    };

    write_nssa_outputs_with_chained_call(instruction_words, pre_states, post_states, chained_calls);
}


fn compute_pool_pda(amm_program_id: ProgramId, definition_token_a_id: AccountId, definition_token_b_id: AccountId) -> AccountId {
    AccountId::from((&amm_program_id,
        &compute_pool_pda_seed(definition_token_a_id, definition_token_b_id)))
}

fn compute_pool_pda_seed(definition_token_a_id: AccountId, definition_token_b_id: AccountId) -> PdaSeed {
    use risc0_zkvm::sha::{Impl, Sha256};

    let mut i: usize = 0;
    let (token_1, token_2) = loop {
        if definition_token_a_id.value()[i] > definition_token_b_id.value()[i] {
            let token_1 = definition_token_a_id.clone();
            let token_2 = definition_token_b_id.clone();
            break (token_1, token_2)
        } else if definition_token_a_id.value()[i] < definition_token_b_id.value()[i] {
            let token_1 = definition_token_b_id.clone();
            let token_2 = definition_token_a_id.clone();
            break (token_1, token_2)
        }
        
        if i == 32 {
            panic!("Definitions match");
        } else {
            i += 1;
        }
    };

    let mut bytes = [0; 64];
    bytes[0..32].copy_from_slice(&token_1.to_bytes());
    bytes[32..].copy_from_slice(&token_2.to_bytes());

    PdaSeed::new(Impl::hash_bytes(&bytes).as_bytes().try_into().expect("Hash output must be exactly 32 bytes long"))
}

fn compute_vault_pda(amm_program_id: ProgramId, 
                    pool_id: AccountId, 
                    definition_token_id: AccountId
) -> AccountId {
    AccountId::from((&amm_program_id,
        &compute_vault_pda_seed(pool_id, definition_token_id)))
}

fn compute_vault_pda_seed(pool_id: AccountId,
                        definition_token_id: AccountId
) -> PdaSeed {
    use risc0_zkvm::sha::{Impl, Sha256};

    let mut bytes = [0; 64];
    bytes[0..32].copy_from_slice(&pool_id.to_bytes());
    bytes[32..].copy_from_slice(&definition_token_id.to_bytes());

    PdaSeed::new(Impl::hash_bytes(&bytes).as_bytes().try_into().expect("Hash output must be exactly 32 bytes long"))
}

fn compute_liquidity_token_pda(amm_program_id: ProgramId, pool_id: AccountId) -> AccountId {
    AccountId::from((&amm_program_id,
        &compute_liquidity_token_pda_seed(pool_id)))
}

fn compute_liquidity_token_pda_seed(pool_id: AccountId) -> PdaSeed {
    use risc0_zkvm::sha::{Impl, Sha256};

    let mut bytes = [0; 64];
    bytes[0..32].copy_from_slice(&pool_id.to_bytes());
    bytes[32..].copy_from_slice(&[0;32]);

    PdaSeed::new(Impl::hash_bytes(&bytes).as_bytes().try_into().expect("Hash output must be exactly 32 bytes long"))
}

fn new_definition (
        pre_states: &[AccountWithMetadata],
        balance_in: &[u128],
        amm_program_id: ProgramId,
    ) -> (Vec<AccountPostState>, Vec<ChainedCall>) {

    //Pool accounts: pool itself, and its 2 vaults and LP token
    //2 accounts for funding tokens
    //initial funder's LP account
    if pre_states.len() != 7 {
        panic!("Invalid number of input accounts")
    }

    if balance_in.len() != 2 {
        panic!("Invalid number of input balances")
    }

    let pool = &pre_states[0];
    let vault_a = &pre_states[1];
    let vault_b = &pre_states[2];
    let pool_lp = &pre_states[3];
    let user_holding_a = &pre_states[4];
    let user_holding_b = &pre_states[5];
    let user_holding_lp = &pre_states[6];

    let amount_a = balance_in[0];
    let amount_b = balance_in[1];

    // Prevents pool constant coefficient (k) from being 0.
    if amount_a == 0 || amount_b == 0 {
        panic!("Balances must be nonzero")
    }

    // Verify token_a and token_b are different
    let definition_token_a_id = TokenHolding::parse(&user_holding_a.account.data)
        .expect("New definition: AMM Program expects valid Token Holding account for Token A").definition_id;
    let definition_token_b_id = TokenHolding::parse(&user_holding_b.account.data)
        .expect("New definition: AMM Program expects valid Token Holding account for Token B").definition_id;
 
    // both instances of the same token program
    let token_program = user_holding_a.account.program_owner;

    if definition_token_a_id == definition_token_b_id {
        panic!("Cannot set up a swap for a token with itself")
    }

    if pool.account_id != compute_pool_pda(amm_program_id.clone(),
                                            definition_token_a_id.clone(),
                                            definition_token_b_id.clone()) {
        panic!("Pool Definition Account ID does not match PDA");
    }

    if vault_a.account_id != compute_vault_pda(amm_program_id.clone(),
                                                pool.account_id.clone(),
                                                definition_token_a_id.clone()) ||
        vault_b.account_id != compute_vault_pda(amm_program_id.clone(),
                                                pool.account_id.clone(),
                                                definition_token_b_id.clone()) {
        panic!("Vault ID does not match PDA");        
    }

    if pool_lp.account_id != compute_liquidity_token_pda(amm_program_id.clone(),
                                                        pool.account_id.clone()) {
        panic!("Liquidity pool Token Definition Account ID does not match PDA");
    }

    // Verify that Pool Account is not active
    let pool_account_data = if pool.account == Account::default() {
        PoolDefinition::default()
    } else { 
        PoolDefinition::parse(&pool.account.data).expect("AMM program expects a valid Pool account")
    };

    if pool_account_data.active {
        panic!("Cannot initialize an active Pool Definition")
    }

    //3. LP Token minting calculation
    // We assume LP is based on the initial deposit amount for Token_A.

    // 5. Update pool account
    let mut pool_post = pool.account.clone();
    let pool_post_definition = PoolDefinition {
            definition_token_a_id,
            definition_token_b_id,
            vault_a_id: vault_a.account_id.clone(),
            vault_b_id: vault_b.account_id.clone(),
            liquidity_pool_id: pool_lp.account_id.clone(),
            liquidity_pool_supply: amount_a.clone(),
            reserve_a: amount_a.clone(),
            reserve_b: amount_b.clone(),
            fees: 0u128, //TODO: we assume all fees are 0 for now.
            active: true, 
    };

    pool_post.data = pool_post_definition.into_data().try_into().expect("Data too big");
    let pool_post: AccountPostState = 
        if pool.account == Account::default() { AccountPostState::new_claimed(pool_post.clone()) }
        else { AccountPostState::new(pool_post.clone()) };

    let mut chained_calls = Vec::<ChainedCall>::new();

    //Chain call for Token A (user_holding_a -> Vault_A)
    let mut instruction_data = [0; 23];
    instruction_data[0] = 1;
    instruction_data[1..17].copy_from_slice(&amount_a.to_le_bytes());
    let instruction_data = risc0_zkvm::serde::to_vec(&instruction_data).expect("New definition: AMM Program expects valid token transfer instruction data");
    let call_token_a = ChainedCall{
            program_id: user_holding_a.account.program_owner,
            instruction_data,
            pre_states: vec![user_holding_a.clone(), vault_a.clone()],
            pda_seeds: Vec::<PdaSeed>::new(),
        };

    //Chain call for Token B (user_holding_b -> Vault_B)
    let mut instruction_data = [0; 23];
    instruction_data[0] = 1;
    instruction_data[1..17].copy_from_slice(&amount_b.to_le_bytes());
    let instruction_data = risc0_zkvm::serde::to_vec(&instruction_data).expect("New definition: AMM Program expects valid instruction_data");

    let call_token_b = ChainedCall{
            program_id: user_holding_b.account.program_owner,
            instruction_data,
            pre_states: vec![user_holding_b.clone(), vault_b.clone()],
            pda_seeds: Vec::<PdaSeed>::new(),
        };

    //Chain call for liquidity token (TokenLP definition -> User LP Holding)
    let mut instruction_data = [0; 23];
    instruction_data[0] = if pool.account == Account::default() { 0 } else { 4 }; //new or mint
    let nme = if pool.account == Account::default() { [1u8;6] } else { [0u8; 6] };

    instruction_data[1..17].copy_from_slice(&amount_a.to_le_bytes());
    instruction_data[17..].copy_from_slice(&nme);
    let instruction_data = risc0_zkvm::serde::to_vec(&instruction_data).expect("New definition: AMM Program expects valid instruction_data");

    let mut pool_lp_auth = pool_lp.clone();
    pool_lp_auth.is_authorized = true;

    let token_program_id = user_holding_a.account.program_owner;
    let call_token_lp = ChainedCall{
            program_id: token_program_id,
            instruction_data,
            pre_states: vec![pool_lp_auth.clone(), user_holding_lp.clone()],
            pda_seeds:  vec![compute_liquidity_token_pda_seed(pool.account_id.clone())],
        };

    chained_calls.push(call_token_lp);
    chained_calls.push(call_token_b);
    chained_calls.push(call_token_a);


    let post_states = vec![
        pool_post.clone(),
        AccountPostState::new(pre_states[1].account.clone()),
        AccountPostState::new(pre_states[2].account.clone()),
        AccountPostState::new(pre_states[3].account.clone()),
        AccountPostState::new(pre_states[4].account.clone()),
        AccountPostState::new(pre_states[5].account.clone()),
        AccountPostState::new(pre_states[6].account.clone())];

    (post_states.clone(), chained_calls)
}

fn swap(
        pre_states: &[AccountWithMetadata],
        amounts: &[u128],
        token_in_id: AccountId,
    ) -> (Vec<AccountPostState>, Vec<ChainedCall>) {

    if pre_states.len() != 5 {
        panic!("Invalid number of input accounts");
    }

    if amounts.len() != 2 {
        panic!("Invalid number of amounts provided");
    }

    let amount_in = amounts[0];
    let min_amount_out = amounts[1]; 

    let pool = &pre_states[0];
    let vault_a = &pre_states[1];
    let vault_b = &pre_states[2];
    let user_holding_a = &pre_states[3];
    let user_holding_b = &pre_states[4];

    // Verify vaults are in fact vaults
    let pool_def_data = PoolDefinition::parse(&pool.account.data).expect("Swap: AMM Program expects a valid Pool Definition Account");

    if !pool_def_data.active {
        panic!("Pool is inactive");
    }

    if vault_a.account_id != pool_def_data.vault_a_id {  
        panic!("Vault A was not provided");
    }
        
    if vault_b.account_id != pool_def_data.vault_b_id {
        panic!("Vault B was not provided");
    }

    // fetch pool reserves
    // validates reserves is at least the vaults' balances
    if TokenHolding::parse(&vault_a.account.data).expect("Swap: AMM Program expects a valid Token Holding Account for Vault A").balance < pool_def_data.reserve_a {
        panic!("Reserve for Token A exceeds vault balance");
    }
    if TokenHolding::parse(&vault_b.account.data).expect("Swap: AMM Program expects a valid Token Holding Account for Vault B").balance < pool_def_data.reserve_b {
        panic!("Reserve for Token B exceeds vault balance");        
    }

    let (chained_calls, [deposit_a, withdraw_a], [deposit_b, withdraw_b])
    = if token_in_id == pool_def_data.definition_token_a_id {
        let (chained_calls, withdraw_b) = swap_logic(&[user_holding_a.clone(),
                                                        vault_a.clone(),
                                                        vault_b.clone(),
                                                        user_holding_b.clone()],
                                                        &[amount_in, min_amount_out],
                                                        &[pool_def_data.reserve_a, pool_def_data.reserve_b],
                                                        pool.account_id.clone());
                
        (chained_calls, [amount_in, 0], [0, withdraw_b])
    } else if token_in_id == pool_def_data.definition_token_b_id {
        let (chained_calls, withdraw_a) = swap_logic(&[user_holding_b.clone(),
                                                        vault_b.clone(),
                                                        vault_a.clone(),
                                                        user_holding_a.clone()],
                                                        &[amount_in, min_amount_out],
                                                        &[pool_def_data.reserve_b, pool_def_data.reserve_a],
                                                        pool.account_id.clone());

        (chained_calls, [0, withdraw_a], [amount_in, 0])
    } else {
        panic!("AccountId is not a token type for the pool");
    };         

    // Update pool account
    let mut pool_post = pool.account.clone();
    let pool_post_definition = PoolDefinition {
            definition_token_a_id: pool_def_data.definition_token_a_id.clone(),
            definition_token_b_id: pool_def_data.definition_token_b_id.clone(),
            vault_a_id: pool_def_data.vault_a_id.clone(),
            vault_b_id: pool_def_data.vault_b_id.clone(),
            liquidity_pool_id: pool_def_data.liquidity_pool_id.clone(),
            liquidity_pool_supply: pool_def_data.liquidity_pool_supply.clone(),
            reserve_a: pool_def_data.reserve_a + deposit_a - withdraw_a,
            reserve_b: pool_def_data.reserve_b + deposit_b - withdraw_b,
            fees: 0u128,
            active: true, 
    };

    pool_post.data = pool_post_definition.into_data().try_into().expect("Data too big");
    
    let post_states = vec![
        AccountPostState::new(pool_post.clone()),
        AccountPostState::new(pre_states[1].account.clone()),
        AccountPostState::new(pre_states[2].account.clone()),
        AccountPostState::new(pre_states[3].account.clone()),
        AccountPostState::new(pre_states[4].account.clone())];

    (post_states, chained_calls)
}

fn swap_logic(
    pre_states: &[AccountWithMetadata],
    balances: &[u128],
    reserve_amounts: &[u128],
    pool_id: AccountId,
) -> (Vec<ChainedCall>, u128)
{
    let user_deposit_tx = pre_states[0].clone();
    let vault_deposit_tx = pre_states[1].clone();
    let vault_withdraw_tx = pre_states[2].clone();
    let user_withdraw_tx = pre_states[3].clone();

    let reserve_deposit_vault_amount = reserve_amounts[0];
    let reserve_withdraw_vault_amount = reserve_amounts[1];

    let deposit_amount = balances[0];
    let min_amount_out = balances[1];

    // Compute withdraw amount
    // Compute pool's exchange constant
    // let k = pool_def_data.reserve_a * pool_def_data.reserve_b; 
    let withdraw_amount = (reserve_withdraw_vault_amount * deposit_amount)/(reserve_deposit_vault_amount + deposit_amount);

    //Slippage check
    if min_amount_out > withdraw_amount {
        panic!("Withdraw amount is less than minimal amount out");
    }

    if withdraw_amount == 0 {
        panic!("Withdraw amount should be nonzero");
    }

    let mut chained_calls = Vec::new();
    let mut instruction_data = [0;23];
    instruction_data[0] = 1;
    instruction_data[1..17].copy_from_slice(&deposit_amount.to_le_bytes());
    let instruction_data = risc0_zkvm::serde::to_vec(&instruction_data).expect("Swap Logic: AMM Program expects valid transaction instruction data");
    chained_calls.push(
        ChainedCall{
                program_id: vault_deposit_tx.account.program_owner,
                instruction_data,
                pre_states: vec![user_deposit_tx.clone(), vault_deposit_tx.clone()],
                pda_seeds: Vec::<PdaSeed>::new(),
            }
    );

    let mut vault_withdraw_tx = vault_withdraw_tx.clone();
    vault_withdraw_tx.is_authorized = true;

    let mut instruction_data = [0;23];
    instruction_data[0] = 1;
    instruction_data[1..17].copy_from_slice(&withdraw_amount.to_le_bytes());
    let instruction_data = risc0_zkvm::serde::to_vec(&instruction_data).expect("Swap Logic: AMM Program expects valid transaction instruction data");
    chained_calls.push(
        ChainedCall{
                program_id: vault_deposit_tx.account.program_owner,
                instruction_data,
                pre_states: vec![vault_withdraw_tx.clone(), user_withdraw_tx.clone()],
                pda_seeds: vec![compute_vault_pda_seed(pool_id, 
                    TokenHolding::parse(&vault_withdraw_tx.account.data)
                    .expect("Swap Logic: AMM Program expects valid token data")
                    .definition_id)],
            }
    );

    (chained_calls, withdraw_amount)
}

fn add_liquidity(pre_states: &[AccountWithMetadata],
    balances: &[u128]) -> (Vec<AccountPostState>, Vec<ChainedCall>) {

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

    // Verify vaults are in fact vaults
    let pool_def_data = PoolDefinition::parse(&pool.account.data).expect("Add liquidity: AMM Program expects valid Pool Definition Account");
    if vault_a.account_id != pool_def_data.vault_a_id {
        panic!("Vault A was not provided");
    }

    if pool_def_data.liquidity_pool_id != pool_definition_lp.account_id {
        panic!("LP definition mismatch");
    }

    if vault_b.account_id != pool_def_data.vault_b_id {
        panic!("Vault B was not provided");
    }    
    if balances.len() != 3 {
        panic!("Invalid number of input balances");
    }

    let min_amount_lp = balances[0];
    let max_amount_a = balances[1];
    let max_amount_b = balances[2];

    if max_amount_a == 0 || max_amount_b == 0 {
        panic!("Both max-balances must be nonzero");
    }

    if min_amount_lp == 0 {
        panic!("Min-lp must be nonzero");
    }
    
    // 2. Determine deposit amount
    let vault_b_balance = TokenHolding::parse(&vault_b.account.data).expect("Add liquidity: AMM Program expects valid Token Holding Account for Vault B").balance;
    let vault_a_balance = TokenHolding::parse(&vault_a.account.data).expect("Add liquidity: AMM Program expects valid Token Holding Account for Vault A").balance;

    if pool_def_data.reserve_a == 0 || pool_def_data.reserve_b == 0 {
        panic!("Reserves must be nonzero");
    }

    if vault_a_balance < pool_def_data.reserve_a || vault_b_balance < pool_def_data.reserve_b {
        panic!("Vaults' balances must be at least the reserve amounts");
    }

    // Calculate actual_amounts
    let ideal_a: u128 = (pool_def_data.reserve_a*max_amount_b)/pool_def_data.reserve_b;
    let ideal_b: u128 = (pool_def_data.reserve_b*max_amount_a)/pool_def_data.reserve_a;

    let actual_amount_a = if ideal_a > max_amount_a { max_amount_a } else { ideal_a };
    let actual_amount_b = if ideal_b > max_amount_b { max_amount_b } else { ideal_b };

    // 3. Validate amounts
    if max_amount_a < actual_amount_a || max_amount_b < actual_amount_b {
        panic!("Actual trade amounts cannot exceed max_amounts");
    }
    
    if actual_amount_a == 0 || actual_amount_b == 0 {
        panic!("A trade amount is 0");
    }
    
    // 4. Calculate LP to mint
    let delta_lp = std::cmp::min(pool_def_data.liquidity_pool_supply * actual_amount_a/pool_def_data.reserve_a,
                    pool_def_data.liquidity_pool_supply * actual_amount_b/pool_def_data.reserve_b);

    if delta_lp == 0 {
        panic!("Payable LP must be nonzero");
    }

    if delta_lp < min_amount_lp {
        panic!("Payable LP is less than provided minimum LP amount");
    }
    
    // 5. Update pool account
    let mut pool_post = pool.account.clone();
    let pool_post_definition = PoolDefinition {
            definition_token_a_id: pool_def_data.definition_token_a_id.clone(),
            definition_token_b_id: pool_def_data.definition_token_b_id.clone(),
            vault_a_id: pool_def_data.vault_a_id.clone(),
            vault_b_id: pool_def_data.vault_b_id.clone(),
            liquidity_pool_id: pool_def_data.liquidity_pool_id.clone(),
            liquidity_pool_supply: pool_def_data.liquidity_pool_supply + delta_lp,
            reserve_a: pool_def_data.reserve_a + actual_amount_a,
            reserve_b: pool_def_data.reserve_b + actual_amount_b,
            fees: 0u128,
            active: true,  
    };
    
    pool_post.data = pool_post_definition.into_data().try_into().expect("Data too big");
    let mut chained_call = Vec::new();

    // Chain call for Token A (UserHoldingA -> Vault_A)
    let mut instruction_data = [0; 23];
    instruction_data[0] = 1;
    instruction_data[1..17].copy_from_slice(&actual_amount_a.to_le_bytes());
    let instruction_data = risc0_zkvm::serde::to_vec(&instruction_data).expect("Add liquidity: AMM Program expects valid token transfer instruction data");
    let call_token_a = ChainedCall{
            program_id: vault_a.account.program_owner,
            instruction_data,
            pre_states: vec![user_holding_a.clone(), vault_a.clone()],
            pda_seeds: Vec::<PdaSeed>::new(),
        };

    // Chain call for Token B (UserHoldingB -> Vault_B)        
    let mut instruction_data = [0; 23];
    instruction_data[0] = 1;
    instruction_data[1..17].copy_from_slice(&actual_amount_b.to_le_bytes());
    let instruction_data = risc0_zkvm::serde::to_vec(&instruction_data).expect("Add liquidity: AMM Program expects valid token transfer instruction data");
    let call_token_b = ChainedCall{
            program_id: vault_b.account.program_owner,
            instruction_data,
            pre_states: vec![user_holding_b.clone(), vault_b.clone()],
            pda_seeds: Vec::<PdaSeed>::new(),
        };

    // Chain call for LP (mint new tokens for user_holding_lp)
    let mut pool_definition_lp_auth = pool_definition_lp.clone();
    pool_definition_lp_auth.is_authorized = true;

    let mut instruction_data = [0; 23];
    instruction_data[0] = 4;
    instruction_data[1..17].copy_from_slice(&delta_lp.to_le_bytes());
    let instruction_data = risc0_zkvm::serde::to_vec(&instruction_data).expect("Add liquidity: AMM Program expects valid token transfer instruction data");
    let call_token_lp = ChainedCall{
            program_id: pool_definition_lp.account.program_owner,
            instruction_data,
            pre_states: vec![pool_definition_lp_auth.clone(), user_holding_lp.clone()],
            pda_seeds: vec![compute_liquidity_token_pda_seed(pool.account_id.clone())]
        };

    chained_call.push(call_token_lp);
    chained_call.push(call_token_b);
    chained_call.push(call_token_a);

    let post_states = vec![
        AccountPostState::new(pool_post), 
        AccountPostState::new(pre_states[1].account.clone()),
        AccountPostState::new(pre_states[2].account.clone()),
        AccountPostState::new(pre_states[3].account.clone()),
        AccountPostState::new(pre_states[4].account.clone()),
        AccountPostState::new(pre_states[5].account.clone()),
        AccountPostState::new(pre_states[6].account.clone()),];

    (post_states, chained_call)

}

fn remove_liquidity(pre_states: &[AccountWithMetadata],
    amounts: &[u128]   
) -> (Vec<AccountPostState>, Vec<ChainedCall>)
{
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

    // Verify vaults are in fact vaults
    let pool_def_data = PoolDefinition::parse(&pool.account.data).expect("Remove liquidity: AMM Program expects a valid Pool Definition Account");

    if !pool_def_data.active {
        panic!("Pool is inactive");
    }

    if pool_def_data.liquidity_pool_id != pool_definition_lp.account_id {
        panic!("LP definition mismatch");
    }

    if vault_a.account_id != pool_def_data.vault_a_id {
        panic!("Vault A was not provided");
    }

    if vault_b.account_id != pool_def_data.vault_b_id {
        panic!("Vault B was not provided");
    }
    
    // Vault addresses do not need to be checked with PDA
    // calculation for setting authorization since stored
    // in the Pool Definition.
    let mut running_vault_a = vault_a.clone();
    let mut running_vault_b = vault_b.clone();
    running_vault_a.is_authorized = true;
    running_vault_b.is_authorized = true;

    if amount_min_a == 0 || amount_min_b == 0 {
        panic!("Minimum withdraw amount must be nonzero");
    }

    if amount_lp == 0 {
        panic!("Liquidity amount must be nonzero");
    }

    // 2. Compute withdrawal amounts
    let user_holding_lp_data = TokenHolding::parse(&user_holding_lp.account.data).expect("Remove liquidity: AMM Program expects a valid Token Account for liquidity token");
 
    if user_holding_lp_data.balance > pool_def_data.liquidity_pool_supply || user_holding_lp_data.definition_id != pool_def_data.liquidity_pool_id {
        panic!("Invalid liquidity account provided");
    }

    let withdraw_amount_a = (pool_def_data.reserve_a * amount_lp)/pool_def_data.liquidity_pool_supply;
    let withdraw_amount_b = (pool_def_data.reserve_b * amount_lp)/pool_def_data.liquidity_pool_supply;

    // 3. Validate and slippage check
    if withdraw_amount_a < amount_min_a {
        panic!("Insufficient minimal withdraw amount (Token A) provided for liquidity amount");
    }
    if withdraw_amount_b < amount_min_b {
        panic!("Insufficient minimal withdraw amount (Token B) provided for liquidity amount");
    }

    // 4. Calculate LP to reduce cap by
    let delta_lp : u128 = (pool_def_data.liquidity_pool_supply*amount_lp)/pool_def_data.liquidity_pool_supply;

    let active: bool = if pool_def_data.liquidity_pool_supply - delta_lp == 0 { false } else { true };

    // 5. Update pool account
    let mut pool_post = pool.account.clone();
    let pool_post_definition = PoolDefinition {
            definition_token_a_id: pool_def_data.definition_token_a_id.clone(),
            definition_token_b_id: pool_def_data.definition_token_b_id.clone(),
            vault_a_id: pool_def_data.vault_a_id.clone(),
            vault_b_id: pool_def_data.vault_b_id.clone(),
            liquidity_pool_id: pool_def_data.liquidity_pool_id.clone(),
            liquidity_pool_supply: pool_def_data.liquidity_pool_supply - delta_lp,
            reserve_a: pool_def_data.reserve_a - withdraw_amount_a,
            reserve_b: pool_def_data.reserve_b - withdraw_amount_b,
            fees: 0u128,
            active,  
    };

    pool_post.data = pool_post_definition.into_data().try_into().expect("Data too big");

    let mut chained_calls = Vec::new();

    //Chaincall for Token A withdraw
    let mut instruction: [u8;23] = [0; 23];
    instruction[0] = 1; // token transfer  
    instruction[1..17].copy_from_slice(&withdraw_amount_a.to_le_bytes());
    let instruction_data = risc0_zkvm::serde::to_vec(&instruction).expect("Remove liquidity: AMM Program expects valid token transfer instruction data");
    let call_token_a = ChainedCall{
            program_id: vault_a.account.program_owner,
            instruction_data,
            pre_states: vec![running_vault_a, user_holding_a.clone()],
            pda_seeds: vec![compute_vault_pda_seed(pool.account_id.clone(), pool_def_data.definition_token_a_id.clone())],
        };

    //Chaincall for Token B withdraw
    let mut instruction: [u8;23] = [0; 23];
    instruction[0] = 1; // token transfer   
    instruction[1..17].copy_from_slice(&withdraw_amount_b.to_le_bytes());
    let instruction_data = risc0_zkvm::serde::to_vec(&instruction).expect("Remove liquidity: AMM Program expects valid token transfer instruction data");
    let call_token_b = ChainedCall{
            program_id: vault_b.account.program_owner,
            instruction_data,
            pre_states: vec![running_vault_b, user_holding_b.clone()],
            pda_seeds: vec![compute_vault_pda_seed(pool.account_id.clone(), pool_def_data.definition_token_b_id.clone())],
        };

    //Chaincall for LP adjustment     
    let mut pool_definition_lp_auth = pool_definition_lp.clone();
    pool_definition_lp_auth.is_authorized = true;

    let mut instruction: [u8;23] = [0; 23];
    instruction[0] = 3; // token burn
    instruction[1..17].copy_from_slice(&delta_lp.to_le_bytes());
    let instruction_data = risc0_zkvm::serde::to_vec(&instruction).expect("Remove liquidity: AMM Program expects valid token transfer instruction data");
    let call_token_lp = ChainedCall{
            program_id: pool_definition_lp.account.program_owner,
            instruction_data,
            pre_states: vec![pool_definition_lp_auth.clone(), user_holding_lp.clone()],
            pda_seeds: vec![compute_liquidity_token_pda_seed(pool.account_id.clone())]
        };

    chained_calls.push(call_token_lp);
    chained_calls.push(call_token_b);
    chained_calls.push(call_token_a);
        
    let post_states = vec!
        [
        AccountPostState::new(pool_post.clone()), 
        AccountPostState::new(pre_states[1].account.clone()),
        AccountPostState::new(pre_states[2].account.clone()),
        AccountPostState::new(pre_states[3].account.clone()),
        AccountPostState::new(pre_states[4].account.clone()),
        AccountPostState::new(pre_states[5].account.clone()),
        AccountPostState::new(pre_states[6].account.clone())];

    (post_states, chained_calls)
}

#[cfg(test)]
mod tests {
    use nssa_core::{{account::{Account, AccountId, AccountWithMetadata}, program::ChainedCall, program::PdaSeed}, program::ProgramId};

    use crate::{PoolDefinition, TokenDefinition, TokenHolding, add_liquidity, new_definition, remove_liquidity, swap,
        compute_liquidity_token_pda, compute_liquidity_token_pda_seed, compute_pool_pda, compute_pool_pda_seed,
        compute_vault_pda, compute_vault_pda_seed};

    const TOKEN_PROGRAM_ID: ProgramId = [15;8];
    const AMM_PROGRAM_ID: ProgramId = [42;8];

    enum AccountEnum {
        UserHoldingB,
        UserHoldingA,
        VaultAUninit,
        VaultBUninit,
        VaultAInit,
        VaultBInit,
        VaultAInitHigh,
        VaultBInitHigh,
        VaultAInitLow,
        VaultBInitLow,
        VaultAInitZero,
        VaultBInitZero,
        VaultAWrongAccId,
        VaultBWrongAccId,
        PoolLPUninit,
        PoolLPInit,
        PoolLPWrongAccId,
        UserHoldingLPUninit,
        UserHoldingLPInit,
        PoolDefinitionUninit,
        PoolDefinitionInit,
        PoolDefinitionInitReserveAZero,
        PoolDefinitionInitReserveBZero,
        PoolDefinitionInitReserveALow,
        PoolDefinitionInitReserveBLow,
        PoolDefinitionUnauth,
        PoolDefinitionSwapTest1,
        PoolDefinitionSwapTest2,
        PoolDefinitionAddZeroLP,
        PoolDefinitionAddSuccessful,
        PoolDefinitionRemoveSuccessful,
        PoolDefinitionInactive,
        PoolDefinitionWrongId,
        VaultAWrongId,
        VaultBWrongId,
        PoolLPWrongId,
        PoolDefinitionActive,
    }

    enum BalanceEnum {
        VaultAReserveInit,
        VaultBReserveInit,
        VaultAReserveLow,
        VaultBReserveLow,
        VaultAReserveHigh,
        VaultBReserveHigh,
        UserTokenABal,
        UserTokenBBal,
        UserTokenLPBal,
        RemoveMinAmountA,
        RemoveMinAmountB,
        RemoveActualASuccessful,
        RemoveMinAmountBLow,
        RemoveMinAmountBAow,
        RemoveAmountLP,
        RemoveAmountLP1,
        AddMaxAmountALow,
        AddMaxAmountBLow,
        AddMaxAmountBHigh,
        AddMaxAmountA,
        AddMaxAmountb,
        AddMinAmountLP,
        VaultASwapTest1,
        VaultASwapTest2,
        VaultBSwapTest1,
        VaultBSwapTest2,
        MinAmountOut,
        VaultAAddSuccessful,
        VaultBAddSuccessful,
        AddSuccessfulAmountA,
        AddSuccessfulAmountB,
        VaultARemoveSuccessful,
        VaultBRemoveSuccessful,
    }

    fn helper_balance_constructor(selection: BalanceEnum) -> u128 {
        match selection {
            BalanceEnum::VaultAReserveInit => 1_000,
            BalanceEnum::VaultBReserveInit => 500,
            BalanceEnum::VaultAReserveLow => 10,
            BalanceEnum::VaultBReserveLow => 10,
            BalanceEnum::VaultAReserveHigh => 500_000,
            BalanceEnum::VaultBReserveHigh => 500_000,
            BalanceEnum::UserTokenABal => 1_000,
            BalanceEnum::UserTokenBBal => 500,
            BalanceEnum::UserTokenLPBal => 100,
            BalanceEnum::RemoveMinAmountA => 50,
            BalanceEnum::RemoveMinAmountB => 100,
            BalanceEnum::RemoveActualASuccessful => 100,
            BalanceEnum::RemoveMinAmountBLow => 50,
            BalanceEnum::RemoveMinAmountBAow => 10,
            BalanceEnum::RemoveAmountLP => 100,
            BalanceEnum::RemoveAmountLP1 => 30,
            BalanceEnum::AddMaxAmountA => 500,
            BalanceEnum::AddMaxAmountb => 200,
            BalanceEnum::AddMaxAmountBHigh => 20_000,
            BalanceEnum::AddMaxAmountALow => 10,
            BalanceEnum::AddMaxAmountBLow => 10,
            BalanceEnum::AddMinAmountLP => 20,
            BalanceEnum::VaultASwapTest1 => 1_500,
            BalanceEnum::VaultASwapTest2 => 715,
            BalanceEnum::VaultBSwapTest1 => 334,
            BalanceEnum::VaultBSwapTest2 => 700,
            BalanceEnum::MinAmountOut => 200,
            BalanceEnum::VaultAAddSuccessful => 1_400,
            BalanceEnum::VaultBAddSuccessful => 700,
            BalanceEnum::AddSuccessfulAmountA => 400,
            BalanceEnum::AddSuccessfulAmountB => 200,
            BalanceEnum::VaultARemoveSuccessful => 900,
            BalanceEnum::VaultBRemoveSuccessful => 450,
            _ => panic!("Invalid selection")
        }
    } 

    enum IdEnum {
        TokenADefinitionId,
        TokenBDefinitionId,
        TokenLPDefinitionId,
        UserTokenAId,
        UserTokenBId,
        UserTokenLPId,
        PoolDefinitionId,
        VaultAId,
        VaultBId,
    }

    enum ChainedCallsEnum {
        CcTokenAInitialization,
        CcTokenBInitialization,
        CcPoolLPInitiailization,
        CcSwapTokenATest1,
        CcSwapTokenBTest1,
        CcSwapTokenATest2,
        CcSwapTokenBTest2,
        CcAddTokenA,
        CcAddTokenB,
        CcAddPoolLP,
        CcRemoveTokenA,
        CcRemoveTokenB,
        CcRemovePoolLP,
        CcNewDefinitionTokenA,
        CcNewDefinitionTokenB,
        CcNewDefinitionLP,
    }

    fn helper_chained_call_constructor(selection: ChainedCallsEnum) -> ChainedCall {
        match selection {
            ChainedCallsEnum::CcTokenAInitialization => {
                let mut instruction: [u8;23] = [0; 23];
                instruction[0] = 1;      
                instruction[1..17].copy_from_slice(
                    &helper_balance_constructor(BalanceEnum::UserTokenABal)
                    .to_le_bytes());
                let instruction_data = risc0_zkvm::serde::to_vec(&instruction).expect("AMM Program expects valid transaction instruction data");
                ChainedCall{
                    program_id: TOKEN_PROGRAM_ID,
                    instruction_data,
                    pre_states: vec![
                            helper_account_constructor(AccountEnum::UserHoldingA),
                            helper_account_constructor(AccountEnum::VaultAUninit)],
                    pda_seeds: Vec::<PdaSeed>::new(),
                }
            }
            ChainedCallsEnum::CcTokenBInitialization => {
                let mut instruction: [u8;23] = [0; 23];
                instruction[0] = 1;      
                instruction[1..17].copy_from_slice(
                    &helper_balance_constructor(BalanceEnum::UserTokenBBal)
                    .to_le_bytes());
                let instruction_data = risc0_zkvm::serde::to_vec(&instruction).expect("AMM Program expects valid transaction instruction data");
                ChainedCall{
                    program_id: TOKEN_PROGRAM_ID,
                    instruction_data,
                    pre_states: vec![
                            helper_account_constructor(AccountEnum::UserHoldingB),
                            helper_account_constructor(AccountEnum::VaultBUninit)],
                    pda_seeds: Vec::<PdaSeed>::new(),
                }
            }
            ChainedCallsEnum::CcPoolLPInitiailization => {
                let mut instruction: [u8;23] = [0; 23];
                instruction[0] = 1;      
                instruction[1..17].copy_from_slice(
                    &helper_balance_constructor(BalanceEnum::UserTokenABal)
                    .to_le_bytes());
                let instruction_data = risc0_zkvm::serde::to_vec(&instruction).expect("AMM Program expects valid transaction instruction data");
                ChainedCall{
                    program_id: TOKEN_PROGRAM_ID,
                    instruction_data,
                    pre_states: vec![
                            helper_account_constructor(AccountEnum::PoolLPUninit),
                            helper_account_constructor(AccountEnum::UserHoldingLPUninit)],
                    pda_seeds: Vec::<PdaSeed>::new(),
                }
            }
            ChainedCallsEnum::CcSwapTokenATest1 => {
                let mut instruction_data: [u8;23] = [0; 23];
                instruction_data[0] = 1;      
                instruction_data[1..17].copy_from_slice(
                    &helper_balance_constructor(BalanceEnum::AddMaxAmountA)
                    .to_le_bytes());
                let instruction_data = risc0_zkvm::serde::to_vec(&instruction_data).expect("AMM Program expects valid transaction instruction data");
                ChainedCall{
                    program_id: TOKEN_PROGRAM_ID,
                    instruction_data,
                    pre_states: vec![
                            helper_account_constructor(AccountEnum::UserHoldingA),
                            helper_account_constructor(AccountEnum::VaultAInit)],
                    pda_seeds: Vec::<PdaSeed>::new(),
                }
            }
            ChainedCallsEnum::CcSwapTokenBTest1 => {
                let swap_amount: u128 = 166;

                let mut vault_b_auth = helper_account_constructor(AccountEnum::VaultBInit);
                vault_b_auth.is_authorized = true;

                let mut instruction: [u8;23] = [0; 23];
                instruction[0] = 1;      
                instruction[1..17].copy_from_slice(
                    &swap_amount
                    .to_le_bytes());
                let instruction_data = risc0_zkvm::serde::to_vec(&instruction).expect("AMM Program expects valid transaction instruction data");
                ChainedCall{
                    program_id: TOKEN_PROGRAM_ID,
                    instruction_data,
                    pre_states: vec![
                            vault_b_auth,
                            helper_account_constructor(AccountEnum::UserHoldingB)],
                    pda_seeds: vec![
                            compute_vault_pda_seed(helper_id_constructor(IdEnum::PoolDefinitionId),
                                            helper_id_constructor(IdEnum::TokenBDefinitionId)),
                    ],
                }
            }
            ChainedCallsEnum::CcSwapTokenATest2 => {
                let swap_amount: u128 = 285;

                let mut vault_a_auth = helper_account_constructor(AccountEnum::VaultAInit);
                vault_a_auth.is_authorized = true;

                let mut instruction_data: [u8;23] = [0; 23];
                instruction_data[0] = 1;      
                instruction_data[1..17].copy_from_slice(
                    &swap_amount
                    .to_le_bytes());
                let instruction_data = risc0_zkvm::serde::to_vec(&instruction_data).expect("AMM Program expects valid transaction instruction data");
                ChainedCall{
                    program_id: TOKEN_PROGRAM_ID,
                    instruction_data,
                    pre_states: vec![
                            vault_a_auth,
                            helper_account_constructor(AccountEnum::UserHoldingA),
                            ],
                    pda_seeds: vec![
                            compute_vault_pda_seed(helper_id_constructor(IdEnum::PoolDefinitionId),
                                            helper_id_constructor(IdEnum::TokenADefinitionId)),
                    ],
                }
            }
            ChainedCallsEnum::CcSwapTokenBTest2 => {
                let mut instruction: [u8;23] = [0; 23];
                instruction[0] = 1;      
                instruction[1..17].copy_from_slice(
                    &helper_balance_constructor(BalanceEnum::AddMaxAmountb)
                    .to_le_bytes());
                let instruction_data = risc0_zkvm::serde::to_vec(&instruction).expect("AMM Program expects valid transaction instruction data");
                ChainedCall{
                    program_id: TOKEN_PROGRAM_ID,
                    instruction_data,
                    pre_states: vec![
                            helper_account_constructor(AccountEnum::UserHoldingB),
                            helper_account_constructor(AccountEnum::VaultBInit)],
                    pda_seeds: Vec::<PdaSeed>::new(),
                }
            }
            ChainedCallsEnum::CcAddTokenA => {
                let mut instruction: [u8;23] = [0; 23];
                instruction[0] = 1;      
                instruction[1..17].copy_from_slice(
                    &helper_balance_constructor(BalanceEnum::AddSuccessfulAmountA)
                    .to_le_bytes());
                let instruction_data = risc0_zkvm::serde::to_vec(&instruction).expect("AMM Program expects valid transaction instruction data");
                ChainedCall{
                    program_id: TOKEN_PROGRAM_ID,
                    instruction_data,
                    pre_states: vec![
                            helper_account_constructor(AccountEnum::UserHoldingA),
                            helper_account_constructor(AccountEnum::VaultAInit)],
                    pda_seeds: Vec::<PdaSeed>::new(),
                }
            }
            ChainedCallsEnum::CcAddTokenB => {
                let mut instruction: [u8;23] = [0; 23];
                instruction[0] = 1;      
                instruction[1..17].copy_from_slice(
                    &helper_balance_constructor(BalanceEnum::AddSuccessfulAmountB)
                    .to_le_bytes());
                let instruction_data = risc0_zkvm::serde::to_vec(&instruction).expect("Swap Logic: AMM Program expects valid transaction instruction data");
                ChainedCall{
                    program_id: TOKEN_PROGRAM_ID,
                    instruction_data,
                    pre_states: vec![
                            helper_account_constructor(AccountEnum::UserHoldingB),
                            helper_account_constructor(AccountEnum::VaultBInit)],
                    pda_seeds: Vec::<PdaSeed>::new(),
                }
            }
            ChainedCallsEnum::CcAddPoolLP => {
                let mut pool_lp_auth = helper_account_constructor(AccountEnum::PoolLPInit);
                pool_lp_auth.is_authorized = true;

                let mut instruction: [u8;23] = [0; 23];
                instruction[0] = 4;      
                instruction[1..17].copy_from_slice(
                    &helper_balance_constructor(BalanceEnum::AddSuccessfulAmountA)
                    .to_le_bytes());
                let instruction_data = risc0_zkvm::serde::to_vec(&instruction).expect("Swap Logic: AMM Program expects valid transaction instruction data");
                ChainedCall{
                    program_id: TOKEN_PROGRAM_ID,
                    instruction_data,
                    pre_states: vec![
                            pool_lp_auth,
                            helper_account_constructor(AccountEnum::UserHoldingLPInit)],
                    pda_seeds: vec![compute_liquidity_token_pda_seed(
                                        helper_id_constructor(IdEnum::PoolDefinitionId))],
                }
            }
            ChainedCallsEnum::CcRemoveTokenA => {
                let mut vault_a_auth = helper_account_constructor(AccountEnum::VaultAInit);
                vault_a_auth.is_authorized = true;

                let mut instruction: [u8;23] = [0; 23];
                instruction[0] = 1;      
                instruction[1..17].copy_from_slice(
                    &helper_balance_constructor(BalanceEnum::RemoveActualASuccessful)
                    .to_le_bytes());
                let instruction_data = risc0_zkvm::serde::to_vec(&instruction).expect("AMM Program expects valid transaction instruction data");
                ChainedCall{
                    program_id: TOKEN_PROGRAM_ID,
                    instruction_data,
                    pre_states: vec![
                            vault_a_auth,
                            helper_account_constructor(AccountEnum::UserHoldingA),],
                    pda_seeds: vec![
                            compute_vault_pda_seed(helper_id_constructor(IdEnum::PoolDefinitionId),
                            helper_id_constructor(IdEnum::TokenADefinitionId)),
                    ],
                }
            }
            ChainedCallsEnum::CcRemoveTokenB => {
                let mut vault_b_auth = helper_account_constructor(AccountEnum::VaultBInit);
                vault_b_auth.is_authorized = true;

                let mut instruction: [u8;23] = [0; 23];
                instruction[0] = 1;      
                instruction[1..17].copy_from_slice(
                    &helper_balance_constructor(BalanceEnum::RemoveMinAmountBLow)
                    .to_le_bytes());
                let instruction_data = risc0_zkvm::serde::to_vec(&instruction).expect("AMM Program expects valid transaction instruction data");
                ChainedCall{
                    program_id: TOKEN_PROGRAM_ID,
                    instruction_data,
                    pre_states: vec![
                            vault_b_auth,
                            helper_account_constructor(AccountEnum::UserHoldingB),],
                    pda_seeds: vec![
                            compute_vault_pda_seed(helper_id_constructor(IdEnum::PoolDefinitionId),
                            helper_id_constructor(IdEnum::TokenBDefinitionId)),
                    ],
                }
            }
            ChainedCallsEnum::CcRemovePoolLP => {
                let mut pool_lp_auth = helper_account_constructor(AccountEnum::PoolLPInit);
                pool_lp_auth.is_authorized = true;

                let mut instruction: [u8;23] = [0; 23];
                instruction[0] = 3;      
                instruction[1..17].copy_from_slice(
                    &helper_balance_constructor(BalanceEnum::RemoveActualASuccessful)
                    .to_le_bytes());
                let instruction_data = risc0_zkvm::serde::to_vec(&instruction).expect("AMM Program expects valid transaction instruction data");
                ChainedCall{
                    program_id: TOKEN_PROGRAM_ID,
                    instruction_data,
                    pre_states: vec![
                            helper_account_constructor(AccountEnum::UserHoldingLPInit),
                            helper_account_constructor(AccountEnum::PoolLPInit),],
                    pda_seeds: vec![compute_liquidity_token_pda_seed(
                                        helper_id_constructor(IdEnum::PoolDefinitionId))],
                }
            }
            ChainedCallsEnum::CcNewDefinitionTokenA => {
                let mut instruction: [u8;23] = [0; 23];
                instruction[0] = 1;      
                instruction[1..17].copy_from_slice(
                    &helper_balance_constructor(BalanceEnum::AddSuccessfulAmountA)
                    .to_le_bytes());
                let instruction_data = risc0_zkvm::serde::to_vec(&instruction).expect("AMM Program expects valid transaction instruction data");
                ChainedCall{
                    program_id: TOKEN_PROGRAM_ID,
                    instruction_data,
                    pre_states: vec![
                            helper_account_constructor(AccountEnum::UserHoldingA),
                            helper_account_constructor(AccountEnum::VaultAInit)],
                    pda_seeds: Vec::<PdaSeed>::new(),
                }
            }
            ChainedCallsEnum::CcNewDefinitionTokenB => {
                let mut instruction: [u8;23] = [0; 23];
                instruction[0] = 1;      
                instruction[1..17].copy_from_slice(
                    &helper_balance_constructor(BalanceEnum::AddSuccessfulAmountB)
                    .to_le_bytes());
                let instruction_data = risc0_zkvm::serde::to_vec(&instruction).expect("Swap Logic: AMM Program expects valid transaction instruction data");
                ChainedCall{
                    program_id: TOKEN_PROGRAM_ID,
                    instruction_data,
                    pre_states: vec![
                            helper_account_constructor(AccountEnum::UserHoldingB),
                            helper_account_constructor(AccountEnum::VaultBInit)],
                    pda_seeds: Vec::<PdaSeed>::new(),
                }
            }
            ChainedCallsEnum::CcAddPoolLP => {
                let mut pool_lp_auth = helper_account_constructor(AccountEnum::PoolLPInit);
                pool_lp_auth.is_authorized = true;

                let mut instruction: [u8;23] = [0; 23];
                instruction[0] = 0;      
                instruction[1..17].copy_from_slice(
                    &helper_balance_constructor(BalanceEnum::AddSuccessfulAmountA)
                    .to_le_bytes());
                let instruction_data = risc0_zkvm::serde::to_vec(&instruction).expect("Swap Logic: AMM Program expects valid transaction instruction data");
                ChainedCall{
                    program_id: TOKEN_PROGRAM_ID,
                    instruction_data,
                    pre_states: vec![
                            pool_lp_auth,
                            helper_account_constructor(AccountEnum::UserHoldingLPInit)],
                    pda_seeds: vec![compute_liquidity_token_pda_seed(
                                        helper_id_constructor(IdEnum::PoolDefinitionId))],
                }
            }
            _ => panic!("Invalid selection")
        }
    }

    fn helper_id_constructor(selection: IdEnum) -> AccountId {

        match selection {
            IdEnum::TokenADefinitionId => AccountId::new([42;32]),
            IdEnum::TokenBDefinitionId => AccountId::new([43;32]),
            IdEnum::TokenLPDefinitionId => compute_liquidity_token_pda(AMM_PROGRAM_ID,
                                                helper_id_constructor(IdEnum::PoolDefinitionId),),
            IdEnum::UserTokenAId => AccountId::new([45;32]),
            IdEnum::UserTokenBId => AccountId::new([46;32]),
            IdEnum::UserTokenLPId => AccountId::new([47;32]),
            IdEnum::PoolDefinitionId => compute_pool_pda(AMM_PROGRAM_ID,
                                                helper_id_constructor(IdEnum::TokenADefinitionId),
                                                helper_id_constructor(IdEnum::TokenBDefinitionId)),
            IdEnum::VaultAId => compute_vault_pda(AMM_PROGRAM_ID,
                                                helper_id_constructor(IdEnum::PoolDefinitionId),
                                                helper_id_constructor(IdEnum::TokenADefinitionId)),
            IdEnum::VaultBId => compute_vault_pda(AMM_PROGRAM_ID,
                                                helper_id_constructor(IdEnum::PoolDefinitionId),
                                                helper_id_constructor(IdEnum::TokenBDefinitionId)),
            _ => panic!("Invalid selection")
        }
    }

    fn helper_account_constructor(selection: AccountEnum) -> AccountWithMetadata {
        
        match selection {
            AccountEnum::UserHoldingA => AccountWithMetadata {
                account: Account {
                    program_owner:  TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(
                        TokenHolding{
                            account_type: 1u8,
                            definition_id: helper_id_constructor(IdEnum::TokenADefinitionId),
                            balance: helper_balance_constructor(BalanceEnum::UserTokenABal),
                        }),
                        nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::UserTokenAId),
            },
            AccountEnum::UserHoldingB => AccountWithMetadata {
                    account: Account {
                        program_owner:  TOKEN_PROGRAM_ID,
                        balance: 0u128,
                        data: TokenHolding::into_data(
                            TokenHolding{
                                account_type: 1u8,
                                definition_id: helper_id_constructor(IdEnum::TokenBDefinitionId),
                                balance: helper_balance_constructor(BalanceEnum::UserTokenBBal),
                            }),
                        nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::UserTokenBId),
            },
            AccountEnum::VaultAUninit => AccountWithMetadata {
                account: Account {
                    program_owner:  TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(
                        TokenHolding{
                            account_type: 1u8,
                            definition_id: helper_id_constructor(IdEnum::TokenADefinitionId),
                            balance: 0,
                        }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::VaultAId),
            },
            AccountEnum::VaultBUninit => AccountWithMetadata {
                account: Account {
                    program_owner:  TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(
                        TokenHolding{
                            account_type: 1u8,
                            definition_id: helper_id_constructor(IdEnum::TokenBDefinitionId),
                            balance: 0,
                        }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::VaultBId),
            },
            AccountEnum::VaultAInit => AccountWithMetadata {
                account: Account {
                    program_owner:  TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(
                        TokenHolding{
                            account_type: 1u8,
                            definition_id: helper_id_constructor(IdEnum::TokenADefinitionId),
                            balance: helper_balance_constructor(BalanceEnum::VaultAReserveInit),
                        }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::VaultAId),
            },
            AccountEnum::VaultBInit => AccountWithMetadata {
                account: Account {
                    program_owner:  TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(
                        TokenHolding{
                            account_type: 1u8,
                            definition_id: helper_id_constructor(IdEnum::TokenBDefinitionId),
                            balance: helper_balance_constructor(BalanceEnum::VaultBReserveInit),
                        }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::VaultBId),
            },
            AccountEnum::VaultAInitHigh => AccountWithMetadata {
                account: Account {
                    program_owner:  TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(
                        TokenHolding{
                            account_type: 1u8,
                            definition_id: helper_id_constructor(IdEnum::TokenADefinitionId),
                            balance: helper_balance_constructor(BalanceEnum::VaultAReserveHigh),
                        }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::VaultAId),
            },
            AccountEnum::VaultBInitHigh => AccountWithMetadata {
                account: Account {
                    program_owner:  TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(
                        TokenHolding{
                            account_type: 1u8,
                            definition_id: helper_id_constructor(IdEnum::TokenBDefinitionId),
                            balance: helper_balance_constructor(BalanceEnum::VaultBReserveHigh),
                        }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::VaultBId),
            },
            AccountEnum::VaultAInitLow => AccountWithMetadata {
                account: Account {
                    program_owner:  TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(
                        TokenHolding{
                            account_type: 1u8,
                            definition_id: helper_id_constructor(IdEnum::TokenADefinitionId),
                            balance: helper_balance_constructor(BalanceEnum::VaultAReserveLow),
                        }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::VaultAId),
            },
            AccountEnum::VaultBInitLow => AccountWithMetadata {
                account: Account {
                    program_owner:  TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(
                        TokenHolding{
                            account_type: 1u8,
                            definition_id: helper_id_constructor(IdEnum::TokenBDefinitionId),
                            balance: helper_balance_constructor(BalanceEnum::VaultBReserveLow),
                        }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::VaultBId),
            },
            AccountEnum::VaultAInitZero => AccountWithMetadata {
                account: Account {
                    program_owner:  TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(
                        TokenHolding{
                            account_type: 1u8,
                            definition_id: helper_id_constructor(IdEnum::TokenADefinitionId),
                            balance: 0,
                        }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::VaultAId),
            },
            AccountEnum::VaultBInitZero => AccountWithMetadata {
                account: Account {
                    program_owner:  TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(
                        TokenHolding{
                            account_type: 1u8,
                            definition_id: helper_id_constructor(IdEnum::TokenBDefinitionId),
                            balance: 0,
                        }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::VaultBId),
            },
            AccountEnum::VaultAWrongAccId => AccountWithMetadata {
                account: Account {
                    program_owner:  TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(
                        TokenHolding{
                            account_type: 1u8,
                            definition_id: helper_id_constructor(IdEnum::TokenADefinitionId),
                            balance: helper_balance_constructor(BalanceEnum::VaultAReserveInit),
                        }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::VaultBId),
            },
            AccountEnum::VaultBWrongAccId => AccountWithMetadata {
                account: Account {
                    program_owner:  TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(
                        TokenHolding{
                            account_type: 1u8,
                            definition_id: helper_id_constructor(IdEnum::TokenBDefinitionId),
                            balance: helper_balance_constructor(BalanceEnum::VaultBReserveInit),
                        }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::VaultAId),
            },
            AccountEnum::PoolLPUninit => AccountWithMetadata {
                account: Account {
                    program_owner:  TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenDefinition::into_data(
                        TokenDefinition{
                            account_type: 0u8,
                            name: [1;6],
                            total_supply: 0u128,
                        }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::TokenLPDefinitionId),
            },
            AccountEnum::PoolLPInit => AccountWithMetadata {
                account: Account {
                    program_owner:  TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenDefinition::into_data(
                        TokenDefinition{
                            account_type: 0u8,
                            name: [1;6],
                            total_supply: helper_balance_constructor(BalanceEnum::VaultAReserveInit),
                        }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::TokenLPDefinitionId),
            },
            AccountEnum::PoolLPWrongAccId => AccountWithMetadata {
              account: Account {
                    program_owner:  TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenDefinition::into_data(
                        TokenDefinition{
                            account_type: 0u8,
                            name: [1;6],
                            total_supply: helper_balance_constructor(BalanceEnum::VaultAReserveInit),
                        }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::VaultAId),
            },
            AccountEnum::UserHoldingLPUninit => AccountWithMetadata {
                account: Account {
                    program_owner:  TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(
                        TokenHolding{
                            account_type: 1u8,
                            definition_id: helper_id_constructor(IdEnum::TokenLPDefinitionId),
                            balance: 0,
                        }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::UserTokenLPId),
            },
            AccountEnum::UserHoldingLPInit => AccountWithMetadata {
                account: Account {
                    program_owner:  TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(
                        TokenHolding{
                            account_type: 1u8,
                            definition_id: helper_id_constructor(IdEnum::TokenLPDefinitionId),
                            balance: helper_balance_constructor(BalanceEnum::UserTokenLPBal),
                        }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::UserTokenLPId),
            },
            AccountEnum::PoolDefinitionUninit => AccountWithMetadata {
                account: Account::default(),
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::PoolDefinitionId),
            },
            AccountEnum::PoolDefinitionInit => AccountWithMetadata {
                account: Account {
                        program_owner:  ProgramId::default(),
                        balance: 0u128,
                        data: PoolDefinition::into_data(
                        PoolDefinition {
                            definition_token_a_id: helper_id_constructor(IdEnum::TokenADefinitionId),
                            definition_token_b_id: helper_id_constructor(IdEnum::TokenBDefinitionId),
                            vault_a_id: helper_id_constructor(IdEnum::VaultAId),
                            vault_b_id: helper_id_constructor(IdEnum::VaultBId),
                            liquidity_pool_id: helper_id_constructor(IdEnum::TokenLPDefinitionId),
                            liquidity_pool_supply: helper_balance_constructor(BalanceEnum::VaultAReserveInit),
                            reserve_a: helper_balance_constructor(BalanceEnum::VaultAReserveInit),
                            reserve_b: helper_balance_constructor(BalanceEnum::VaultBReserveInit),
                            fees: 0u128,
                            active: true,
                        }),
                        nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::PoolDefinitionId),
            },
            AccountEnum::PoolDefinitionInitReserveAZero => AccountWithMetadata {
                account: Account {
                        program_owner:  ProgramId::default(),
                        balance: 0u128,
                        data: PoolDefinition::into_data(
                        PoolDefinition {
                            definition_token_a_id: helper_id_constructor(IdEnum::TokenADefinitionId),
                            definition_token_b_id: helper_id_constructor(IdEnum::TokenBDefinitionId),
                            vault_a_id: helper_id_constructor(IdEnum::VaultAId),
                            vault_b_id: helper_id_constructor(IdEnum::VaultBId),
                            liquidity_pool_id: helper_id_constructor(IdEnum::TokenLPDefinitionId),
                            liquidity_pool_supply: helper_balance_constructor(BalanceEnum::VaultAReserveInit),
                            reserve_a: 0,
                            reserve_b: helper_balance_constructor(BalanceEnum::VaultBReserveInit),
                            fees: 0u128,
                            active: true,
                        }),
                        nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::PoolDefinitionId),
            },
            AccountEnum::PoolDefinitionInitReserveBZero => AccountWithMetadata {
                account: Account {
                        program_owner:  ProgramId::default(),
                        balance: 0u128,
                        data: PoolDefinition::into_data(
                        PoolDefinition {
                            definition_token_a_id: helper_id_constructor(IdEnum::TokenADefinitionId),
                            definition_token_b_id: helper_id_constructor(IdEnum::TokenBDefinitionId),
                            vault_a_id: helper_id_constructor(IdEnum::VaultAId),
                            vault_b_id: helper_id_constructor(IdEnum::VaultBId),
                            liquidity_pool_id: helper_id_constructor(IdEnum::TokenLPDefinitionId),
                            liquidity_pool_supply: helper_balance_constructor(BalanceEnum::VaultAReserveInit),
                            reserve_a: helper_balance_constructor(BalanceEnum::VaultAReserveInit),
                            reserve_b: 0,
                            fees: 0u128,
                            active: true,
                        }),
                        nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::PoolDefinitionId),
            },
            AccountEnum::PoolDefinitionInitReserveALow => AccountWithMetadata {
                account: Account {
                        program_owner:  ProgramId::default(),
                        balance: 0u128,
                        data: PoolDefinition::into_data(
                        PoolDefinition {
                            definition_token_a_id: helper_id_constructor(IdEnum::TokenADefinitionId),
                            definition_token_b_id: helper_id_constructor(IdEnum::TokenBDefinitionId),
                            vault_a_id: helper_id_constructor(IdEnum::VaultAId),
                            vault_b_id: helper_id_constructor(IdEnum::VaultBId),
                            liquidity_pool_id: helper_id_constructor(IdEnum::TokenLPDefinitionId),
                            liquidity_pool_supply: helper_balance_constructor(BalanceEnum::VaultAReserveLow),
                            reserve_a: helper_balance_constructor(BalanceEnum::VaultAReserveLow),
                            reserve_b: helper_balance_constructor(BalanceEnum::VaultBReserveHigh),
                            fees: 0u128,
                            active: true,
                        }),
                        nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::PoolDefinitionId),
            },
            AccountEnum::PoolDefinitionInitReserveBLow => AccountWithMetadata {
                account: Account {
                        program_owner:  ProgramId::default(),
                        balance: 0u128,
                        data: PoolDefinition::into_data(
                        PoolDefinition {
                            definition_token_a_id: helper_id_constructor(IdEnum::TokenADefinitionId),
                            definition_token_b_id: helper_id_constructor(IdEnum::TokenBDefinitionId),
                            vault_a_id: helper_id_constructor(IdEnum::VaultAId),
                            vault_b_id: helper_id_constructor(IdEnum::VaultBId),
                            liquidity_pool_id: helper_id_constructor(IdEnum::TokenLPDefinitionId),
                            liquidity_pool_supply: helper_balance_constructor(BalanceEnum::VaultAReserveHigh),
                            reserve_a: helper_balance_constructor(BalanceEnum::VaultAReserveHigh),
                            reserve_b: helper_balance_constructor(BalanceEnum::VaultBReserveLow),
                            fees: 0u128,
                            active: true,
                        }),
                        nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::PoolDefinitionId),
            },
            AccountEnum::PoolDefinitionUnauth => AccountWithMetadata {
                account: Account {
                        program_owner:  ProgramId::default(),
                        balance: 0u128,
                        data: PoolDefinition::into_data(
                        PoolDefinition {
                            definition_token_a_id: helper_id_constructor(IdEnum::TokenADefinitionId),
                            definition_token_b_id: helper_id_constructor(IdEnum::TokenBDefinitionId),
                            vault_a_id: helper_id_constructor(IdEnum::VaultAId),
                            vault_b_id: helper_id_constructor(IdEnum::VaultBId),
                            liquidity_pool_id: helper_id_constructor(IdEnum::TokenLPDefinitionId),
                            liquidity_pool_supply: helper_balance_constructor(BalanceEnum::VaultAReserveInit),
                            reserve_a: helper_balance_constructor(BalanceEnum::VaultAReserveInit),
                            reserve_b: helper_balance_constructor(BalanceEnum::VaultBReserveInit),
                            fees: 0u128,
                            active: true,
                        }),
                        nonce: 0,
                },
                is_authorized: false,
                account_id: helper_id_constructor(IdEnum::PoolDefinitionId),
            },
            AccountEnum::PoolDefinitionSwapTest1 => AccountWithMetadata {
                account: Account {
                        program_owner:  ProgramId::default(),
                        balance: 0u128,
                        data: PoolDefinition::into_data(
                        PoolDefinition {
                            definition_token_a_id: helper_id_constructor(IdEnum::TokenADefinitionId),
                            definition_token_b_id: helper_id_constructor(IdEnum::TokenBDefinitionId),
                            vault_a_id: helper_id_constructor(IdEnum::VaultAId),
                            vault_b_id: helper_id_constructor(IdEnum::VaultBId),
                            liquidity_pool_id: helper_id_constructor(IdEnum::TokenLPDefinitionId),
                            liquidity_pool_supply: helper_balance_constructor(BalanceEnum::VaultAReserveInit),
                            reserve_a: helper_balance_constructor(BalanceEnum::VaultASwapTest1),
                            reserve_b: helper_balance_constructor(BalanceEnum::VaultBSwapTest1),
                            fees: 0u128,
                            active: true,
                        }),
                        nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::PoolDefinitionId),
            },
            AccountEnum::PoolDefinitionSwapTest2 => AccountWithMetadata {
                account: Account {
                        program_owner:  ProgramId::default(),
                        balance: 0u128,
                        data: PoolDefinition::into_data(
                        PoolDefinition {
                            definition_token_a_id: helper_id_constructor(IdEnum::TokenADefinitionId),
                            definition_token_b_id: helper_id_constructor(IdEnum::TokenBDefinitionId),
                            vault_a_id: helper_id_constructor(IdEnum::VaultAId),
                            vault_b_id: helper_id_constructor(IdEnum::VaultBId),
                            liquidity_pool_id: helper_id_constructor(IdEnum::TokenLPDefinitionId),
                            liquidity_pool_supply: helper_balance_constructor(BalanceEnum::VaultAReserveInit),
                            reserve_a: helper_balance_constructor(BalanceEnum::VaultASwapTest2),
                            reserve_b: helper_balance_constructor(BalanceEnum::VaultBSwapTest2),
                            fees: 0u128,
                            active: true,
                        }),
                        nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::PoolDefinitionId),
            },
            AccountEnum::PoolDefinitionAddZeroLP => AccountWithMetadata {
                account: Account {
                        program_owner:  ProgramId::default(),
                        balance: 0u128,
                        data: PoolDefinition::into_data(
                        PoolDefinition {
                            definition_token_a_id: helper_id_constructor(IdEnum::TokenADefinitionId),
                            definition_token_b_id: helper_id_constructor(IdEnum::TokenBDefinitionId),
                            vault_a_id: helper_id_constructor(IdEnum::VaultAId),
                            vault_b_id: helper_id_constructor(IdEnum::VaultBId),
                            liquidity_pool_id: helper_id_constructor(IdEnum::TokenLPDefinitionId),
                            liquidity_pool_supply: helper_balance_constructor(BalanceEnum::VaultAReserveLow),
                            reserve_a: helper_balance_constructor(BalanceEnum::VaultAReserveInit),
                            reserve_b: helper_balance_constructor(BalanceEnum::VaultBReserveInit),
                            fees: 0u128,
                            active: true,
                        }),
                        nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::PoolDefinitionId),
            },
            AccountEnum::PoolDefinitionAddSuccessful => AccountWithMetadata {
                account: Account {
                        program_owner:  ProgramId::default(),
                        balance: 0u128,
                        data: PoolDefinition::into_data(
                        PoolDefinition {
                            definition_token_a_id: helper_id_constructor(IdEnum::TokenADefinitionId),
                            definition_token_b_id: helper_id_constructor(IdEnum::TokenBDefinitionId),
                            vault_a_id: helper_id_constructor(IdEnum::VaultAId),
                            vault_b_id: helper_id_constructor(IdEnum::VaultBId),
                            liquidity_pool_id: helper_id_constructor(IdEnum::TokenLPDefinitionId),
                            liquidity_pool_supply: helper_balance_constructor(BalanceEnum::VaultAAddSuccessful),
                            reserve_a: helper_balance_constructor(BalanceEnum::VaultAAddSuccessful),
                            reserve_b: helper_balance_constructor(BalanceEnum::VaultBAddSuccessful),
                            fees: 0u128,
                            active: true,
                        }),
                        nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::PoolDefinitionId),
            },
            AccountEnum::PoolDefinitionRemoveSuccessful => AccountWithMetadata {
                account: Account {
                        program_owner:  ProgramId::default(),
                        balance: 0u128,
                        data: PoolDefinition::into_data(
                        PoolDefinition {
                            definition_token_a_id: helper_id_constructor(IdEnum::TokenADefinitionId),
                            definition_token_b_id: helper_id_constructor(IdEnum::TokenBDefinitionId),
                            vault_a_id: helper_id_constructor(IdEnum::VaultAId),
                            vault_b_id: helper_id_constructor(IdEnum::VaultBId),
                            liquidity_pool_id: helper_id_constructor(IdEnum::TokenLPDefinitionId),
                            liquidity_pool_supply: helper_balance_constructor(BalanceEnum::VaultARemoveSuccessful),
                            reserve_a: helper_balance_constructor(BalanceEnum::VaultARemoveSuccessful),
                            reserve_b: helper_balance_constructor(BalanceEnum::VaultBRemoveSuccessful),
                            fees: 0u128,
                            active: true,
                        }),
                        nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::PoolDefinitionId),
            },
            AccountEnum::PoolDefinitionInactive => AccountWithMetadata {
                account: Account {
                        program_owner:  ProgramId::default(),
                        balance: 0u128,
                        data: PoolDefinition::into_data(
                        PoolDefinition {
                            definition_token_a_id: helper_id_constructor(IdEnum::TokenADefinitionId),
                            definition_token_b_id: helper_id_constructor(IdEnum::TokenBDefinitionId),
                            vault_a_id: helper_id_constructor(IdEnum::VaultAId),
                            vault_b_id: helper_id_constructor(IdEnum::VaultBId),
                            liquidity_pool_id: helper_id_constructor(IdEnum::TokenLPDefinitionId),
                            liquidity_pool_supply: helper_balance_constructor(BalanceEnum::VaultAReserveInit),
                            reserve_a: helper_balance_constructor(BalanceEnum::VaultAReserveInit),
                            reserve_b: helper_balance_constructor(BalanceEnum::VaultBReserveInit),
                            fees: 0u128,
                            active: false,
                        }),
                        nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::PoolDefinitionId),
            },
            AccountEnum::PoolDefinitionWrongId => AccountWithMetadata {
                account: Account {
                        program_owner:  ProgramId::default(),
                        balance: 0u128,
                        data: PoolDefinition::into_data(
                        PoolDefinition {
                            definition_token_a_id: helper_id_constructor(IdEnum::TokenADefinitionId),
                            definition_token_b_id: helper_id_constructor(IdEnum::TokenBDefinitionId),
                            vault_a_id: helper_id_constructor(IdEnum::VaultAId),
                            vault_b_id: helper_id_constructor(IdEnum::VaultBId),
                            liquidity_pool_id: helper_id_constructor(IdEnum::TokenLPDefinitionId),
                            liquidity_pool_supply: helper_balance_constructor(BalanceEnum::VaultAReserveInit),
                            reserve_a: helper_balance_constructor(BalanceEnum::VaultAReserveInit),
                            reserve_b: helper_balance_constructor(BalanceEnum::VaultBReserveInit),
                            fees: 0u128,
                            active: false,
                        }),
                        nonce: 0,
                },
                is_authorized: true,
                account_id: AccountId::new([4;32]),
            },
            AccountEnum::VaultAWrongId => AccountWithMetadata {
                account: Account {
                    program_owner:  TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(
                        TokenHolding{
                            account_type: 1u8,
                            definition_id: helper_id_constructor(IdEnum::TokenADefinitionId),
                            balance: helper_balance_constructor(BalanceEnum::VaultAReserveInit),
                        }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: AccountId::new([4;32]),
            },
            AccountEnum::VaultBWrongId => AccountWithMetadata {
                account: Account {
                    program_owner:  TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(
                        TokenHolding{
                            account_type: 1u8,
                            definition_id: helper_id_constructor(IdEnum::TokenBDefinitionId),
                            balance: helper_balance_constructor(BalanceEnum::VaultBReserveInit),
                        }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: AccountId::new([4;32]),
            },
            AccountEnum::PoolLPWrongId => AccountWithMetadata {
                account: Account {
                    program_owner:  TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenDefinition::into_data(
                        TokenDefinition{
                            account_type: 0u8,
                            name: [1;6],
                            total_supply: helper_balance_constructor(BalanceEnum::VaultAReserveInit),
                        }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: AccountId::new([4;32]),
            },
            AccountEnum::PoolDefinitionActive => AccountWithMetadata {
                account: Account {
                        program_owner:  ProgramId::default(),
                        balance: 0u128,
                        data: PoolDefinition::into_data(
                        PoolDefinition {
                            definition_token_a_id: helper_id_constructor(IdEnum::TokenADefinitionId),
                            definition_token_b_id: helper_id_constructor(IdEnum::TokenBDefinitionId),
                            vault_a_id: helper_id_constructor(IdEnum::VaultAId),
                            vault_b_id: helper_id_constructor(IdEnum::VaultBId),
                            liquidity_pool_id: helper_id_constructor(IdEnum::TokenLPDefinitionId),
                            liquidity_pool_supply: helper_balance_constructor(BalanceEnum::VaultAReserveInit),
                            reserve_a: helper_balance_constructor(BalanceEnum::VaultAReserveInit),
                            reserve_b: helper_balance_constructor(BalanceEnum::VaultBReserveInit),
                            fees: 0u128,
                            active: true,
                        }),
                        nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::PoolDefinitionId),
            },
            _ => panic!("Invalid selection"),
        }
    }

    #[test]
    fn test_pool_pda_produces_unique_id_for_token_pair() {
        //compute_pool_pda(amm_program_id: ProgramId, definition_token_a_id: AccountId, definition_token_b_id: AccountId)
        assert!(compute_pool_pda(AMM_PROGRAM_ID,
                helper_id_constructor(IdEnum::TokenADefinitionId),
                helper_id_constructor(IdEnum::TokenBDefinitionId)) ==
                compute_pool_pda(AMM_PROGRAM_ID,
                helper_id_constructor(IdEnum::TokenBDefinitionId),
                helper_id_constructor(IdEnum::TokenADefinitionId)));
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]    
    fn test_call_new_definition_with_invalid_number_of_accounts_1() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionUninit),
                ];
        let _post_states = new_definition(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::VaultAReserveInit),
                    helper_balance_constructor(BalanceEnum::VaultBReserveInit)],
                    AMM_PROGRAM_ID,
                    );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_new_definition_with_invalid_number_of_accounts_2() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                ];
        let _post_states = new_definition(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::VaultAReserveInit),
                    helper_balance_constructor(BalanceEnum::VaultBReserveInit)],
                    AMM_PROGRAM_ID,
                    );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_new_definition__with_invalid_number_of_accounts_3() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                ];
        let _post_states = new_definition(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::VaultAReserveInit),
                    helper_balance_constructor(BalanceEnum::VaultBReserveInit)],
                    AMM_PROGRAM_ID,
                    );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_new_definition__with_invalid_number_of_accounts_4() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::PoolLPInit),
                ];
        let _post_states = new_definition(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::VaultAReserveInit),
                    helper_balance_constructor(BalanceEnum::VaultBReserveInit)],
                    AMM_PROGRAM_ID,
                    );
    }
 
    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_new_definition__with_invalid_number_of_accounts_5() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::PoolLPInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                ];
        let _post_states = new_definition(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::VaultAReserveInit),
                    helper_balance_constructor(BalanceEnum::VaultBReserveInit)],
                    AMM_PROGRAM_ID,
                    );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_new_definition_with_invalid_number_of_accounts_6() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::PoolLPInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                ];
        let _post_states = new_definition(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::VaultAReserveInit),
                    helper_balance_constructor(BalanceEnum::VaultBReserveInit)],
                    AMM_PROGRAM_ID,
                    );
    }

    #[should_panic(expected = "Invalid number of input balances")]
    #[test]
    fn test_call_new_definition_with_invalid_number_of_balances() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::PoolLPInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                helper_account_constructor(AccountEnum::UserHoldingLPUninit),
                ];
        let _post_states = new_definition(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::VaultAReserveInit),],
                    AMM_PROGRAM_ID,
                    );
    }   

    #[should_panic(expected = "Balances must be nonzero")]
    #[test]
    fn test_call_new_definition_with_zero_balance_1() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::PoolLPInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                helper_account_constructor(AccountEnum::UserHoldingLPUninit),
                ];
        let _post_states = new_definition(&pre_states, 
                    &[0,
                    helper_balance_constructor(BalanceEnum::VaultBReserveInit),],
                    AMM_PROGRAM_ID,
                    );
    }   

    #[should_panic(expected = "Balances must be nonzero")]
    #[test]
    fn test_call_new_definition_with_zero_balance_2() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::PoolLPInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                helper_account_constructor(AccountEnum::UserHoldingLPUninit),
                ];
        let _post_states = new_definition(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::VaultAReserveInit),
                    0],
                    AMM_PROGRAM_ID,
                    );
    }    

    #[should_panic(expected = "Cannot set up a swap for a token with itself")]
    #[test]
    fn test_call_new_definition_same_token_definition() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::PoolLPInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingLPUninit),
                ];
        let _post_states = new_definition(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::VaultAReserveInit),
                    helper_balance_constructor(BalanceEnum::VaultBReserveInit),],
                    AMM_PROGRAM_ID,
                    );
    }

    #[should_panic(expected = "Liquidity pool Token Definition Account ID does not match PDA")]
    #[test]
    fn test_call_new_definition_wrong_liquidity_id() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::PoolLPWrongId),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                helper_account_constructor(AccountEnum::UserHoldingLPUninit),
                ];
        let _post_states = new_definition(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::VaultAReserveInit),
                    helper_balance_constructor(BalanceEnum::VaultBReserveInit),],
                    AMM_PROGRAM_ID,
                    );
    }    

    #[should_panic(expected = "Pool Definition Account ID does not match PDA")]
    #[test]
    fn test_call_new_definition_wrong_pool_id() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionWrongId),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::PoolLPInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                helper_account_constructor(AccountEnum::UserHoldingLPUninit),
                ];
        let _post_states = new_definition(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::VaultAReserveInit),
                    helper_balance_constructor(BalanceEnum::VaultBReserveInit),],
                    AMM_PROGRAM_ID,
                    );
    }    

    #[should_panic(expected = "Vault ID does not match PDA")]
    #[test]
    fn test_call_new_definition_wrong_vault_id_1() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAWrongId),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::PoolLPInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                helper_account_constructor(AccountEnum::UserHoldingLPUninit),
                ];
        let _post_states = new_definition(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::VaultAReserveInit),
                    helper_balance_constructor(BalanceEnum::VaultBReserveInit),],
                    AMM_PROGRAM_ID,
                    );
    }   

    #[should_panic(expected = "Vault ID does not match PDA")]
    #[test]
    fn test_call_new_definition_wrong_vault_id_2() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBWrongId),
                helper_account_constructor(AccountEnum::PoolLPInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                helper_account_constructor(AccountEnum::UserHoldingLPUninit),
                ];
        let _post_states = new_definition(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::VaultAReserveInit),
                    helper_balance_constructor(BalanceEnum::VaultBReserveInit),],
                    AMM_PROGRAM_ID,
                    );
    }   

    #[should_panic(expected = "Cannot initialize an active Pool Definition")]
    #[test]
    fn test_call_new_definition_cannot_initialize_active_pool() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionActive),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::PoolLPInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                helper_account_constructor(AccountEnum::UserHoldingLPUninit),
                ];
        let _post_states = new_definition(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::VaultAReserveInit),
                    helper_balance_constructor(BalanceEnum::VaultBReserveInit),],
                    AMM_PROGRAM_ID,
                    );
    }

    #[should_panic(expected = "Cannot initialize an active Pool Definition")]
    #[test]
    fn test_call_new_definition_chain_call_successful() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionActive),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::PoolLPInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                helper_account_constructor(AccountEnum::UserHoldingLPUninit),
                ];
        let (post_states, chained_calls) = new_definition(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::VaultAReserveInit),
                    helper_balance_constructor(BalanceEnum::VaultBReserveInit),],
                    AMM_PROGRAM_ID,
                    );
    
        let pool_post = post_states[0].clone();

        assert!(helper_account_constructor(AccountEnum::PoolDefinitionAddSuccessful).account ==
                    *pool_post.account());

        let chained_call_lp = chained_calls[0].clone();
        let chained_call_b = chained_calls[1].clone();
        let chained_call_a = chained_calls[2].clone();

        assert!(chained_call_a == helper_chained_call_constructor(ChainedCallsEnum::CcNewDefinitionTokenA));
        assert!(chained_call_b == helper_chained_call_constructor(ChainedCallsEnum::CcNewDefinitionTokenB));
        assert!(chained_call_lp == helper_chained_call_constructor(ChainedCallsEnum::CcNewDefinitionLP));
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_remove_liquidity_with_invalid_number_of_accounts_2() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                ];
        let _post_states = remove_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::RemoveAmountLP), 
                    helper_balance_constructor(BalanceEnum::RemoveMinAmountA),
                    helper_balance_constructor(BalanceEnum::RemoveMinAmountB)],
                    );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_remove_liquidity_with_invalid_number_of_accounts_3() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                ];
        let _post_states = remove_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::RemoveAmountLP), 
                    helper_balance_constructor(BalanceEnum::RemoveMinAmountA),
                    helper_balance_constructor(BalanceEnum::RemoveMinAmountB)],
                    );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_remove_liquidity_with_invalid_number_of_accounts_4() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::PoolLPInit),
                ];
        let _post_states = remove_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::RemoveAmountLP), 
                    helper_balance_constructor(BalanceEnum::RemoveMinAmountA),
                    helper_balance_constructor(BalanceEnum::RemoveMinAmountB)],
                    );
    }
 
    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_remove_liquidity_with_invalid_number_of_accounts_5() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::PoolLPInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                ];
        let _post_states = remove_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::RemoveAmountLP), 
                    helper_balance_constructor(BalanceEnum::RemoveMinAmountA),
                    helper_balance_constructor(BalanceEnum::RemoveMinAmountB)],
                    );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_remove_liquidity_with_invalid_number_of_accounts_6() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::PoolLPInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                ];
        let _post_states = remove_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::RemoveAmountLP), 
                    helper_balance_constructor(BalanceEnum::RemoveMinAmountA),
                    helper_balance_constructor(BalanceEnum::RemoveMinAmountB)],
                    );
    }

    #[should_panic(expected = "Vault A was not provided")]
    #[test]
    fn test_call_remove_liquidity_vault_a_omitted() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAWrongAccId),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::PoolLPInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                helper_account_constructor(AccountEnum::UserHoldingLPInit),
                ];
        let _post_states = remove_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::RemoveAmountLP), 
                    helper_balance_constructor(BalanceEnum::RemoveMinAmountA),
                    helper_balance_constructor(BalanceEnum::RemoveMinAmountB)],
                    );
    }
    
    #[should_panic(expected = "Vault B was not provided")]
    #[test]
    fn test_call_remove_liquidity_vault_b_omitted() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBWrongAccId),
                helper_account_constructor(AccountEnum::PoolLPInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                helper_account_constructor(AccountEnum::UserHoldingLPInit),
                ];
        let _post_states = remove_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::RemoveAmountLP), 
                    helper_balance_constructor(BalanceEnum::RemoveMinAmountA),
                    helper_balance_constructor(BalanceEnum::RemoveMinAmountB)],
                    );
    }
    
    #[should_panic(expected = "LP definition mismatch")]
    #[test]
    fn test_call_remove_liquidity_lp_def_mismatch() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::PoolLPWrongAccId),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                helper_account_constructor(AccountEnum::UserHoldingLPInit),
                ];
        let _post_states = remove_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::RemoveAmountLP), 
                    helper_balance_constructor(BalanceEnum::RemoveMinAmountA),
                    helper_balance_constructor(BalanceEnum::RemoveMinAmountB)],
                    );
    }

    #[should_panic(expected = "Invalid liquidity account provided")]
    #[test]
    fn test_call_remove_liquidity_insufficient_liquidity_amount() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::PoolLPInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                helper_account_constructor(AccountEnum::UserHoldingA), //different token account than lp to create desired error
                ];
        let _post_states = remove_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::RemoveAmountLP), 
                    helper_balance_constructor(BalanceEnum::RemoveMinAmountA),
                    helper_balance_constructor(BalanceEnum::RemoveMinAmountB)],
                    );
    }

    #[should_panic(expected = "Insufficient minimal withdraw amount (Token A) provided for liquidity amount")]
    #[test]
    fn test_call_remove_liquidity_insufficient_balance_1() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::PoolLPInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                helper_account_constructor(AccountEnum::UserHoldingLPInit),
                ];
        let _post_states = remove_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::RemoveAmountLP1), 
                    helper_balance_constructor(BalanceEnum::RemoveMinAmountA),
                    helper_balance_constructor(BalanceEnum::RemoveMinAmountB)],
                    );
    }

    #[should_panic(expected = "Insufficient minimal withdraw amount (Token B) provided for liquidity amount")]
    #[test]
    fn test_call_remove_liquidity_insufficient_balance_2() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::PoolLPInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                helper_account_constructor(AccountEnum::UserHoldingLPInit),
                ];
        let _post_states = remove_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::RemoveAmountLP), 
                    helper_balance_constructor(BalanceEnum::RemoveMinAmountA),
                    helper_balance_constructor(BalanceEnum::RemoveMinAmountB)],
                    );
    }

    #[should_panic(expected = "Minimum withdraw amount must be nonzero")]
    #[test]
    fn test_call_remove_liquidity_min_bal_zero_1() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::PoolLPInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                helper_account_constructor(AccountEnum::UserHoldingLPInit),
                ];
        let _post_states = remove_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::RemoveAmountLP), 
                    0,
                    helper_balance_constructor(BalanceEnum::RemoveMinAmountB)],
                    );
    }

    #[should_panic(expected = "Minimum withdraw amount must be nonzero")]
    #[test]
    fn test_call_remove_liquidity_min_bal_zero_2() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::PoolLPInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                helper_account_constructor(AccountEnum::UserHoldingLPInit),
                ];
        let _post_states = remove_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::RemoveAmountLP), 
                    helper_balance_constructor(BalanceEnum::RemoveMinAmountA),
                    0],
                    );
    }

    #[should_panic(expected = "Liquidity amount must be nonzero")]
    #[test]
    fn test_call_remove_liquidity_lp_bal_zero() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::PoolLPInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                helper_account_constructor(AccountEnum::UserHoldingLPInit),
                ];
        let _post_states = remove_liquidity(&pre_states, 
                    &[0, 
                    helper_balance_constructor(BalanceEnum::RemoveMinAmountA),
                    helper_balance_constructor(BalanceEnum::RemoveMinAmountB),],
                    );
    }    

    #[test]
    fn test_call_remove_liquidity_chained_call_successful() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::PoolLPInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                helper_account_constructor(AccountEnum::UserHoldingLPInit),
                ];
        let (post_states, chained_calls) = remove_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::RemoveAmountLP), 
                    helper_balance_constructor(BalanceEnum::RemoveMinAmountA),
                    helper_balance_constructor(BalanceEnum::RemoveMinAmountBLow),],
                    );

        let pool_post = post_states[0].clone();

        assert!(helper_account_constructor(AccountEnum::PoolDefinitionRemoveSuccessful).account ==
                   *pool_post.account());

        let chained_call_lp = chained_calls[0].clone();
        let chained_call_b = chained_calls[1].clone();           
        let chained_call_a = chained_calls[2].clone();     

        assert!(chained_call_a == helper_chained_call_constructor(ChainedCallsEnum::CcRemoveTokenA));
        assert!(chained_call_b == helper_chained_call_constructor(ChainedCallsEnum::CcRemoveTokenB));
        assert!(chained_call_lp.instruction_data == helper_chained_call_constructor(ChainedCallsEnum::CcRemovePoolLP).instruction_data);
    }   

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]    
    fn test_call_add_liquidity_with_invalid_number_of_accounts_1() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                ];
        let _post_states = add_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::AddMaxAmountA),
                    helper_balance_constructor(BalanceEnum::AddMaxAmountb),
                    helper_balance_constructor(BalanceEnum::AddMinAmountLP),],
                    );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_add_liquidity_with_invalid_number_of_accounts_2() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                ];
        let _post_states = add_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::AddMaxAmountA),
                    helper_balance_constructor(BalanceEnum::AddMaxAmountb),
                    helper_balance_constructor(BalanceEnum::AddMinAmountLP),],
                    );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_add_liquidity_with_invalid_number_of_accounts_3() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                ];
        let _post_states = add_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::AddMaxAmountA),
                    helper_balance_constructor(BalanceEnum::AddMaxAmountb),
                    helper_balance_constructor(BalanceEnum::AddMinAmountLP),],
                    );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_add_liquidity_with_invalid_number_of_accounts_4() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::PoolLPInit),
                ];
        let _post_states = add_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::AddMaxAmountA),
                    helper_balance_constructor(BalanceEnum::AddMaxAmountb),
                    helper_balance_constructor(BalanceEnum::AddMinAmountLP),],
                    );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_add_liquidity_with_invalid_number_of_accounts_5() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::PoolLPInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                ];
        let _post_states = add_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::AddMaxAmountA),
                    helper_balance_constructor(BalanceEnum::AddMaxAmountb),
                    helper_balance_constructor(BalanceEnum::AddMinAmountLP),],
                    );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_add_liquidity_with_invalid_number_of_accounts_6() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::PoolLPInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                ];
        let _post_states = add_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::AddMaxAmountA),
                    helper_balance_constructor(BalanceEnum::AddMaxAmountb),
                    helper_balance_constructor(BalanceEnum::AddMinAmountLP),],
                    );
    }
  
    #[should_panic(expected = "Invalid number of input balances")]
    #[test]
    fn test_call_add_liquidity_invalid_number_of_balances_1() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::PoolLPInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                helper_account_constructor(AccountEnum::UserHoldingLPInit),
                ];
        let _post_states = add_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::AddMaxAmountA),],
                    );
    }

    #[should_panic(expected = "Vault A was not provided")]
    #[test]
    fn test_call_add_liquidity_vault_a_omitted() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAWrongAccId),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::PoolLPInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                helper_account_constructor(AccountEnum::UserHoldingLPInit),
                ];
        let _post_states = add_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::AddMaxAmountA),
                    helper_balance_constructor(BalanceEnum::AddMaxAmountb),
                    helper_balance_constructor(BalanceEnum::AddMinAmountLP),],
                    );
    }        

    #[should_panic(expected = "Vault B was not provided")]
    #[test]
    fn test_call_add_liquidity_vault_b_omitted() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBWrongAccId),
                helper_account_constructor(AccountEnum::PoolLPInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                helper_account_constructor(AccountEnum::UserHoldingLPInit),
                ];
        let _post_states = add_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::AddMaxAmountA),
                    helper_balance_constructor(BalanceEnum::AddMaxAmountb),
                    helper_balance_constructor(BalanceEnum::AddMinAmountLP),],
                    );
    }    

    #[should_panic(expected = "LP definition mismatch")]
    #[test]
    fn test_call_add_liquidity_lp_def_mismatch() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::PoolLPWrongAccId),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                helper_account_constructor(AccountEnum::UserHoldingLPInit),
                ];
        let _post_states = add_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::AddMaxAmountA),
                    helper_balance_constructor(BalanceEnum::AddMaxAmountb),
                    helper_balance_constructor(BalanceEnum::AddMinAmountLP),],
                    );
    }    

    #[should_panic(expected = "Both max-balances must be nonzero")]
    #[test]
    fn test_call_add_liquidity_zero_balance_1() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::PoolLPInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                helper_account_constructor(AccountEnum::UserHoldingLPInit),
                ];
        let _post_states = add_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::AddMinAmountLP),
                    0,
                    helper_balance_constructor(BalanceEnum::AddMaxAmountb),],
                    );
    }

    #[should_panic(expected = "Both max-balances must be nonzero")]
    #[test]
    fn test_call_add_liquidity_zero_balance_2() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::PoolLPInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                helper_account_constructor(AccountEnum::UserHoldingLPInit),
                ];
        let _post_states = add_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::AddMaxAmountA),
                    0,
                    helper_balance_constructor(BalanceEnum::AddMinAmountLP),],
                    );
    }

    #[should_panic(expected = "Min-lp must be nonzero")]
    #[test]
    fn test_call_add_liquidity_zero_min_lp() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::PoolLPInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                helper_account_constructor(AccountEnum::UserHoldingLPInit),
                ];
        let _post_states = add_liquidity(&pre_states, 
                    &[0,
                    helper_balance_constructor(BalanceEnum::AddMaxAmountA),
                    helper_balance_constructor(BalanceEnum::AddMaxAmountb),],);
    }

    #[should_panic(expected = "Vaults' balances must be at least the reserve amounts")]
    #[test]
    fn test_call_add_liquidity_vault_insufficient_balance_1() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInitZero),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::PoolLPInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                helper_account_constructor(AccountEnum::UserHoldingLPInit),
                ];
        let _post_states = add_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::AddMaxAmountA), 
                    helper_balance_constructor(BalanceEnum::AddMaxAmountb),
                    helper_balance_constructor(BalanceEnum::AddMinAmountLP),],
                    );
    }

    #[should_panic(expected = "Vaults' balances must be at least the reserve amounts")]
    #[test]
    fn test_call_add_liquidity_vault_insufficient_balance_2() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInitZero),
                helper_account_constructor(AccountEnum::PoolLPInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                helper_account_constructor(AccountEnum::UserHoldingLPInit),
                ];
        let _post_states = add_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::AddMaxAmountA), 
                    helper_balance_constructor(BalanceEnum::AddMaxAmountb),
                    helper_balance_constructor(BalanceEnum::AddMinAmountLP),],
                    );
    }

    #[should_panic(expected = "A trade amount is 0")]
    #[test]
    fn test_call_add_liquidity_actual_amount_zero_1() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInitReserveALow),
                helper_account_constructor(AccountEnum::VaultAInitLow),
                helper_account_constructor(AccountEnum::VaultBInitHigh),
                helper_account_constructor(AccountEnum::PoolLPInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                helper_account_constructor(AccountEnum::UserHoldingLPInit),
                ];
        let _post_states = add_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::AddMaxAmountA), 
                    helper_balance_constructor(BalanceEnum::AddMaxAmountb),
                    helper_balance_constructor(BalanceEnum::AddMinAmountLP),],
                    );
    }

    #[should_panic(expected = "A trade amount is 0")]
    #[test]
    fn test_call_add_liquidity_actual_amount_zero_2() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInitReserveBLow),
                helper_account_constructor(AccountEnum::VaultAInitHigh),
                helper_account_constructor(AccountEnum::VaultBInitLow),
                helper_account_constructor(AccountEnum::PoolLPInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                helper_account_constructor(AccountEnum::UserHoldingLPInit),
                ];
        let _post_states = add_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::AddMaxAmountALow), 
                    helper_balance_constructor(BalanceEnum::AddMaxAmountBLow),
                    helper_balance_constructor(BalanceEnum::AddMinAmountLP),],
                    );
    }

    #[should_panic(expected = "Reserves must be nonzero")]
    #[test]
    fn test_call_add_liquidity_reserves_zero_1() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInitReserveAZero),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::PoolLPInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                helper_account_constructor(AccountEnum::UserHoldingLPInit),
                ];
        let _post_states = add_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::AddMaxAmountA),
                    helper_balance_constructor(BalanceEnum::AddMaxAmountb),
                    helper_balance_constructor(BalanceEnum::AddMinAmountLP),],
                    );        
    }

    #[should_panic(expected = "Reserves must be nonzero")]
    #[test]
    fn test_call_add_liquidity_reserves_zero_2() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInitReserveBZero),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::PoolLPInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                helper_account_constructor(AccountEnum::UserHoldingLPInit),
                ];
        let _post_states = add_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::AddMaxAmountA),
                    helper_balance_constructor(BalanceEnum::AddMaxAmountb),
                    helper_balance_constructor(BalanceEnum::AddMinAmountLP),],
                    );        
    }

    #[should_panic(expected = "Payable LP must be nonzero")]
    #[test]
    fn test_call_add_liquidity_payable_lp_zero() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionAddZeroLP),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::PoolLPInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                helper_account_constructor(AccountEnum::UserHoldingLPInit),
                ];
        let _post_states = add_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::AddMaxAmountALow),
                    helper_balance_constructor(BalanceEnum::AddMaxAmountBLow),
                    helper_balance_constructor(BalanceEnum::AddMinAmountLP),],
                    );
    }

    #[test]
    fn test_call_add_liquidity_successful_chain_call() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::PoolLPInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                helper_account_constructor(AccountEnum::UserHoldingLPInit),
                ];
        let (post_states, chained_calls) = add_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::AddMinAmountLP),
                    helper_balance_constructor(BalanceEnum::AddMaxAmountA),
                    helper_balance_constructor(BalanceEnum::AddMaxAmountb),],
                    );
    
        let pool_post = post_states[0].clone();

        assert!(helper_account_constructor(AccountEnum::PoolDefinitionAddSuccessful).account ==
                    *pool_post.account());

        let chained_call_lp = chained_calls[0].clone();
        let chained_call_b = chained_calls[1].clone();
        let chained_call_a = chained_calls[2].clone();


        assert!(chained_call_a == helper_chained_call_constructor(ChainedCallsEnum::CcAddTokenA));
        assert!(chained_call_b == helper_chained_call_constructor(ChainedCallsEnum::CcAddTokenB));
        assert!(chained_call_lp == helper_chained_call_constructor(ChainedCallsEnum::CcAddPoolLP));
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]    
    fn test_call_swap_with_invalid_number_of_accounts_1() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                ];
        let _post_states = swap(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::AddMaxAmountA),
                    helper_balance_constructor(BalanceEnum::MinAmountOut)],
                    helper_id_constructor(IdEnum::TokenADefinitionId),
                    );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_swap_with_invalid_number_of_accounts_2() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                ];
        let _post_states = swap(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::AddMaxAmountA),
                    helper_balance_constructor(BalanceEnum::MinAmountOut)],
                    helper_id_constructor(IdEnum::TokenADefinitionId),
                    );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_swap_with_invalid_number_of_accounts_3() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                ];
        let _post_states = swap(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::AddMaxAmountA),
                    helper_balance_constructor(BalanceEnum::MinAmountOut)],
                    helper_id_constructor(IdEnum::TokenADefinitionId),
                    );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_swap_with_invalid_number_of_accounts_4() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                ];
        let _post_states = swap(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::AddMaxAmountA),
                    helper_balance_constructor(BalanceEnum::MinAmountOut)],
                    helper_id_constructor(IdEnum::TokenADefinitionId),
                    );
    }

    #[should_panic(expected = "Invalid number of amounts provided")]
    #[test]
    fn test_call_swap_with_invalid_number_of_amounts() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                ];
        let _post_states = swap(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::AddMaxAmountA)],
                    helper_id_constructor(IdEnum::TokenLPDefinitionId),
                    );
    }

    #[should_panic(expected = "AccountId is not a token type for the pool")]
    #[test]
    fn test_call_swap_incorrect_token_type() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                ];
        let _post_states = swap(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::AddMaxAmountA),
                    helper_balance_constructor(BalanceEnum::MinAmountOut)],
                    helper_id_constructor(IdEnum::TokenLPDefinitionId),
                    );
    }

    #[should_panic(expected = "Vault A was not provided")]
    #[test]
    fn test_call_swap_vault_a_omitted() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAWrongAccId),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                ];
        let _post_states = swap(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::AddMaxAmountA),
                    helper_balance_constructor(BalanceEnum::MinAmountOut)],
                    helper_id_constructor(IdEnum::TokenADefinitionId),
                    );
    }

    #[should_panic(expected = "Vault B was not provided")]
    #[test]
    fn test_call_swap_vault_b_omitted() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBWrongAccId),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                ];
        let _post_states = swap(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::AddMaxAmountA),
                    helper_balance_constructor(BalanceEnum::MinAmountOut)],
                    helper_id_constructor(IdEnum::TokenADefinitionId),
                    );
    }

    #[should_panic(expected = "Reserve for Token A exceeds vault balance")]
    #[test]
    fn test_call_swap_reserves_vault_mismatch_1() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInitLow),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                ];
        let _post_states = swap(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::AddMaxAmountA),
                    helper_balance_constructor(BalanceEnum::MinAmountOut)],
                    helper_id_constructor(IdEnum::TokenADefinitionId),
                    );
    }

        #[should_panic(expected = "Reserve for Token B exceeds vault balance")]
    #[test]
    fn test_call_swap_reserves_vault_mismatch_2() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInitLow),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                ];
        let _post_states = swap(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::AddMaxAmountA),
                    helper_balance_constructor(BalanceEnum::MinAmountOut)],
                    helper_id_constructor(IdEnum::TokenADefinitionId),
                    );
    }

    #[should_panic(expected = "Pool is inactive")]
    #[test]
    fn test_call_swap_ianctive() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInactive),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                ];
        let _post_states = swap(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::AddMaxAmountA),
                    helper_balance_constructor(BalanceEnum::MinAmountOut)],
                    helper_id_constructor(IdEnum::TokenADefinitionId),
                    );
    }

    #[should_panic(expected = "Withdraw amount is less than minimal amount out")]
    #[test]
    fn test_call_swap_below_min_out() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                ];
        let _post_states = swap(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::AddMaxAmountA),
                    helper_balance_constructor(BalanceEnum::MinAmountOut)],
                    helper_id_constructor(IdEnum::TokenADefinitionId),
                    );
    }

    #[test]
    fn test_call_swap_successful_chain_call_1() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                ];
        let (post_states, chained_calls) = swap(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::AddMaxAmountA),
                    helper_balance_constructor(BalanceEnum::AddMaxAmountALow)],
                    helper_id_constructor(IdEnum::TokenADefinitionId),
                    );
    
        let pool_post = post_states[0].clone();

        assert!(helper_account_constructor(AccountEnum::PoolDefinitionSwapTest1).account ==
                    *pool_post.account());

        let chained_call_a = chained_calls[0].clone();            
        let chained_call_b = chained_calls[1].clone();

        assert!(chained_call_a == helper_chained_call_constructor(ChainedCallsEnum::CcSwapTokenATest1));
        assert!(chained_call_b == helper_chained_call_constructor(ChainedCallsEnum::CcSwapTokenBTest1));
    }

    #[test]
    fn test_call_swap_successful_chain_call_2() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::PoolDefinitionInit),
                helper_account_constructor(AccountEnum::VaultAInit),
                helper_account_constructor(AccountEnum::VaultBInit),
                helper_account_constructor(AccountEnum::UserHoldingA),
                helper_account_constructor(AccountEnum::UserHoldingB),
                ];
        let (post_states, chained_calls) = swap(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::AddMaxAmountb),
                    helper_balance_constructor(BalanceEnum::MinAmountOut)],
                    helper_id_constructor(IdEnum::TokenBDefinitionId),
                    );
    
        let pool_post = post_states[0].clone();

        assert!(helper_account_constructor(AccountEnum::PoolDefinitionSwapTest2).account ==
                    *pool_post.account());

        let chained_call_a = chained_calls[1].clone();            
        let chained_call_b = chained_calls[0].clone();

        assert!(chained_call_a == helper_chained_call_constructor(ChainedCallsEnum::CcSwapTokenATest2));
        assert!(chained_call_b == helper_chained_call_constructor(ChainedCallsEnum::CcSwapTokenBTest2));
    }

}