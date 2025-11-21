use nssa_core::{
    account::{Account, AccountId, AccountWithMetadata, Data},
    program::{ProgramId, ProgramInput, ChainedCall, read_nssa_inputs, write_nssa_outputs, write_nssa_outputs_with_chained_call},
};

use bytemuck;

// The AMM program has four functions:
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
//        TODO: ideally, vault_holding_a, vault_holding_b, pool_lp and user_holding_lp are uninitated.
//      * An instruction data of 55-bytes, indicating the initial amm reserves' balances and token_program_id with
//        the following layout:
//        [0x00 || array of balances (little-endian 16 bytes) || TOKEN_PROGRAM_ID)]
// 2. Swap assets
//    Arguments to this function are:
//      * Two accounts: [amm_pool, vault_holding_1, vault_holding_2, user_holding_a, user_holding_b].
//      * An instruction data byte string of length 49, indicating which token type to swap and maximum amount with the following layout
//        [0x01 || amount (little-endian 16 bytes) || TOKEN_DEFINITION_ID].
// 3. Add liquidity
//    Arguments to this function are:
//      * Two accounts: [amm_pool, vault_holding_a, vault_holding_b, pool_lp, user_holding_a, user_holding_b, user_holding_lp].
//      * An instruction data byte string of length 65, amounts to add
//        [0x02 || array of max amounts (little-endian 16 bytes) || TOKEN_DEFINITION_ID (for primary)].
// 4. Remove liquidity
//      * Input instruction set [0x03].

const POOL_DEFINITION_DATA_SIZE: usize = 240;

struct PoolDefinition{
    definition_token_a_id: AccountId,
    definition_token_b_id: AccountId,
    vault_a_addr: AccountId,
    vault_b_addr: AccountId,
    liquidity_pool_id: AccountId,
    liquidity_pool_cap: u128,
    reserve_a: u128,
    reserve_b: u128,
    token_program_id: ProgramId,
}


impl PoolDefinition {
    fn into_data(self) -> Vec<u8> {
        let u8_token_program_id : [u8;32] = bytemuck::cast(self.token_program_id);

        let mut bytes = [0; POOL_DEFINITION_DATA_SIZE];
        bytes[0..32].copy_from_slice(&self.definition_token_a_id.to_bytes());
        bytes[32..64].copy_from_slice(&self.definition_token_b_id.to_bytes());
        bytes[64..96].copy_from_slice(&self.vault_a_addr.to_bytes());
        bytes[96..128].copy_from_slice(&self.vault_b_addr.to_bytes());
        bytes[128..160].copy_from_slice(&self.liquidity_pool_id.to_bytes());
        bytes[160..176].copy_from_slice(&self.liquidity_pool_cap.to_le_bytes());
        bytes[176..192].copy_from_slice(&self.reserve_a.to_le_bytes());
        bytes[192..208].copy_from_slice(&self.reserve_b.to_le_bytes());
        bytes[208..].copy_from_slice(&u8_token_program_id);
        bytes.into()
    }

    fn parse(data: &[u8]) -> Option<Self> {
        if data.len() != POOL_DEFINITION_DATA_SIZE {
            None
        } else {
            let definition_token_a_id = AccountId::new(data[0..32].try_into().unwrap());
            let definition_token_b_id = AccountId::new(data[32..64].try_into().unwrap());
            let vault_a_addr = AccountId::new(data[64..96].try_into().unwrap());
            let vault_b_addr = AccountId::new(data[96..128].try_into().unwrap());
            let liquidity_pool_id = AccountId::new(data[128..160].try_into().unwrap());
            let liquidity_pool_cap = u128::from_le_bytes(data[160..176].try_into().unwrap());
            let reserve_a = u128::from_le_bytes(data[176..192].try_into().unwrap());
            let reserve_b = u128::from_le_bytes(data[192..208].try_into().unwrap());

            let token_program_id : &[u32] = bytemuck::cast_slice(&data[208..]);
            let token_program_id : ProgramId = token_program_id[0..8].try_into().unwrap();
            Some(Self {
                definition_token_a_id,
                definition_token_b_id,
                vault_a_addr,
                vault_b_addr,
                liquidity_pool_id,
                liquidity_pool_cap,
                reserve_a,
                reserve_b,
                token_program_id,
            })
        }
    }
}


//TODO: remove repeated code for Token_Definition and TokenHoldling
const TOKEN_DEFINITION_TYPE: u8 = 0;
const TOKEN_DEFINITION_DATA_SIZE: usize = 23;
const TOKEN_HOLDING_TYPE: u8 = 1;
const TOKEN_HOLDING_DATA_SIZE: usize = 49;

struct TokenHolding {
    account_type: u8,
    definition_id: AccountId,
    balance: u128,
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
            let definition_id = AccountId::new(data[1..33].try_into().unwrap());
            let balance = u128::from_le_bytes(data[33..].try_into().unwrap());
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
        bytes.into()
    }
}


fn new_definition(
        pre_states: &[AccountWithMetadata],
        balance_in: &[u128],
        token_program: ProgramId,
    ) -> (Vec<Account>, Vec<ChainedCall>) {

    //Pool accounts: pool itself, and its 2 vaults and LP token
    //2 accounts for funding tokens
    //initial funder's LP account
    if pre_states.len() != 7 {
        panic!("Invalid number of input accounts")
    }

    if balance_in.len() != 2 {
        panic!("Invalid number of balance")
    }

    let pool = &pre_states[0];
    let vault_a = &pre_states[1];
    let vault_b = &pre_states[2];
    let pool_lp = &pre_states[3];
    let user_a = &pre_states[4];
    let user_b = &pre_states[5];
    let user_lp = &pre_states[6];

    if pool.account == Account::default() || !pool.is_authorized {
        panic!("Pool account is uninitiated or not authorized");
    }

    // TODO: temporary band-aid to prevent vault's from being
    // owned by the amm program.
    if vault_a.account == Account::default() || vault_b.account == Account::default() {
        panic!("Vault accounts uninitialized")
    }
    if pool_lp.account == Account::default() {
        panic!("Pool LP must be initialized first")
    }

    let amount_a = balance_in[0];
    let amount_b = balance_in[1];

    // Prevents pool constant coefficient (k) from being 0.
    if amount_a == 0 || amount_b == 0 {
        panic!("Balances must be nonzero")
    }

    // Verify token_a and token_b are different
    let definition_token_a_id = TokenHolding::parse(&vault_a.account.data).unwrap().definition_id;
    let definition_token_b_id = TokenHolding::parse(&vault_b.account.data).unwrap().definition_id;
    let user1_id = TokenHolding::parse(&vault_a.account.data).unwrap().definition_id;

    if definition_token_a_id == definition_token_b_id {
        panic!("Vaults are for the same token")
    }

    // 5. Update pool account
    let mut pool_post = Account::default();
    let pool_post_definition = PoolDefinition {
            definition_token_a_id,
            definition_token_b_id,
            vault_a_addr: vault_a.account_id.clone(),
            vault_b_addr: vault_b.account_id.clone(),
            liquidity_pool_id: pool_lp.account_id.clone(),
            liquidity_pool_cap: amount_a,
            reserve_a: amount_a,
            reserve_b: amount_b,
            token_program_id: token_program,  
    };

    pool_post.data = pool_post_definition.into_data();

    let mut chained_call = Vec::new();

    let mut instruction_data = [0; 23];
    instruction_data[0] = 1;

    instruction_data[1..17].copy_from_slice(&amount_a.to_le_bytes());
    let call_token_a = ChainedCall{
            program_id: token_program,
            instruction_data: bytemuck::cast_slice(&instruction_data).to_vec(),
            pre_states: vec![user_a.clone(), vault_a.clone()]
        };


    instruction_data[1..17].copy_from_slice(&amount_b.to_le_bytes());
    let call_token_b = ChainedCall{
            program_id: token_program,
            instruction_data: bytemuck::cast_slice(&instruction_data).to_vec(),
            pre_states: vec![user_b.clone(), vault_b.clone()]
        };

    instruction_data[1..17].copy_from_slice(&amount_a.to_le_bytes());
    let call_token_lp = ChainedCall{
            program_id: token_program,
            instruction_data: bytemuck::cast_slice(&instruction_data).to_vec(),
            pre_states: vec![pool_lp.clone(), user_lp.clone()]
        };

    chained_call.push(call_token_lp);
    chained_call.push(call_token_b);
    chained_call.push(call_token_a);


    let post_states = vec![pool_post.clone(), 
        pre_states[1].account.clone(),
        pre_states[2].account.clone(),
        pre_states[3].account.clone(),
        pre_states[4].account.clone(),
        pre_states[5].account.clone(),
        pre_states[6].account.clone()];

    (post_states.clone(), chained_call)
}


type Instruction = Vec<u8>;
fn main() {
    let ProgramInput {
        pre_states,
        instruction,
    } = read_nssa_inputs::<Instruction>();

    match instruction[0] {
        0 => {
            let balance_a: u128 = u128::from_le_bytes(instruction[1..17].try_into().unwrap());
            let balance_b: u128 = u128::from_le_bytes(instruction[17..33].try_into().unwrap());
        

            let token_program_id : &[u32] = bytemuck::cast_slice(&instruction[33..65]);
            let token_program_id : [u32;8] = token_program_id.try_into().unwrap();

            let (post_states, chained_call) = new_definition(&pre_states,
                &[balance_a, balance_b],
                token_program_id
                );

            write_nssa_outputs_with_chained_call(pre_states, post_states, chained_call);
        }
        1 => {
            let token_id = AccountId::new(instruction[1..33].try_into().unwrap());
            let amount = u128::from_le_bytes(instruction[33..49].try_into().unwrap());

            let (post_states, chained_call) = swap(&pre_states, amount, token_id);

            write_nssa_outputs_with_chained_call(pre_states, post_states, chained_call);
        }
        2 => {
            let (post_states, chained_call) = add_liquidity(&pre_states,
                        &[u128::from_le_bytes(instruction[1..17].try_into().unwrap()),
                            u128::from_le_bytes(instruction[16..33].try_into().unwrap()),],
                        AccountId::new(instruction[33..65].try_into().unwrap()));
           write_nssa_outputs_with_chained_call(pre_states, post_states, chained_call);
        }
        3 => {

            let (post_states, chained_call) = remove_liquidity(&pre_states);

            write_nssa_outputs_with_chained_call(pre_states, post_states, chained_call);
        }
        _ => panic!("Invalid instruction"),
    };
}

fn swap(
        pre_states: &[AccountWithMetadata],
        amount: u128,
        token_id: AccountId,
    ) -> (Vec<Account>, Vec<ChainedCall>) {

    if pre_states.len() != 5 {
        panic!("Invalid number of input accounts");
    }

    let pool = &pre_states[0];
    let vault1 = &pre_states[1];
    let vault2 = &pre_states[2];
    let user_a = &pre_states[3];
    let user_b = &pre_states[4];

    // Verify vaults are in fact vaults
    let pool_def_data = PoolDefinition::parse(&pool.account.data).unwrap();
    let vault1_data = TokenHolding::parse(&vault1.account.data).unwrap();
    let vault2_data = TokenHolding::parse(&vault2.account.data).unwrap();


    let mut vault_a = AccountWithMetadata::default();
    let mut vault_b = AccountWithMetadata::default();

    if vault1_data.definition_id == pool_def_data.definition_token_a_id {
            vault_a = vault1.clone();
        } else if vault2_data.definition_id == pool_def_data.definition_token_a_id {
            vault_a = vault2.clone();
        } else {
            panic!("Vault A was not provided");
        }
        
    if vault1_data.definition_id == pool_def_data.definition_token_b_id {
       vault_b = vault1.clone();
    } else if vault2_data.definition_id == pool_def_data.definition_token_b_id {
        vault_b = vault2.clone();
    } else {
        panic!("Vault B was not provided");
    }

    // 1. Identify swap direction (a -> b or b -> a)
    let mut deposit_a = 0;
    let mut deposit_b = 0;
    let a_to_b;
    if token_id == pool_def_data.definition_token_a_id {
        deposit_a = amount;
        a_to_b = true;
    } else if token_id == pool_def_data.definition_token_b_id {
        deposit_b = amount;
        a_to_b = false;
    } else {
        panic!("Address is not a token type for the pool");
    }

    // 2. fetch pool reserves
    //validates reserves is at least the vaults' balances
    assert!(vault_a.account.balance >= pool_def_data.reserve_a);
    assert!(vault_b.account.balance >= pool_def_data.reserve_b);
    //Cannot swap if a reserve is 0
    assert!(pool_def_data.reserve_a > 0);
    assert!(pool_def_data.reserve_b > 0);

    // 3. Compute output amount
    // Note: no fees
    // Compute pool's exchange constant
    // let k = pool_def_data.reserve_a * pool_def_data.reserve_b;
    let withdraw_a = if a_to_b { 0 }
            else { (pool_def_data.reserve_a * deposit_b)/(pool_def_data.reserve_b + deposit_b) };   
    let withdraw_b = if a_to_b { (pool_def_data.reserve_b * deposit_a)/(pool_def_data.reserve_a + deposit_a)}
                    else { 0 };                 

    // 4. Slippage check
    if a_to_b {
        assert!(withdraw_b == 0); }
    else{
        assert!(withdraw_a == 0); }

    // 5. Update pool account
    let mut pool_post = pool.account.clone();
    let pool_post_definition = PoolDefinition {
            definition_token_a_id: pool_def_data.definition_token_a_id.clone(),
            definition_token_b_id: pool_def_data.definition_token_b_id.clone(),
            vault_a_addr: pool_def_data.vault_a_addr.clone(),
            vault_b_addr: pool_def_data.vault_b_addr.clone(),
            liquidity_pool_id: pool_def_data.liquidity_pool_id.clone(),
            liquidity_pool_cap: pool_def_data.liquidity_pool_cap.clone(),
            reserve_a: pool_def_data.reserve_a + deposit_a - withdraw_a,
            reserve_b: pool_def_data.reserve_b + deposit_b - withdraw_b,
            token_program_id: pool_def_data.token_program_id.clone(),  
    };

    pool_post.data = pool_post_definition.into_data();

    let mut chained_call = Vec::new();

    let mut instruction_data = [0; 23];
    instruction_data[0] = 1;

    let call_token_a = if a_to_b {
        instruction_data[1..17].copy_from_slice(&deposit_a.to_le_bytes());
        ChainedCall{
            program_id: pool_def_data.token_program_id.clone(),
            instruction_data: bytemuck::cast_slice(&instruction_data).to_vec(),
            pre_states: vec![user_a.clone(), vault_a.clone()]
        }
    } else {
        instruction_data[1..17].copy_from_slice(&withdraw_a.to_le_bytes());
        ChainedCall{
            program_id: pool_def_data.token_program_id.clone(),
            instruction_data: bytemuck::cast_slice(&instruction_data).to_vec(),
            pre_states: vec![vault_a.clone(), user_a.clone()]
        }
    };

    let call_token_b = if a_to_b {
        instruction_data[1..17].copy_from_slice(&deposit_b.to_le_bytes());
        ChainedCall{
            program_id: pool_def_data.token_program_id.clone(),
            instruction_data: bytemuck::cast_slice(&instruction_data).to_vec(),
            pre_states: vec![user_b.clone(), vault_b.clone()]
        }
    } else {
        instruction_data[1..17].copy_from_slice(&withdraw_b.to_le_bytes());
        ChainedCall{
            program_id: pool_def_data.token_program_id.clone(),
            instruction_data: bytemuck::cast_slice(&instruction_data).to_vec(),
            pre_states: vec![vault_b.clone(), user_b.clone()]
        }
    };

    chained_call.push(call_token_a);
    chained_call.push(call_token_b);

    let post_states = vec![pool_post.clone(), 
        pre_states[1].account.clone(),
        pre_states[2].account.clone(),
        pre_states[3].account.clone(),
        pre_states[4].account.clone()];

    (post_states.clone(), chained_call)
}


fn add_liquidity(pre_states: &[AccountWithMetadata],
    max_balance_in: &[u128],
    main_token: AccountId) -> (Vec<Account>, Vec<ChainedCall>) {

    if pre_states.len() != 7 {
       panic!("Invalid number of input accounts");
    }

    let pool = &pre_states[0];
    let vault1 = &pre_states[1];
    let vault2 = &pre_states[2];
    let pool_lp = &pre_states[3];
    let user_a = &pre_states[4];
    let user_b = &pre_states[5];
    let user_lp = &pre_states[6];

    let mut vault_a = AccountWithMetadata::default();
    let mut vault_b = AccountWithMetadata::default();
    
    let pool_def_data = PoolDefinition::parse(&pool.account.data).unwrap();

    let vault1_data = TokenHolding::parse(&vault1.account.data).unwrap();
    let vault2_data = TokenHolding::parse(&vault2.account.data).unwrap();

    if vault1_data.definition_id == pool_def_data.definition_token_a_id {
            vault_a = vault1.clone();
        } else if vault2_data.definition_id == pool_def_data.definition_token_a_id {
            vault_a = vault2.clone();
        } else {
            panic!("Vault A was not provided");
        }
        
    if vault1_data.definition_id == pool_def_data.definition_token_b_id {
       vault_b = vault1.clone();
    } else if vault2_data.definition_id == pool_def_data.definition_token_b_id {
        vault_b = vault2.clone();
    } else {
        panic!("Vault B was not provided");
    }
    
    if max_balance_in.len() != 2 {
        panic!("Invalid number of input balances");
    }
    let max_amount_a = max_balance_in[0];
    let max_amount_b = max_balance_in[1];

    if max_amount_a == 0 || max_amount_b == 0 {
        panic!("Both max-balances must be nonzero");
    }
    
    // 2. Determine deposit amounts
    let mut actual_amount_a = 0;
    let mut actual_amount_b = 0;
    
    if vault_b.account.balance == 0 || vault_a.account.balance == 0 {
        panic!("Vaults must have nonzero balances");
    }

    if pool_def_data.reserve_a == 0 || pool_def_data.reserve_b == 0 {
        panic!("Reserves must be nonzero");
    }


    if main_token == pool_def_data.definition_token_a_id {
        actual_amount_a = max_amount_a;
        actual_amount_b = (pool_def_data.reserve_b*actual_amount_a)/pool_def_data.reserve_a;
    } else if main_token == pool_def_data.definition_token_b_id {
        actual_amount_b = max_amount_b;
        actual_amount_a = (pool_def_data.reserve_a*actual_amount_b)/pool_def_data.reserve_b;
    } else {
        panic!("Mismatch of token types"); //main token does not match with vaults.
    }

    
    // 3. Validate amounts
    assert!(max_amount_a >= actual_amount_a && max_amount_b >= actual_amount_b);
    if user_a.account.balance < actual_amount_a {
        panic!("Insufficient balance");
    }

    if user_b.account.balance < actual_amount_b {
        panic!("Insufficient balance");
    }
    
    if actual_amount_a == 0 || actual_amount_b == 0 {
        panic!("A trade amount is 0");
    }
    

    // 4. Calculate LP to mint
    let mut delta_lp: u128 = 0;
    delta_lp = (pool_def_data.liquidity_pool_cap *actual_amount_b)/pool_def_data.reserve_b;

    // 5. Update pool account
    let mut pool_post = pool.account.clone();
    let pool_post_definition = PoolDefinition {
            definition_token_a_id: pool_def_data.definition_token_a_id.clone(),
            definition_token_b_id: pool_def_data.definition_token_b_id.clone(),
            vault_a_addr: pool_def_data.vault_a_addr.clone(),
            vault_b_addr: pool_def_data.vault_b_addr.clone(),
            liquidity_pool_id: pool_def_data.liquidity_pool_id.clone(),
            liquidity_pool_cap: pool_def_data.liquidity_pool_cap + delta_lp,
            reserve_a: pool_def_data.reserve_a + actual_amount_a,
            reserve_b: pool_def_data.reserve_b + actual_amount_b,
            token_program_id: pool_def_data.token_program_id.clone(),  
    };

    pool_post.data = pool_post_definition.into_data();

    let mut chained_call = Vec::new();

    let mut instruction_data = [0; 23];
    instruction_data[0] = 1;

    instruction_data[1..17].copy_from_slice(&actual_amount_a.to_le_bytes());
    let call_token_a = ChainedCall{
            program_id: pool_def_data.token_program_id.clone(),
            instruction_data: bytemuck::cast_slice(&instruction_data).to_vec(),
            pre_states: vec![user_a.clone(), vault_a]
        };


    instruction_data[1..17].copy_from_slice(&actual_amount_b.to_le_bytes());
    let call_token_b = ChainedCall{
            program_id: pool_def_data.token_program_id.clone(),
            instruction_data: bytemuck::cast_slice(&instruction_data).to_vec(),
            pre_states: vec![user_b.clone(), vault_b]
        };

    instruction_data[1..17].copy_from_slice(&delta_lp.to_le_bytes());
    let call_token_lp = ChainedCall{
            program_id: pool_def_data.token_program_id.clone(),
            instruction_data: bytemuck::cast_slice(&instruction_data).to_vec(),
            pre_states: vec![pool_lp.clone(), user_lp.clone()]
        };

    chained_call.push(call_token_lp);
    chained_call.push(call_token_b);
    chained_call.push(call_token_a);


    let post_states = vec![pool_post.clone(), 
        pre_states[1].account.clone(),
        pre_states[2].account.clone(),
        pre_states[3].account.clone(),
        pre_states[4].account.clone(),
        pre_states[5].account.clone(),
        pre_states[6].account.clone(),];

    (post_states.clone(), chained_call)

}


fn remove_liquidity(pre_states: &[AccountWithMetadata]) -> (Vec<Account>, Vec<ChainedCall>) {

    if pre_states.len() != 7 {
       panic!("Invalid number of input accounts");
    }

    let pool = &pre_states[0];
    let vault1 = &pre_states[1];
    let vault2 = &pre_states[2];
    let pool_lp = &pre_states[3];
    let user_a = &pre_states[4];
    let user_b = &pre_states[5];
    let user_lp = &pre_states[6];

    let mut vault_a = AccountWithMetadata::default();
    let mut vault_b = AccountWithMetadata::default();
    
    let pool_def_data = PoolDefinition::parse(&pool.account.data).unwrap();
    let vault1_data = TokenHolding::parse(&vault1.account.data).unwrap();
    let vault2_data = TokenHolding::parse(&vault2.account.data).unwrap();

    if vault1_data.definition_id == pool_def_data.definition_token_a_id {
            vault_a = vault1.clone();
        } else if vault2_data.definition_id == pool_def_data.definition_token_a_id {
            vault_a = vault2.clone();
        } else {
            panic!("Vault A was not provided");
        }
        
    if vault1_data.definition_id == pool_def_data.definition_token_b_id {
       vault_b = vault1.clone();
    } else if vault2_data.definition_id == pool_def_data.definition_token_b_id {
        vault_b = vault2.clone();
    } else {
        panic!("Vault B was not provided");
    }

    // 2. Determine deposit amounts
    let withdraw_amount_a = pool_def_data.reserve_a * (user_lp.account.balance/pool_def_data.liquidity_pool_cap);
    let withdraw_amount_b = pool_def_data.reserve_b * (user_lp.account.balance/pool_def_data.liquidity_pool_cap);

    //3. Validate amounts handled by token programs

    // 4. Calculate LP to reduce cap by
    if pool_def_data.liquidity_pool_cap == 0 {
        panic!("Liquidity pool must be nonzero");
    }
    let delta_lp : u128 = (pool_def_data.liquidity_pool_cap*pool_def_data.liquidity_pool_cap - user_lp.account.balance)/pool_def_data.liquidity_pool_cap;

    // 5. Update pool account
    let mut pool_post = pool.account.clone();
    let pool_post_definition = PoolDefinition {
            definition_token_a_id: pool_def_data.definition_token_a_id.clone(),
            definition_token_b_id: pool_def_data.definition_token_b_id.clone(),
            vault_a_addr: pool_def_data.vault_a_addr.clone(),
            vault_b_addr: pool_def_data.vault_b_addr.clone(),
            liquidity_pool_id: pool_def_data.liquidity_pool_id.clone(),
            liquidity_pool_cap: pool_def_data.liquidity_pool_cap - delta_lp,
            reserve_a: pool_def_data.reserve_a - withdraw_amount_a,
            reserve_b: pool_def_data.reserve_b - withdraw_amount_b,
            token_program_id: pool_def_data.token_program_id.clone(),  
    };

    pool_post.data = pool_post_definition.into_data();

    let mut chained_call = Vec::new();

    let mut instruction_data = [0; 23];
    instruction_data[0] = 1;

    instruction_data[1..17].copy_from_slice(&withdraw_amount_a.to_le_bytes());
    let call_token_a = ChainedCall{
            program_id: pool_def_data.token_program_id.clone(),
            instruction_data: bytemuck::cast_slice(&instruction_data).to_vec(),
            pre_states: vec![vault_a, user_a.clone()]
        };


    instruction_data[1..17].copy_from_slice(&withdraw_amount_b.to_le_bytes());
    let call_token_b = ChainedCall{
            program_id: pool_def_data.token_program_id.clone(),
            instruction_data: bytemuck::cast_slice(&instruction_data).to_vec(),
            pre_states: vec![vault_b, user_b.clone()]
        };

    instruction_data[1..17].copy_from_slice(&delta_lp.to_le_bytes());
    let call_token_lp = ChainedCall{
            program_id: pool_def_data.token_program_id.clone(),
            instruction_data: bytemuck::cast_slice(&instruction_data).to_vec(),
            pre_states: vec![user_lp.clone(), pool_lp.clone()]
        };

    chained_call.push(call_token_lp);
    chained_call.push(call_token_b);
    chained_call.push(call_token_a);


    let post_states = vec![pool_post.clone(), 
        pre_states[1].account.clone(),
        pre_states[2].account.clone(),
        pre_states[3].account.clone(),
        pre_states[4].account.clone(),
        pre_states[5].account.clone(),
        pre_states[6].account.clone()];

    (post_states.clone(), chained_call)

}

#[cfg(test)]
mod tests {
    use nssa_core::account::{Account, AccountId, AccountWithMetadata};

    use crate::{POOL_DEFINITION_DATA_SIZE, PoolDefinition, TOKEN_HOLDING_DATA_SIZE, TOKEN_HOLDING_TYPE, TokenHolding, add_liquidity, new_definition, remove_liquidity, swap};

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]    
    fn test_call_new_definition_with_invalid_number_of_accounts_1() {
        let pre_states = vec![AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([1; 32]),
        }];
        let balance_a = 15u128;
        let balance_b = 15u128;
        let token_program_id: [u32;8] = [0; 8];
        let _post_states = new_definition(&pre_states, &[balance_a, balance_b], token_program_id);
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_new_definition_with_invalid_number_of_accounts_2() {
        let pre_states = vec![AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([1; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([2; 32])},
        ];
        let balance_a = 15u128;
        let balance_b = 15u128;
        let token_program_id: [u32;8] = [0; 8];
        let _post_states = new_definition(&pre_states, &[balance_a, balance_b], token_program_id);
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_new_definition_with_invalid_number_of_accounts_3() {
        let pre_states = vec![AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([1; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([2; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([3; 32])},
        ];
        let balance_a = 15u128;
        let balance_b = 15u128;
        let token_program_id: [u32;8] = [0; 8];
        let _post_states = new_definition(&pre_states, &[balance_a, balance_b], token_program_id);
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_new_definition_with_invalid_number_of_accounts_4() {
        let pre_states = vec![AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([1; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([2; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([3; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
        ];
        let balance_a = 15u128;
        let balance_b = 15u128;
        let token_program_id: [u32;8] = [0; 8];
        let _post_states = new_definition(&pre_states, &[balance_a, balance_b], token_program_id);
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_new_definition_with_invalid_number_of_accounts_5() {
        let pre_states = vec![AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([1; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([2; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([3; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([5; 32])},
        ];
        let balance_a = 15u128;
        let balance_b = 15u128;
        let token_program_id: [u32;8] = [0; 8];
        let _post_states = new_definition(&pre_states, &[balance_a, balance_b], token_program_id);
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_new_definition_with_invalid_number_of_accounts_6() {
        let pre_states = vec![AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([1; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([2; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([3; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([5; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([6; 32])},
        ];
        let balance_a = 15u128;
        let balance_b = 15u128;
        let token_program_id: [u32;8] = [0; 8];
        let _post_states = new_definition(&pre_states, &[balance_a, balance_b], token_program_id);
    }

    #[should_panic(expected = "Invalid number of balance")]
    #[test]
    fn test_call_new_definition_with_invalid_number_of_balances_1() {
        let pre_states = vec![AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([1; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([2; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([3; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([5; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([6; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([7; 32])},
        ];
        let balance_a = 15u128;
        let token_program_id: [u32;8] = [0; 8];
        let _post_states = new_definition(&pre_states, &[balance_a], token_program_id);
    }
    
    #[should_panic(expected = "Pool account is uninitiated or not authorized")]
    #[test]
    fn test_call_new_definition_with_uninitiated_pool() {
        let pre_states = vec![AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([0; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([2; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([3; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([5; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([6; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([7; 32])},
        ];
        let balance_a = 15u128;
        let balance_b = 15u128;
        let token_program_id: [u32;8] = [0; 8];
        let _post_states = new_definition(&pre_states, &[balance_a, balance_b], token_program_id);
    }

    #[should_panic(expected = "Pool account is uninitiated or not authorized")]
    #[test]
    fn test_call_new_definition_with_unauthorized_pool() {
        let pre_states = vec![AccountWithMetadata {
            account: Account::default(),
            is_authorized: false,
            account_id: AccountId::new([0; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([2; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([3; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([5; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([6; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([7; 32])},
        ];
        let balance_a = 15u128;
        let balance_b = 15u128;
        let token_program_id: [u32;8] = [0; 8];
        let _post_states = new_definition(&pre_states, &[balance_a, balance_b], token_program_id);
    }

    #[should_panic(expected = "Pool LP must be initialized first")]
    #[test]
    fn test_call_new_definition_with_uninitated_pool_lp() {
        let mut pool = Account::default();
        let mut vault_a = Account::default();
        let mut vault_b = Account::default();

        pool.data = vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];
        vault_a.data = vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];
        vault_b.data = vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];

        let pre_states = vec![AccountWithMetadata {
            account: pool,
            is_authorized: true,
            account_id: AccountId::new([0; 32])},
            AccountWithMetadata {
            account: vault_a,
            is_authorized: true,
            account_id: AccountId::new([2; 32])},
            AccountWithMetadata {
            account: vault_b,
            is_authorized: true,
            account_id: AccountId::new([3; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([5; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([6; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([7; 32])},
        ];
        let balance_a = 15u128;
        let balance_b = 15u128;
        let token_program_id: [u32;8] = [0; 8];
        let _post_states = new_definition(&pre_states, &[balance_a, balance_b], token_program_id);
    }
    
    #[should_panic(expected = "Balances must be nonzero")]
    #[test]
    fn test_call_new_definition_with_balance_zero_1() {
        let mut pool = Account::default();
        let mut vault_a = Account::default();
        let mut vault_b = Account::default();
        let mut pool_lp = Account::default();

        pool.data = vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];
        vault_a.data = vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];
        vault_b.data = vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];
        pool_lp.data = vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];

        let pre_states = vec![AccountWithMetadata {
            account: pool,
            is_authorized: true,
            account_id: AccountId::new([0; 32])},
            AccountWithMetadata {
            account: vault_a,
            is_authorized: true,
            account_id: AccountId::new([2; 32])},
            AccountWithMetadata {
            account: vault_b,
            is_authorized: true,
            account_id: AccountId::new([3; 32])},
            AccountWithMetadata {
            account: pool_lp,
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([5; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([6; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([7; 32])},
        ];
        let balance_a = 0u128;
        let balance_b = 15u128;
        let token_program_id: [u32;8] = [0; 8];
        let _post_states = new_definition(&pre_states, &[balance_a, balance_b], token_program_id);
    }

    #[should_panic(expected = "Balances must be nonzero")]
    #[test]
    fn test_call_new_definition_with_balance_zero_2() {
        let mut pool = Account::default();
        let mut vault_a = Account::default();
        let mut vault_b = Account::default();
        let mut pool_lp = Account::default();

        pool.data = vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];
        vault_a.data = vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];
        vault_b.data = vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];
        pool_lp.data = vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];

        let pre_states = vec![AccountWithMetadata {
            account: pool,
            is_authorized: true,
            account_id: AccountId::new([0; 32])},
            AccountWithMetadata {
            account: vault_a,
            is_authorized: true,
            account_id: AccountId::new([2; 32])},
            AccountWithMetadata {
            account: vault_b,
            is_authorized: true,
            account_id: AccountId::new([3; 32])},
            AccountWithMetadata {
            account: pool_lp,
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([5; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([6; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([7; 32])},
        ];
        let balance_a = 15u128;
        let balance_b = 0u128;
        let token_program_id: [u32;8] = [0; 8];
        let _post_states = new_definition(&pre_states, &[balance_a, balance_b], token_program_id);
    }

    #[should_panic(expected = "Vaults are for the same token")]
    #[test]
    fn test_call_new_definition_same_token() {
        let mut pool = Account::default();
        let mut vault_a = Account::default();
        let mut vault_b = Account::default();
        let mut pool_lp = Account::default();

        vault_a.data = vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];
        vault_b.data = vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];
        pool_lp.data = vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];

        let definition_token_a_id = AccountId::new([1;32]);
        let definition_token_b_id = AccountId::new([2;32]);
        let vault_a_addr = AccountId::new([5;32]);
        let vault_b_addr = AccountId::new([6;32]);
        let liquidity_pool_id = AccountId::new([7;32]);
        let liquidity_pool_cap: u128 = 30u128;
        let reserve_a: u128 = 10;
        let reserve_b: u128 = 20;
        let token_program_id: [u32;8] = [0; 8];

        pool.data = PoolDefinition::into_data( PoolDefinition {
            definition_token_a_id,
            definition_token_b_id,
            vault_a_addr: vault_a_addr.clone(),
            vault_b_addr: vault_b_addr.clone(),
            liquidity_pool_id,
            liquidity_pool_cap,
            reserve_a,
            reserve_b,
            token_program_id,
        });

        let pre_states = vec![AccountWithMetadata {
            account: pool,
            is_authorized: true,
            account_id: AccountId::new([0; 32])},
            AccountWithMetadata {
            account: vault_a,
            is_authorized: true,
            account_id: vault_b_addr.clone()},
            AccountWithMetadata {
            account: vault_b,
            is_authorized: true,
            account_id: vault_b_addr},
            AccountWithMetadata {
            account: pool_lp,
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([5; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([6; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([7; 32])},
        ];
        
        let balance_a = 15u128;
        let balance_b = 15u128;
        let token_program_id: [u32;8] = [0; 8];
        
        let _post_states = new_definition(&pre_states, &[balance_a, balance_b], token_program_id);
    }

    #[should_panic(expected = "Vaults are for the same token")]
    #[test]
    fn test_call_new_definition_success() {
        let mut pool = Account::default();
        let mut vault_a = Account::default();
        let mut vault_b = Account::default();
        let mut pool_lp = Account::default();

        pool.data = vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];
        vault_a.data = vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];
        vault_b.data = vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];
        pool_lp.data = vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];

        let pre_states = vec![AccountWithMetadata {
            account: pool,
            is_authorized: true,
            account_id: AccountId::new([0; 32])},
            AccountWithMetadata {
            account: vault_a,
            is_authorized: true,
            account_id: AccountId::new([2; 32])},
            AccountWithMetadata {
            account: vault_b,
            is_authorized: true,
            account_id: AccountId::new([3; 32])},
            AccountWithMetadata {
            account: pool_lp,
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([5; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([6; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([7; 32])},
        ];
        let balance_a = 15u128;
        let balance_b = 15u128;
        let token_program_id: [u32;8] = [0; 8];
        let _post_states = new_definition(&pre_states, &[balance_a, balance_b], token_program_id);
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]    
    fn test_call_remove_liquidity_with_invalid_number_of_accounts_1() {
        let pre_states = vec![AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([1; 32]),
        }];
        let _post_states = remove_liquidity(&pre_states);
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_remove_liquidity_with_invalid_number_of_accounts_2() {
        let pre_states = vec![AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([1; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([2; 32])},
        ];
        let _post_states = remove_liquidity(&pre_states);
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_remove_liquidity_with_invalid_number_of_accounts_3() {
        let pre_states = vec![AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([1; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([2; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([3; 32])},
        ];
        let _post_states = remove_liquidity(&pre_states);
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_remove_liquidity_with_invalid_number_of_accounts_4() {
        let pre_states = vec![AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([1; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([2; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([3; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
        ];
        let _post_states = remove_liquidity(&pre_states);
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_remove_liquidity_with_invalid_number_of_accounts_5() {
        let pre_states = vec![AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([1; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([2; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([3; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([5; 32])},
        ];
        let _post_states = remove_liquidity(&pre_states);
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_remove_liquidity_with_invalid_number_of_accounts_6() {
        let pre_states = vec![AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([1; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([2; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([3; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([5; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([6; 32])},
        ];
        let _post_states = remove_liquidity(&pre_states);
    }

    #[should_panic(expected = "Vault A was not provided")]
    #[test]
    fn test_call_remove_liquidity_vault_a_omitted() {
        let mut pool = Account::default();
        let mut vault1 = Account::default();
        let mut vault2 = Account::default();
        let mut pool_lp = Account::default();


        let definition_token_a_id = AccountId::new([1;32]);
        let definition_token_b_id = AccountId::new([2;32]); 

        vault1.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_b_id.clone(),
                balance: 15u128 }
        );

        vault2.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_b_id.clone(),
                balance: 15u128 }
        );

        pool_lp.data = vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];

        let vault_a_addr = AccountId::new([5;32]);
        let vault_b_addr = AccountId::new([6;32]);
        let liquidity_pool_id = AccountId::new([7;32]);
        let liquidity_pool_cap: u128 = 30u128;
        let reserve_a: u128 = 10;
        let reserve_b: u128 = 20;
        let token_program_id: [u32;8] = [0; 8];

        pool.data = PoolDefinition::into_data( PoolDefinition {
            definition_token_a_id,
            definition_token_b_id,
            vault_a_addr: vault_a_addr.clone(),
            vault_b_addr: vault_b_addr.clone(),
            liquidity_pool_id,
            liquidity_pool_cap,
            reserve_a,
            reserve_b,
            token_program_id,
        });

        let pre_states = vec![AccountWithMetadata {
            account: pool,
            is_authorized: true,
            account_id: AccountId::new([0; 32])},
            AccountWithMetadata {
            account: vault1,
            is_authorized: true,
            account_id: vault_b_addr.clone()},
            AccountWithMetadata {
            account: vault2,
            is_authorized: true,
            account_id: vault_b_addr},
            AccountWithMetadata {
            account: pool_lp,
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([5; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([6; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([7; 32])},
        ];
        
        let _post_states = remove_liquidity(&pre_states);
    }

    #[should_panic(expected = "Vault B was not provided")]
    #[test]
    fn test_call_remove_liquidity_vault_b_omitted() {
        let mut pool = Account::default();
        let mut vault1 = Account::default();
        let mut vault2 = Account::default();
        let mut pool_lp = Account::default();


        let definition_token_a_id = AccountId::new([1;32]);
        let definition_token_b_id = AccountId::new([2;32]); 

        vault1.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 15u128 }
        );

        vault2.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 15u128 }
        );

        pool_lp.data = vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];

        let vault_a_addr = AccountId::new([5;32]);
        let vault_b_addr = AccountId::new([6;32]);
        let liquidity_pool_id = AccountId::new([7;32]);
        let liquidity_pool_cap: u128 = 30u128;
        let reserve_a: u128 = 10;
        let reserve_b: u128 = 20;
        let token_program_id: [u32;8] = [0; 8];

        pool.data = PoolDefinition::into_data( PoolDefinition {
            definition_token_a_id,
            definition_token_b_id,
            vault_a_addr: vault_a_addr.clone(),
            vault_b_addr: vault_b_addr.clone(),
            liquidity_pool_id,
            liquidity_pool_cap,
            reserve_a,
            reserve_b,
            token_program_id,
        });

        let pre_states = vec![AccountWithMetadata {
            account: pool,
            is_authorized: true,
            account_id: AccountId::new([0; 32])},
            AccountWithMetadata {
            account: vault1,
            is_authorized: true,
            account_id: vault_a_addr.clone()},
            AccountWithMetadata {
            account: vault2,
            is_authorized: true,
            account_id: vault_a_addr.clone()},
            AccountWithMetadata {
            account: pool_lp,
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([5; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([6; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([7; 32])},
        ];
        
        let _post_states = remove_liquidity(&pre_states);
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]    
    fn test_call_swap_with_invalid_number_of_accounts_1() {
        let pre_states = vec![AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([1; 32]),
        }];

        let amount = 15u128;
        let vault_addr = AccountId::new([1;32]);
        let _post_states = swap(&pre_states, amount, vault_addr);
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_swap_with_invalid_number_of_accounts_2() {
        let pre_states = vec![AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([1; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([2; 32])},
        ];
        let amount = 15u128;
        let vault_addr = AccountId::new([1;32]);
        let _post_states = swap(&pre_states, amount, vault_addr);
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_swap_with_invalid_number_of_accounts_3() {
        let pre_states = vec![AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([1; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([2; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([3; 32])},
        ];
        let amount = 15u128;
        let vault_addr = AccountId::new([1;32]);
        let _post_states = swap(&pre_states, amount, vault_addr);
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_swap_with_invalid_number_of_accounts_4() {
        let pre_states = vec![AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([1; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([2; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([3; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
        ];
        let amount = 15u128;
        let vault_addr = AccountId::new([1;32]);
        let _post_states = swap(&pre_states, amount, vault_addr);
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]    
    fn test_call_add_liquidity_with_invalid_number_of_accounts_1() {
        let pre_states = vec![AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([1; 32]),
        }];
        let balance_a = 15u128;
        let balance_b = 15u128;
        let vault_addr = AccountId::new([1;32]);
        let _post_states = add_liquidity(&pre_states, &[balance_a, balance_b], vault_addr);
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_add_liquidity_with_invalid_number_of_accounts_2() {
        let pre_states = vec![AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([1; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([2; 32])},
        ];
        let balance_a = 15u128;
        let balance_b = 15u128;
        let vault_addr = AccountId::new([1;32]);
        let _post_states = add_liquidity(&pre_states, &[balance_a, balance_b], vault_addr);
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_add_liquidity_with_invalid_number_of_accounts_3() {
        let pre_states = vec![AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([1; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([2; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([3; 32])},
        ];
        let balance_a = 15u128;
        let balance_b = 15u128;
        let vault_addr = AccountId::new([1;32]);
        let _post_states = add_liquidity(&pre_states, &[balance_a, balance_b], vault_addr);
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_add_liquidity_with_invalid_number_of_accounts_4() {
        let pre_states = vec![AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([1; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([2; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([3; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
        ];
        let balance_a = 15u128;
        let balance_b = 15u128;
        let vault_addr = AccountId::new([1;32]);
        let _post_states = add_liquidity(&pre_states, &[balance_a, balance_b], vault_addr);
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_add_liquidity_with_invalid_number_of_accounts_5() {
        let pre_states = vec![AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([1; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([2; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([3; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([5; 32])},
        ];
        let balance_a = 15u128;
        let balance_b = 15u128;
        let vault_addr = AccountId::new([1;32]);
        let _post_states = add_liquidity(&pre_states, &[balance_a, balance_b], vault_addr);
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_add_liquidity_with_invalid_number_of_accounts_6() {
        let pre_states = vec![AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([1; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([2; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([3; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([5; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([6; 32])},
        ];
        let balance_a = 15u128;
        let balance_b = 15u128;
        let vault_addr = AccountId::new([1;32]);
        let _post_states = add_liquidity(&pre_states, &[balance_a, balance_b], vault_addr);
    }

    #[should_panic(expected = "Invalid number of input balances")]
    #[test]
    fn test_call_add_liquidity_invalid_number_of_balances_1() {
        let mut pool = Account::default();
        let mut vault1 = Account::default();
        let mut vault2 = Account::default();
        let mut pool_lp = Account::default();


        let definition_token_a_id = AccountId::new([1;32]);
        let definition_token_b_id = AccountId::new([2;32]); 

        vault1.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 15u128 }
        );

        vault2.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_b_id.clone(),
                balance: 15u128 }
        );

        pool_lp.data = vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];

        let vault_a_addr = AccountId::new([5;32]);
        let vault_b_addr = AccountId::new([6;32]);
        let liquidity_pool_id = AccountId::new([7;32]);
        let liquidity_pool_cap: u128 = 30u128;
        let reserve_a: u128 = 10;
        let reserve_b: u128 = 20;
        let token_program_id: [u32;8] = [0; 8];

        pool.data = PoolDefinition::into_data( PoolDefinition {
            definition_token_a_id: definition_token_a_id.clone(),
            definition_token_b_id: definition_token_b_id.clone(),
            vault_a_addr: vault_a_addr.clone(),
            vault_b_addr: vault_b_addr.clone(),
            liquidity_pool_id,
            liquidity_pool_cap,
            reserve_a,
            reserve_b,
            token_program_id,
        });

        let pre_states = vec![AccountWithMetadata {
            account: pool,
            is_authorized: true,
            account_id: AccountId::new([0; 32])},
            AccountWithMetadata {
            account: vault1,
            is_authorized: true,
            account_id: vault_a_addr.clone()},
            AccountWithMetadata {
            account: vault2,
            is_authorized: true,
            account_id: vault_b_addr.clone()},
            AccountWithMetadata {
            account: pool_lp,
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([5; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([6; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([7; 32])},
        ];
        let balance_a = 15u128;
        let balance_b = 15u128;
        let main_token = definition_token_a_id.clone();
        let _post_states = add_liquidity(&pre_states, &[balance_a], main_token);
    }

    #[should_panic(expected = "Vault A was not provided")]
    #[test]
    fn test_call_add_liquidity_vault_a_omitted() {
        let mut pool = Account::default();
        let mut vault1 = Account::default();
        let mut vault2 = Account::default();
        let mut pool_lp = Account::default();


        let definition_token_a_id = AccountId::new([1;32]);
        let definition_token_b_id = AccountId::new([2;32]); 

        vault1.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_b_id.clone(),
                balance: 15u128 }
        );

        vault2.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_b_id.clone(),
                balance: 15u128 }
        );

        pool_lp.data = vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];

        let vault_a_addr = AccountId::new([5;32]);
        let vault_b_addr = AccountId::new([6;32]);
        let liquidity_pool_id = AccountId::new([7;32]);
        let liquidity_pool_cap: u128 = 30u128;
        let reserve_a: u128 = 10;
        let reserve_b: u128 = 20;
        let token_program_id: [u32;8] = [0; 8];

        pool.data = PoolDefinition::into_data( PoolDefinition {
            definition_token_a_id,
            definition_token_b_id,
            vault_a_addr: vault_a_addr.clone(),
            vault_b_addr: vault_b_addr.clone(),
            liquidity_pool_id,
            liquidity_pool_cap,
            reserve_a,
            reserve_b,
            token_program_id,
        });

        let pre_states = vec![AccountWithMetadata {
            account: pool,
            is_authorized: true,
            account_id: AccountId::new([0; 32])},
            AccountWithMetadata {
            account: vault1,
            is_authorized: true,
            account_id: vault_b_addr.clone()},
            AccountWithMetadata {
            account: vault2,
            is_authorized: true,
            account_id: vault_b_addr},
            AccountWithMetadata {
            account: pool_lp,
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([5; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([6; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([7; 32])},
        ];
        let balance_a = 15u128;
        let balance_b = 15u128;
        let vault_addr = AccountId::new([1;32]);
        let _post_states = add_liquidity(&pre_states, &[balance_a,balance_b], vault_addr);
    }

    #[should_panic(expected = "Vault B was not provided")]
    #[test]
    fn test_call_add_liquidity_vault_b_omitted() {
        let mut pool = Account::default();
        let mut vault1 = Account::default();
        let mut vault2 = Account::default();
        let mut pool_lp = Account::default();


        let definition_token_a_id = AccountId::new([1;32]);
        let definition_token_b_id = AccountId::new([2;32]); 

        vault1.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 15u128 }
        );

        vault2.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 15u128 }
        );


        pool_lp.data = vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];

        let vault_a_addr = AccountId::new([5;32]);
        let vault_b_addr = AccountId::new([6;32]);
        let liquidity_pool_id = AccountId::new([7;32]);
        let liquidity_pool_cap: u128 = 30u128;
        let reserve_a: u128 = 10;
        let reserve_b: u128 = 20;
        let token_program_id: [u32;8] = [0; 8];

        pool.data = PoolDefinition::into_data( PoolDefinition {
            definition_token_a_id,
            definition_token_b_id,
            vault_a_addr: vault_a_addr.clone(),
            vault_b_addr: vault_b_addr.clone(),
            liquidity_pool_id,
            liquidity_pool_cap,
            reserve_a,
            reserve_b,
            token_program_id,
        });

        let pre_states = vec![AccountWithMetadata {
            account: pool,
            is_authorized: true,
            account_id: AccountId::new([0; 32])},
            AccountWithMetadata {
            account: vault1,
            is_authorized: true,
            account_id: vault_a_addr.clone()},
            AccountWithMetadata {
            account: vault2,
            is_authorized: true,
            account_id: vault_a_addr.clone()},
            AccountWithMetadata {
            account: pool_lp,
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([5; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([6; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([7; 32])},
        ];
        let balance_a = 15u128;
        let balance_b = 15u128;
        let vault_addr = AccountId::new([1;32]);
        let _post_states = add_liquidity(&pre_states, &[balance_a,balance_b], vault_addr);
    }

    #[should_panic(expected = "Both max-balances must be nonzero")]
    #[test]
    fn test_call_add_liquidity_zero_balance_1() {
        let mut pool = Account::default();
        let mut vault1 = Account::default();
        let mut vault2 = Account::default();
        let mut pool_lp = Account::default();


        let definition_token_a_id = AccountId::new([1;32]);
        let definition_token_b_id = AccountId::new([2;32]); 

        vault1.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 15u128 }
        );

        vault2.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_b_id.clone(),
                balance: 15u128 }
        );

        pool_lp.data = vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];

        let vault_a_addr = AccountId::new([5;32]);
        let vault_b_addr = AccountId::new([6;32]);
        let liquidity_pool_id = AccountId::new([7;32]);
        let liquidity_pool_cap: u128 = 30u128;
        let reserve_a: u128 = 10;
        let reserve_b: u128 = 20;
        let token_program_id: [u32;8] = [0; 8];

        pool.data = PoolDefinition::into_data( PoolDefinition {
            definition_token_a_id: definition_token_a_id.clone(),
            definition_token_b_id: definition_token_b_id.clone(),
            vault_a_addr: vault_a_addr.clone(),
            vault_b_addr: vault_b_addr.clone(),
            liquidity_pool_id,
            liquidity_pool_cap,
            reserve_a,
            reserve_b,
            token_program_id,
        });

        let pre_states = vec![AccountWithMetadata {
            account: pool,
            is_authorized: true,
            account_id: AccountId::new([0; 32])},
            AccountWithMetadata {
            account: vault1,
            is_authorized: true,
            account_id: vault_a_addr.clone()},
            AccountWithMetadata {
            account: vault2,
            is_authorized: true,
            account_id: vault_b_addr.clone()},
            AccountWithMetadata {
            account: pool_lp,
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([5; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([6; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([7; 32])},
        ];
        let balance_a = 0u128;
        let balance_b = 15u128;
        let main_token = definition_token_a_id.clone();
        let _post_states = add_liquidity(&pre_states, &[balance_a,balance_b], main_token);
    }

    #[should_panic(expected = "Both max-balances must be nonzero")]
    #[test]
    fn test_call_add_liquidity_zero_balance_2() {
        let mut pool = Account::default();
        let mut vault1 = Account::default();
        let mut vault2 = Account::default();
        let mut pool_lp = Account::default();


        let definition_token_a_id = AccountId::new([1;32]);
        let definition_token_b_id = AccountId::new([2;32]); 

        vault1.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 15u128 }
        );

        vault2.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_b_id.clone(),
                balance: 15u128 }
        );

        pool_lp.data = vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];

        let vault_a_addr = AccountId::new([5;32]);
        let vault_b_addr = AccountId::new([6;32]);
        let liquidity_pool_id = AccountId::new([7;32]);
        let liquidity_pool_cap: u128 = 30u128;
        let reserve_a: u128 = 10;
        let reserve_b: u128 = 20;
        let token_program_id: [u32;8] = [0; 8];

        pool.data = PoolDefinition::into_data( PoolDefinition {
            definition_token_a_id: definition_token_a_id.clone(),
            definition_token_b_id: definition_token_b_id.clone(),
            vault_a_addr: vault_a_addr.clone(),
            vault_b_addr: vault_b_addr.clone(),
            liquidity_pool_id,
            liquidity_pool_cap,
            reserve_a,
            reserve_b,
            token_program_id,
        });

        let pre_states = vec![AccountWithMetadata {
            account: pool,
            is_authorized: true,
            account_id: AccountId::new([0; 32])},
            AccountWithMetadata {
            account: vault1,
            is_authorized: true,
            account_id: vault_a_addr.clone()},
            AccountWithMetadata {
            account: vault2,
            is_authorized: true,
            account_id: vault_b_addr.clone()},
            AccountWithMetadata {
            account: pool_lp,
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([5; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([6; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([7; 32])},
        ];
        let balance_a = 15u128;
        let balance_b = 0u128;
        let main_token = definition_token_a_id.clone();
        let _post_states = add_liquidity(&pre_states, &[balance_a,balance_b], main_token);
    }

    #[should_panic(expected = "Mismatch of token types")]
    #[test]
    fn test_call_add_liquidity_incorrect_token_type() {
        let mut pool = Account::default();
        let mut vault1 = Account::default();
        let mut vault2 = Account::default();
        let mut pool_lp = Account::default();


        let definition_token_a_id = AccountId::new([1;32]);
        let definition_token_b_id = AccountId::new([2;32]); 

        vault1.balance = 15u128;
        vault2.balance = 15u128;

        vault1.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 15u128 }
        );

        vault2.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_b_id.clone(),
                balance: 15u128 }
        );

        pool_lp.data = vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];

        let vault_a_addr = AccountId::new([5;32]);
        let vault_b_addr = AccountId::new([6;32]);
        let liquidity_pool_id = AccountId::new([7;32]);
        let liquidity_pool_cap: u128 = 30u128;
        let reserve_a: u128 = 10;
        let reserve_b: u128 = 20;
        let token_program_id: [u32;8] = [0; 8];

        pool.data = PoolDefinition::into_data( PoolDefinition {
            definition_token_a_id,
            definition_token_b_id,
            vault_a_addr: vault_a_addr.clone(),
            vault_b_addr: vault_b_addr.clone(),
            liquidity_pool_id,
            liquidity_pool_cap,
            reserve_a,
            reserve_b,
            token_program_id,
        });

        let pre_states = vec![AccountWithMetadata {
            account: pool,
            is_authorized: true,
            account_id: AccountId::new([0; 32])},
            AccountWithMetadata {
            account: vault1,
            is_authorized: true,
            account_id: vault_a_addr.clone()},
            AccountWithMetadata {
            account: vault2,
            is_authorized: true,
            account_id: vault_b_addr.clone()},
            AccountWithMetadata {
            account: pool_lp,
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([5; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([6; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([7; 32])},
        ];
        let balance_a = 15u128;
        let balance_b = 15u128;
        let main_token = AccountId::new([9;32]);
        let _post_states = add_liquidity(&pre_states, &[balance_a,balance_b], main_token);
    }

    #[should_panic(expected = "Vaults must have nonzero balances")]
    #[test]
    fn test_call_add_liquidity_zero_vault_balance_1() {
        let mut pool = Account::default();
        let mut vault1 = Account::default();
        let mut vault2 = Account::default();
        let mut pool_lp = Account::default();


        let definition_token_a_id = AccountId::new([1;32]);
        let definition_token_b_id = AccountId::new([2;32]); 

        vault1.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 15u128 }
        );

        vault1.balance = 0;
        vault2.balance = 15;

        vault2.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_b_id.clone(),
                balance: 15u128 }
        );

        pool_lp.data = vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];

        let vault_a_addr = AccountId::new([5;32]);
        let vault_b_addr = AccountId::new([6;32]);
        let liquidity_pool_id = AccountId::new([7;32]);
        let liquidity_pool_cap: u128 = 30u128;
        let reserve_a: u128 = 10;
        let reserve_b: u128 = 20;
        let token_program_id: [u32;8] = [0; 8];

        pool.data = PoolDefinition::into_data( PoolDefinition {
            definition_token_a_id: definition_token_a_id.clone(),
            definition_token_b_id: definition_token_b_id.clone(),
            vault_a_addr: vault_a_addr.clone(),
            vault_b_addr: vault_b_addr.clone(),
            liquidity_pool_id,
            liquidity_pool_cap,
            reserve_a,
            reserve_b,
            token_program_id,
        });

        let pre_states = vec![AccountWithMetadata {
            account: pool,
            is_authorized: true,
            account_id: AccountId::new([0; 32])},
            AccountWithMetadata {
            account: vault1,
            is_authorized: true,
            account_id: vault_a_addr.clone()},
            AccountWithMetadata {
            account: vault2,
            is_authorized: true,
            account_id: vault_b_addr.clone()},
            AccountWithMetadata {
            account: pool_lp,
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([5; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([6; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([7; 32])},
        ];
        let balance_a = 15u128;
        let balance_b = 15u128;
        let main_token = definition_token_a_id.clone();
        let _post_states = add_liquidity(&pre_states, &[balance_a,balance_b], main_token);
    }

    #[should_panic(expected = "Vaults must have nonzero balances")]
    #[test]
    fn test_call_add_liquidity_zero_vault_balance_2() {
        let mut pool = Account::default();
        let mut vault1 = Account::default();
        let mut vault2 = Account::default();
        let mut pool_lp = Account::default();


        let definition_token_a_id = AccountId::new([1;32]);
        let definition_token_b_id = AccountId::new([2;32]); 

        vault1.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 15u128 }
        );

        vault1.balance = 15;
        vault2.balance = 0;

        vault2.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_b_id.clone(),
                balance: 15u128 }
        );

        pool_lp.data = vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];

        let vault_a_addr = AccountId::new([5;32]);
        let vault_b_addr = AccountId::new([6;32]);
        let liquidity_pool_id = AccountId::new([7;32]);
        let liquidity_pool_cap: u128 = 30u128;
        let reserve_a: u128 = 10;
        let reserve_b: u128 = 20;
        let token_program_id: [u32;8] = [0; 8];

        pool.data = PoolDefinition::into_data( PoolDefinition {
            definition_token_a_id: definition_token_a_id.clone(),
            definition_token_b_id: definition_token_b_id.clone(),
            vault_a_addr: vault_a_addr.clone(),
            vault_b_addr: vault_b_addr.clone(),
            liquidity_pool_id,
            liquidity_pool_cap,
            reserve_a,
            reserve_b,
            token_program_id,
        });

        let pre_states = vec![AccountWithMetadata {
            account: pool,
            is_authorized: true,
            account_id: AccountId::new([0; 32])},
            AccountWithMetadata {
            account: vault1,
            is_authorized: true,
            account_id: vault_a_addr.clone()},
            AccountWithMetadata {
            account: vault2,
            is_authorized: true,
            account_id: vault_b_addr.clone()},
            AccountWithMetadata {
            account: pool_lp,
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([5; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([6; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([7; 32])},
        ];
        let balance_a = 15u128;
        let balance_b = 15u128;
        let main_token = definition_token_a_id.clone();
        let _post_states = add_liquidity(&pre_states, &[balance_a,balance_b], main_token);
    }

   #[should_panic(expected = "Insufficient balance")]
    #[test]
    fn test_call_add_liquidity_insufficient_balance_1() {
        let mut pool = Account::default();
        let mut vault1 = Account::default();
        let mut vault2 = Account::default();
        let mut pool_lp = Account::default();


        let definition_token_a_id = AccountId::new([1;32]);
        let definition_token_b_id = AccountId::new([2;32]); 

        vault1.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 15u128 }
        );

        vault1.balance = 15u128;
        vault2.balance = 15u128;

        vault2.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_b_id.clone(),
                balance: 15u128 }
        );

        pool_lp.data = vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];

        let vault_a_addr = AccountId::new([5;32]);
        let vault_b_addr = AccountId::new([6;32]);
        let liquidity_pool_id = AccountId::new([7;32]);
        let liquidity_pool_cap: u128 = 30u128;
        let reserve_a: u128 = 15;
        let reserve_b: u128 = 15;
        let token_program_id: [u32;8] = [0; 8];

        pool.data = PoolDefinition::into_data( PoolDefinition {
            definition_token_a_id: definition_token_a_id.clone(),
            definition_token_b_id: definition_token_b_id.clone(),
            vault_a_addr: vault_a_addr.clone(),
            vault_b_addr: vault_b_addr.clone(),
            liquidity_pool_id,
            liquidity_pool_cap,
            reserve_a,
            reserve_b,
            token_program_id,
        });

        let pre_states = vec![AccountWithMetadata {
            account: pool,
            is_authorized: true,
            account_id: AccountId::new([0; 32])},
            AccountWithMetadata {
            account: vault1,
            is_authorized: true,
            account_id: vault_a_addr.clone()},
            AccountWithMetadata {
            account: vault2,
            is_authorized: true,
            account_id: vault_b_addr.clone()},
            AccountWithMetadata {
            account: pool_lp,
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([5; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([6; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([7; 32])},
        ];
        let balance_a = 15u128;
        let balance_b = 15u128;
        let main_token = definition_token_a_id.clone();
        let _post_states = add_liquidity(&pre_states, &[balance_a,balance_b], main_token);
    }

   #[should_panic(expected = "Insufficient balance")]
    #[test]
    fn test_call_add_liquidity_insufficient_balance_2() {
        let mut pool = Account::default();
        let mut vault1 = Account::default();
        let mut vault2 = Account::default();
        let mut pool_lp = Account::default();


        let definition_token_a_id = AccountId::new([1;32]);
        let definition_token_b_id = AccountId::new([2;32]); 

        vault1.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 15u128 }
        );

        vault1.balance = 15u128;
        vault2.balance = 15u128;

        vault2.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_b_id.clone(),
                balance: 15u128 }
        );

        pool_lp.data = vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];

        let vault_a_addr = AccountId::new([5;32]);
        let vault_b_addr = AccountId::new([6;32]);
        let liquidity_pool_id = AccountId::new([7;32]);
        let liquidity_pool_cap: u128 = 30u128;
        let reserve_a: u128 = 10;
        let reserve_b: u128 = 20;
        let token_program_id: [u32;8] = [0; 8];

        pool.data = PoolDefinition::into_data( PoolDefinition {
            definition_token_a_id: definition_token_a_id.clone(),
            definition_token_b_id: definition_token_b_id.clone(),
            vault_a_addr: vault_a_addr.clone(),
            vault_b_addr: vault_b_addr.clone(),
            liquidity_pool_id,
            liquidity_pool_cap,
            reserve_a,
            reserve_b,
            token_program_id,
        });

        let mut user_a = Account::default();
        let mut user_b = Account::default();

        user_a.balance = 15u128;
        user_b.balance = 12u128;

        let pre_states = vec![AccountWithMetadata {
            account: pool,
            is_authorized: true,
            account_id: AccountId::new([0; 32])},
            AccountWithMetadata {
            account: vault1,
            is_authorized: true,
            account_id: vault_a_addr.clone()},
            AccountWithMetadata {
            account: vault2,
            is_authorized: true,
            account_id: vault_b_addr.clone()},
            AccountWithMetadata {
            account: pool_lp,
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
            AccountWithMetadata {
            account: user_a,
            is_authorized: true,
            account_id: AccountId::new([5; 32])},
            AccountWithMetadata {
            account: user_b,
            is_authorized: true,
            account_id: AccountId::new([6; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([7; 32])},
        ];
        let balance_a = 15u128;
        let balance_b = 15u128;
        let main_token = definition_token_b_id.clone();
        let _post_states = add_liquidity(&pre_states, &[balance_a,balance_b], main_token);
    }

   #[should_panic(expected = "A trade amount is 0")]
    #[test]
    fn test_call_add_liquidity_actual_trade_insufficient() {
        let mut pool = Account::default();
        let mut vault1 = Account::default();
        let mut vault2 = Account::default();
        let mut pool_lp = Account::default();


        let definition_token_a_id = AccountId::new([1;32]);
        let definition_token_b_id = AccountId::new([2;32]); 

        vault1.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 15u128 }
        );

        vault1.balance = 1500u128;
        vault2.balance = 2000u128;

        vault2.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_b_id.clone(),
                balance: 15u128 }
        );

        pool_lp.data = vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];

        let vault_a_addr = AccountId::new([5;32]);
        let vault_b_addr = AccountId::new([6;32]);
        let liquidity_pool_id = AccountId::new([7;32]);
        let liquidity_pool_cap: u128 = 30u128;
        let reserve_a: u128 = 1500u128;
        let reserve_b: u128 = 2000u128;
        let token_program_id: [u32;8] = [0; 8];

        pool.data = PoolDefinition::into_data( PoolDefinition {
            definition_token_a_id: definition_token_a_id.clone(),
            definition_token_b_id: definition_token_b_id.clone(),
            vault_a_addr: vault_a_addr.clone(),
            vault_b_addr: vault_b_addr.clone(),
            liquidity_pool_id,
            liquidity_pool_cap,
            reserve_a,
            reserve_b,
            token_program_id,
        });

        let mut user_a = Account::default();
        let mut user_b = Account::default();

        user_a.balance = 150u128;
        user_b.balance = 150u128;

        let pre_states = vec![AccountWithMetadata {
            account: pool,
            is_authorized: true,
            account_id: AccountId::new([0; 32])},
            AccountWithMetadata {
            account: vault1,
            is_authorized: true,
            account_id: vault_a_addr.clone()},
            AccountWithMetadata {
            account: vault2,
            is_authorized: true,
            account_id: vault_b_addr.clone()},
            AccountWithMetadata {
            account: pool_lp,
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
            AccountWithMetadata {
            account: user_a,
            is_authorized: true,
            account_id: AccountId::new([5; 32])},
            AccountWithMetadata {
            account: user_b,
            is_authorized: true,
            account_id: AccountId::new([6; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([7; 32])},
        ];
        let balance_a = 1u128;
        let balance_b = 1u128;
        let main_token = definition_token_b_id.clone();
        let _post_states = add_liquidity(&pre_states, &[balance_a,balance_b], main_token);
    }

    #[should_panic(expected = "Reserves must be nonzero")]
    #[test]
    fn test_call_add_liquidity_reserves_zero() {
        let mut pool = Account::default();
        let mut vault1 = Account::default();
        let mut vault2 = Account::default();
        let mut pool_lp = Account::default();


        let definition_token_a_id = AccountId::new([1;32]);
        let definition_token_b_id = AccountId::new([2;32]); 

        vault1.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 15u128 }
        );

        vault1.balance = 15u128;
        vault2.balance = 20u128;

        vault2.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_b_id.clone(),
                balance: 15u128 }
        );

        pool_lp.data = vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];

        let vault_a_addr = AccountId::new([5;32]);
        let vault_b_addr = AccountId::new([6;32]);
        let liquidity_pool_id = AccountId::new([7;32]);
        let liquidity_pool_cap: u128 = 30u128;
        let reserve_a: u128 = 15;
        let reserve_b: u128 = 0;
        let token_program_id: [u32;8] = [0; 8];

        pool.data = PoolDefinition::into_data( PoolDefinition {
            definition_token_a_id: definition_token_a_id.clone(),
            definition_token_b_id: definition_token_b_id.clone(),
            vault_a_addr: vault_a_addr.clone(),
            vault_b_addr: vault_b_addr.clone(),
            liquidity_pool_id,
            liquidity_pool_cap,
            reserve_a,
            reserve_b,
            token_program_id,
        });

        let mut user_a = Account::default();
        let mut user_b = Account::default();

        user_a.balance = 150u128;
        user_b.balance = 150u128;

        let pre_states = vec![AccountWithMetadata {
            account: pool,
            is_authorized: true,
            account_id: AccountId::new([0; 32])},
            AccountWithMetadata {
            account: vault1,
            is_authorized: true,
            account_id: vault_a_addr.clone()},
            AccountWithMetadata {
            account: vault2,
            is_authorized: true,
            account_id: vault_b_addr.clone()},
            AccountWithMetadata {
            account: pool_lp,
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
            AccountWithMetadata {
            account: user_a,
            is_authorized: true,
            account_id: AccountId::new([5; 32])},
            AccountWithMetadata {
            account: user_b,
            is_authorized: true,
            account_id: AccountId::new([6; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([7; 32])},
        ];
        let balance_a = 15u128;
        let balance_b = 15u128;
        let main_token = definition_token_b_id.clone();
        let _post_states = add_liquidity(&pre_states, &[balance_a,balance_b], main_token);
    }

    #[should_panic(expected = "Address is not a token type for the pool")]
    #[test]
    fn test_call_swap_incorrect_token_type() {
        let mut pool = Account::default();
        let mut vault1 = Account::default();
        let mut vault2 = Account::default();
        let mut pool_lp = Account::default();


        let definition_token_a_id = AccountId::new([1;32]);
        let definition_token_b_id = AccountId::new([2;32]); 

        vault1.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 15u128 }
        );

        vault2.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_b_id.clone(),
                balance: 15u128 }
        );

        pool_lp.data = vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];

        let definition_token_a_id = AccountId::new([1;32]);
        let definition_token_b_id = AccountId::new([2;32]);
        let vault_a_addr = AccountId::new([5;32]);
        let vault_b_addr = AccountId::new([6;32]);
        let liquidity_pool_id = AccountId::new([7;32]);
        let liquidity_pool_cap: u128 = 30u128;
        let reserve_a: u128 = 10;
        let reserve_b: u128 = 20;
        let token_program_id: [u32;8] = [0; 8];

        pool.data = PoolDefinition::into_data( PoolDefinition {
            definition_token_a_id,
            definition_token_b_id,
            vault_a_addr: vault_a_addr.clone(),
            vault_b_addr: vault_b_addr.clone(),
            liquidity_pool_id,
            liquidity_pool_cap,
            reserve_a,
            reserve_b,
            token_program_id,
        });

        let pre_states = vec![AccountWithMetadata {
            account: pool,
            is_authorized: true,
            account_id: AccountId::new([0; 32])},
            AccountWithMetadata {
            account: vault1,
            is_authorized: true,
            account_id: vault_a_addr.clone()},
            AccountWithMetadata {
            account: vault2,
            is_authorized: true,
            account_id: vault_b_addr.clone()},
            AccountWithMetadata {
            account: pool_lp,
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([5; 32])}
        ];
        let amount = 15u128;
        let main_token = AccountId::new([9;32]);
        let _post_states = swap(&pre_states, amount, main_token);
    }

    #[should_panic(expected = "Vault A was not provided")]
    #[test]
    fn test_call_swap_vault_a_omitted() {
        let mut pool = Account::default();
        let mut vault1 = Account::default();
        let mut vault2 = Account::default();
        let mut pool_lp = Account::default();


        let definition_token_a_id = AccountId::new([1;32]);
        let definition_token_b_id = AccountId::new([2;32]); 

        vault1.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_b_id.clone(),
                balance: 15u128 }
        );

        vault2.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_b_id.clone(),
                balance: 15u128 }
        );

        pool_lp.data = vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];


        let vault_a_addr = AccountId::new([5;32]);
        let vault_b_addr = AccountId::new([6;32]);
        let liquidity_pool_id = AccountId::new([7;32]);
        let liquidity_pool_cap: u128 = 30u128;
        let reserve_a: u128 = 10;
        let reserve_b: u128 = 20;
        let token_program_id: [u32;8] = [0; 8];

        pool.data = PoolDefinition::into_data( PoolDefinition {
            definition_token_a_id,
            definition_token_b_id,
            vault_a_addr: vault_a_addr.clone(),
            vault_b_addr: vault_b_addr.clone(),
            liquidity_pool_id,
            liquidity_pool_cap,
            reserve_a,
            reserve_b,
            token_program_id,
        });

        let pre_states = vec![AccountWithMetadata {
            account: pool,
            is_authorized: true,
            account_id: AccountId::new([0; 32])},
            AccountWithMetadata {
            account: vault1,
            is_authorized: true,
            account_id: vault_b_addr.clone()},
            AccountWithMetadata {
            account: vault2,
            is_authorized: true,
            account_id: vault_b_addr.clone()},
            AccountWithMetadata {
            account: pool_lp,
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([5; 32])}
        ];
        let amount = 15u128;
        let vault_addr = AccountId::new([0;32]);
        let _post_states = swap(&pre_states, amount, vault_addr);
    }

    #[should_panic(expected = "Vault B was not provided")]
    #[test]
    fn test_call_swap_vault_b_omitted() {
        let mut pool = Account::default();
        let mut vault1 = Account::default();
        let mut vault2 = Account::default();
        let mut pool_lp = Account::default();


        let definition_token_a_id = AccountId::new([1;32]);
        let definition_token_b_id = AccountId::new([2;32]); 

        vault1.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 15u128 }
        );

        vault2.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 15u128 }
        );

        pool_lp.data = vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];

        let definition_token_a_id = AccountId::new([1;32]);
        let definition_token_b_id = AccountId::new([2;32]);
        let vault_a_addr = AccountId::new([5;32]);
        let vault_b_addr = AccountId::new([6;32]);
        let liquidity_pool_id = AccountId::new([7;32]);
        let liquidity_pool_cap: u128 = 30u128;
        let reserve_a: u128 = 10;
        let reserve_b: u128 = 20;
        let token_program_id: [u32;8] = [0; 8];

        pool.data = PoolDefinition::into_data( PoolDefinition {
            definition_token_a_id,
            definition_token_b_id,
            vault_a_addr: vault_a_addr.clone(),
            vault_b_addr: vault_b_addr.clone(),
            liquidity_pool_id,
            liquidity_pool_cap,
            reserve_a,
            reserve_b,
            token_program_id,
        });

        let pre_states = vec![AccountWithMetadata {
            account: pool,
            is_authorized: true,
            account_id: AccountId::new([0; 32])},
            AccountWithMetadata {
            account: vault1,
            is_authorized: true,
            account_id: vault_a_addr.clone()},
            AccountWithMetadata {
            account: vault2,
            is_authorized: true,
            account_id: vault_a_addr.clone()},
            AccountWithMetadata {
            account: pool_lp,
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
            AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([5; 32])}
        ];
        let amount = 15u128;
        let vault_addr = AccountId::new([0;32]);
        let _post_states = swap(&pre_states, amount, vault_addr);
    }
}
