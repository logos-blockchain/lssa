use nssa_core::{
    account::{Account, AccountId, AccountWithMetadata, Data},
    program::{ProgramId, ProgramInput, ChainedCall, read_nssa_inputs, write_nssa_outputs_with_chained_call},
};

use bytemuck;

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
//        TODO: ideally, vault_holding_a, vault_holding_b, pool_lp and user_holding_lp are uninitated.
//      * An instruction data of 65-bytes, indicating the initial amm reserves' balances and token_program_id with
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
// - Swap logic
//    Arguments of this function are:
//      * Four accounts: [user_deposit_tx, vault_deposit_tx, vault_withdraw_tx, user_withdraw_tx].
//        user_deposit_tx and vault_deposit_tx define deposit transaction.
//        vault_withdraw_tx and user_withdraw_tx define withdraw transaction.
//      * deposit_amount is the amount for user_deposit_tx -> vault_deposit_tx transfer.
//      * reserve_amounts is the pool's reserves; used to compute the withdraw amount.
//      * Outputs the token transfers as a Vec<ChainedCall> and the withdraw amount.

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
const TOKEN_HOLDING_TYPE: u8 = 1;
const TOKEN_HOLDING_DATA_SIZE: usize = 49;

struct TokenHolding {
    account_type: u8,
    definition_id: AccountId,
    balance: u128,
}

impl TokenHolding {
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
            
            let mut token_program_id: [u32;8] = [0;8];
            token_program_id[0] = u32::from_le_bytes(instruction[33..37].try_into().unwrap());
            token_program_id[1] = u32::from_le_bytes(instruction[37..41].try_into().unwrap());
            token_program_id[2] = u32::from_le_bytes(instruction[41..45].try_into().unwrap());
            token_program_id[3] = u32::from_le_bytes(instruction[45..49].try_into().unwrap());
            token_program_id[4] = u32::from_le_bytes(instruction[49..53].try_into().unwrap());
            token_program_id[5] = u32::from_le_bytes(instruction[53..57].try_into().unwrap());
            token_program_id[6] = u32::from_le_bytes(instruction[57..61].try_into().unwrap());
            token_program_id[7] = u32::from_le_bytes(instruction[61..65].try_into().unwrap());

            let (post_states, chained_call) = new_definition(&pre_states,
                &[balance_a, balance_b],
                token_program_id
                );

            write_nssa_outputs_with_chained_call(pre_states, post_states, chained_call);
        }
        1 => {
            let mut token_addr: [u8;32] = [0;32];
            token_addr[0..].copy_from_slice(&instruction[17..49]);
            
            let token_addr = AccountId::new(token_addr);
            
            let amount = u128::from_le_bytes(instruction[1..17].try_into().unwrap());

            let (post_states, chained_call) = swap(&pre_states, amount, token_addr);

            write_nssa_outputs_with_chained_call(pre_states, post_states, chained_call);
        }
        2 => {

            let balance_a = u128::from_le_bytes(instruction[1..17].try_into().unwrap());
            let balance_b = u128::from_le_bytes(instruction[17..33].try_into().unwrap());

            let mut token_addr: [u8;32] = [0;32];
            token_addr[0..].copy_from_slice(&instruction[33..65]);
            let token_addr = AccountId::new(token_addr);

            let (post_states, chained_call) = add_liquidity(&pre_states,
                        &[balance_a, balance_b], token_addr.clone());
           write_nssa_outputs_with_chained_call(pre_states, post_states, chained_call);
        }
        3 => {

            let (post_states, chained_call) = remove_liquidity(&pre_states);

            write_nssa_outputs_with_chained_call(pre_states, post_states, chained_call);
        }
        _ => panic!("Invalid instruction"),
    };
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

    if pool.account != Account::default() || !pool.is_authorized {
        panic!("Pool account is initiated or not authorized");
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
    let definition_token_a_id = TokenHolding::parse(&user_a.account.data).unwrap().definition_id;
    let definition_token_b_id = TokenHolding::parse(&user_b.account.data).unwrap().definition_id;
   
    if definition_token_a_id == definition_token_b_id {
        panic!("Cannot set up a swap for a token with itself.")
    }

    // 5. Update pool account
    let mut pool_post = Account::default();
    let pool_post_definition = PoolDefinition {
            definition_token_a_id,
            definition_token_b_id,
            vault_a_addr: vault_a.account_id.clone(),
            vault_b_addr: vault_b.account_id.clone(),
            liquidity_pool_id: TokenHolding::parse(&pool_lp.account.data).unwrap().definition_id,
            liquidity_pool_cap: amount_a,
            reserve_a: amount_a,
            reserve_b: amount_b,
            token_program_id: token_program,  
    };

    pool_post.data = pool_post_definition.into_data();

    let mut chained_call = Vec::new();
   
    //Chain call for Token A (User_A -> Vault_A)
    let mut instruction: [u8;32] = [0; 32];
    instruction[0] = 1;      
    instruction[1..17].copy_from_slice(&amount_a.to_le_bytes());
    let instruction_data = risc0_zkvm::serde::to_vec(&instruction).unwrap();

    let call_token_a = ChainedCall{
            program_id: token_program,
            instruction_data: instruction_data,
            pre_states: vec![user_a.clone(), vault_a.clone()]
        };
        
    //Chain call for Token B (User_B -> Vault_B)
    instruction[1..17].copy_from_slice(&amount_b.to_le_bytes());
    let instruction_data = risc0_zkvm::serde::to_vec(&instruction).unwrap();

    let call_token_b = ChainedCall{
            program_id: token_program,
            instruction_data: instruction_data,
            pre_states: vec![user_b.clone(), vault_b.clone()]
        };

    //Chain call for LP (Pool_LP -> User_LP)
    instruction[1..17].copy_from_slice(&amount_a.to_le_bytes());
    let instruction_data = risc0_zkvm::serde::to_vec(&instruction).unwrap();

    let call_token_lp = ChainedCall{
            program_id: token_program,
            instruction_data: instruction_data,
            pre_states: vec![pool_lp.clone(), user_lp.clone()]
        };

    chained_call.push(call_token_lp);
    chained_call.push(call_token_b);
    chained_call.push(call_token_a);

    let post_states = vec![
        pool_post.clone(), 
        pre_states[1].account.clone(),
        pre_states[2].account.clone(),
        pre_states[3].account.clone(),
        pre_states[4].account.clone(),
        pre_states[5].account.clone(),
        pre_states[6].account.clone()];

    (post_states.clone(), chained_call)
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
    let vault_a = &pre_states[1];
    let vault_b = &pre_states[2];
    let user_a = &pre_states[3];
    let user_b = &pre_states[4];

    // Verify vaults are in fact vaults
    let pool_def_data = PoolDefinition::parse(&pool.account.data).unwrap();

    if vault_a.account_id != pool_def_data.vault_a_addr {  
        panic!("Vault A was not provided");
    }
        
    if vault_b.account_id != pool_def_data.vault_b_addr {
        panic!("Vault B was not provided");
    }

    // fetch pool reserves
    //validates reserves is at least the vaults' balances
    assert!(TokenHolding::parse(&vault_a.account.data).unwrap().balance >= pool_def_data.reserve_a);
    assert!(TokenHolding::parse(&vault_b.account.data).unwrap().balance >= pool_def_data.reserve_b);
    //Cannot swap if a reserve is 0
    assert!(pool_def_data.reserve_a > 0);
    assert!(pool_def_data.reserve_b > 0);

    let (chained_call, [deposit_a, withdraw_a], [deposit_b, withdraw_b])
    = if token_id == pool_def_data.definition_token_a_id {
        let (chained_call, withdraw_b) = swap_logic(&[user_a.clone(), vault_a.clone(), vault_b.clone(), user_b.clone()],
                    amount,
                    &[pool_def_data.reserve_a, pool_def_data.reserve_b]);
                
        (chained_call, [amount, 0], [0, withdraw_b])
    } else if token_id == pool_def_data.definition_token_b_id {
        let (chained_call, withdraw_a) = swap_logic(&[user_b.clone(), vault_b.clone(), vault_a.clone(), user_a.clone()],
                        amount,
                        &[pool_def_data.reserve_b, pool_def_data.reserve_a]);

        (chained_call, [0, withdraw_a], [amount, 0])
    } else {
        panic!("AccountId is not a token type for the pool");
    };         

    // Update pool account
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
    
    let post_states = vec![
        pool_post.clone(),
        pre_states[1].account.clone(),
        pre_states[2].account.clone(),
        pre_states[3].account.clone(),
        pre_states[4].account.clone()];

    (post_states.clone(), chained_call)
}

fn swap_logic(
    pre_states: &[AccountWithMetadata],
    deposit_amount: u128,
    reserve_amounts: &[u128],
) -> (Vec<ChainedCall>, u128)
{

    let user_deposit_tx = pre_states[0].clone();
    let vault_deposit_tx = pre_states[1].clone();
    let vault_withdraw_tx = pre_states[2].clone();
    let user_withdraw_tx = pre_states[3].clone();

    let reserve_deposit_vault_amount = reserve_amounts[0];
    let reserve_withdraw_vault_amount = reserve_amounts[1];

    // Compute withdraw amount
    // Compute pool's exchange constant
    // let k = pool_def_data.reserve_a * pool_def_data.reserve_b; 
    let withdraw_amount = (reserve_withdraw_vault_amount * deposit_amount)/(reserve_deposit_vault_amount + deposit_amount);

    //Slippage check
    assert!(withdraw_amount != 0);

    let mut chained_call = Vec::new();
    
    let mut instruction_data = [0;23];
    instruction_data[0] = 1;
    instruction_data[1..17].copy_from_slice(&deposit_amount.to_le_bytes());
    let instruction_data = risc0_zkvm::serde::to_vec(&instruction_data).unwrap();
    chained_call.push(
        ChainedCall{
                program_id: vault_deposit_tx.account.program_owner,
                instruction_data: instruction_data,
                pre_states: vec![user_deposit_tx.clone(), vault_deposit_tx.clone()]
            }
    );

    let mut instruction_data = [0;23];
    instruction_data[0] = 1;
    instruction_data[1..17].copy_from_slice(&withdraw_amount.to_le_bytes());
    let instruction_data = risc0_zkvm::serde::to_vec(&instruction_data).unwrap();
    chained_call.push(
        ChainedCall{
                program_id: vault_deposit_tx.account.program_owner,
                instruction_data: instruction_data,
                pre_states: vec![vault_withdraw_tx.clone(), user_withdraw_tx.clone()]
            }
    );

    (chained_call, withdraw_amount)
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

    // Verify vaults are in fact vaults
    let pool_def_data = PoolDefinition::parse(&pool.account.data).unwrap();

    let vault_a = if vault1.account_id == pool_def_data.vault_a_addr {
        vault1.clone()
    } else if vault2.account_id == pool_def_data.vault_a_addr {
        vault2.clone()
    } else {
        panic!("Vault A was not provided");
    };
        
    let vault_b = if vault1.account_id == pool_def_data.vault_b_addr {
       vault1.clone()
    } else if vault2.account_id == pool_def_data.vault_b_addr {
        vault2.clone()
    } else {
        panic!("Vault B was not provided");
    };
    
    if max_balance_in.len() != 2 {
        panic!("Invalid number of input balances");
    }
    let max_amount_a = max_balance_in[0];
    let max_amount_b = max_balance_in[1];

    if max_amount_a == 0 || max_amount_b == 0 {
        panic!("Both max-balances must be nonzero");
    }
    
    // 2. Determine deposit amount
    let vault_b_balance = TokenHolding::parse(&vault_b.account.data).unwrap().balance;
    let vault_a_balance = TokenHolding::parse(&vault_a.account.data).unwrap().balance;
    if vault_a_balance == 0 || vault_b_balance == 0 {
        panic!("Vaults must have nonzero balances");
    }
    
    if pool_def_data.reserve_a == 0 || pool_def_data.reserve_b == 0 {
        panic!("Reserves must be nonzero");
    }

    //Calculate actual_amounts
    let actual_amount_a = if main_token == pool_def_data.definition_token_a_id {
        max_amount_a
    } else if main_token == pool_def_data.definition_token_b_id {
        (pool_def_data.reserve_a*max_amount_b)/pool_def_data.reserve_b
    } else {
        panic!("Mismatch of token types"); //main token does not match with vaults.
    };

    let actual_amount_b = if main_token == pool_def_data.definition_token_a_id {
        (pool_def_data.reserve_b*max_amount_a)/pool_def_data.reserve_a
    } else if main_token == pool_def_data.definition_token_b_id {
        max_amount_b
    } else {
        panic!("Mismatch of token types"); //main token does not match with vaults.
    };

    // 3. Validate amounts
    let user_a_balance = TokenHolding::parse(&user_a.account.data).unwrap().balance;
    let user_b_balance = TokenHolding::parse(&user_b.account.data).unwrap().balance;
    assert!(max_amount_a >= actual_amount_a && max_amount_b >= actual_amount_b);
    if user_a_balance < actual_amount_a {
        panic!("Insufficient balance");
    }

    if  user_b_balance < actual_amount_b {
        panic!("Insufficient balance");
    }
    
    if actual_amount_a == 0 || actual_amount_b == 0 {
        panic!("A trade amount is 0");
    }
    
    // 4. Calculate LP to mint
    let delta_lp = (pool_def_data.liquidity_pool_cap * actual_amount_b)/pool_def_data.reserve_b;

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

    // Chain call for Token A (User_A -> Vault_A)
    let mut instruction_data = [0; 23];
    instruction_data[0] = 1;
    instruction_data[1..17].copy_from_slice(&actual_amount_a.to_le_bytes());
    let instruction_data = risc0_zkvm::serde::to_vec(&instruction_data).unwrap();
    let call_token_a = ChainedCall{
            program_id: pool_def_data.token_program_id,
            instruction_data: instruction_data,
            pre_states: vec![user_a.clone(), vault_a]
        };

    // Chain call for Token B (User_B -> Vault_B)        
    let mut instruction_data = [0; 23];
    instruction_data[0] = 1;
    instruction_data[1..17].copy_from_slice(&actual_amount_b.to_le_bytes());
    let instruction_data = risc0_zkvm::serde::to_vec(&instruction_data).unwrap();
    let call_token_b = ChainedCall{
            program_id: pool_def_data.token_program_id,
            instruction_data: instruction_data,
            pre_states: vec![user_b.clone(), vault_b]
        };

    // Chain call for LP (User_LP -> Pool_LP)    
    let mut instruction_data = [0; 23];
    instruction_data[0] = 1;
    instruction_data[1..17].copy_from_slice(&delta_lp.to_le_bytes());
    let instruction_data = risc0_zkvm::serde::to_vec(&instruction_data).unwrap();
    let call_token_lp = ChainedCall{
            program_id: pool_def_data.token_program_id,
            instruction_data: instruction_data,
            pre_states: vec![pool_lp.clone(), user_lp.clone()]
        };


    chained_call.push(call_token_lp);
    chained_call.push(call_token_b);
    chained_call.push(call_token_a);

    let post_states = vec![
        pool_post.clone(), 
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
    
    // Verify vaults are in fact vaults
    let pool_def_data = PoolDefinition::parse(&pool.account.data).unwrap();

    let vault_a = if vault1.account_id == pool_def_data.vault_a_addr {
        vault1.clone()
    } else if vault2.account_id == pool_def_data.vault_a_addr {
        vault2.clone()
    } else {
        panic!("Vault A was not provided");
    };
        
    let vault_b = if vault1.account_id == pool_def_data.vault_b_addr {
       vault1.clone()
    } else if vault2.account_id == pool_def_data.vault_b_addr {
        vault2.clone()
    } else {
        panic!("Vault B was not provided");
    };

    // 2. Determine deposit amounts
    let user_lp_amt = TokenHolding::parse(&user_lp.account.data).unwrap().balance;
    let withdraw_amount_a = pool_def_data.reserve_a * (user_lp_amt/pool_def_data.liquidity_pool_cap);
    let withdraw_amount_b = pool_def_data.reserve_b * (user_lp_amt/pool_def_data.liquidity_pool_cap);

    //3. Validate amounts handled by token programs

    // 4. Calculate LP to reduce cap by
    if pool_def_data.liquidity_pool_cap == 0 {
        panic!("Liquidity pool must be nonzero");
    }
    let delta_lp : u128 = (pool_def_data.liquidity_pool_cap*user_lp_amt)/pool_def_data.liquidity_pool_cap;

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

    //Chaincall for Token A withdraw
    let mut instruction: [u8;32] = [0; 32];
    instruction[0] = 1;      
    instruction[1..17].copy_from_slice(&withdraw_amount_a.to_le_bytes());
    let instruction_data = risc0_zkvm::serde::to_vec(&instruction).unwrap();
    let call_token_a = ChainedCall{
            program_id: pool_def_data.token_program_id.clone(),
            instruction_data: instruction_data,
            pre_states: vec![vault_a, user_a.clone()]
        };

    //Chaincall for Token B withdraw
    let mut instruction: [u8;32] = [0; 32];
    instruction[0] = 1;      
    instruction[1..17].copy_from_slice(&withdraw_amount_b.to_le_bytes());
    let instruction_data = risc0_zkvm::serde::to_vec(&instruction).unwrap();
    let call_token_b = ChainedCall{
            program_id: pool_def_data.token_program_id.clone(),
            instruction_data: instruction_data,
            pre_states: vec![vault_b, user_b.clone()]
        };

    //Chaincall for LP adjustment        
    let mut instruction: [u8;32] = [0; 32];
    instruction[0] = 1;      
    instruction[1..17].copy_from_slice(&delta_lp.to_le_bytes());
    let instruction_data = risc0_zkvm::serde::to_vec(&instruction).unwrap();
    let call_token_lp = ChainedCall{
            program_id: pool_def_data.token_program_id.clone(),
            instruction_data: instruction_data,
            pre_states: vec![user_lp.clone(), pool_lp.clone()]
        };

    chained_call.push(call_token_lp);
    chained_call.push(call_token_b);
    chained_call.push(call_token_a);

    let post_states = vec!
        [pool_post.clone(), 
        pre_states[1].account.clone(),
        pre_states[2].account.clone(),
        pre_states[3].account.clone(),
        pre_states[4].account.clone(),
        pre_states[5].account.clone(),
        pre_states[6].account.clone()];

    (post_states, chained_call)
}

#[cfg(test)]
mod tests {
    use nssa_core::{account::{Account, AccountId, AccountWithMetadata}, program::ChainedCall};

    use crate::{PoolDefinition, TOKEN_HOLDING_TYPE, TokenHolding, add_liquidity, new_definition, remove_liquidity, swap};

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
    
    #[should_panic(expected = "Pool account is initiated or not authorized")]
    #[test]
    fn test_call_new_definition_with_initiated_pool() {
        let mut pool = Account::default();

        pool.data = vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];

        let pre_states = vec![AccountWithMetadata {
            account: pool,
            is_authorized: true,
            account_id: AccountId::new([0;32])},
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

    #[should_panic(expected = "Pool account is initiated or not authorized")]
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

    #[should_panic(expected = "Cannot set up a swap for a token with itself.")]
    #[test]
    fn test_call_new_definition_same_token() {
        let mut pool = Account::default();
        let mut vault_a = Account::default();
        let mut vault_b = Account::default();
        let mut user_a = Account::default();
        let mut user_b = Account::default();
        let mut pool_lp = Account::default();

        let definition_token_a_id = AccountId::new([1;32]);
        let definition_token_b_id = AccountId::new([2;32]); 

        user_a.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 15u128 }
        );

        user_b.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 15u128 }
        );

        
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
        let token_program_id: [u32;8] = [0; 8];
        
        let _post_states = new_definition(&pre_states, &[balance_a, balance_b], token_program_id);
    }

    #[test]
    fn test_call_new_definition_chain_call_success() {
        let mut pool = Account::default();
        let mut vault_a = Account::default();
        let mut vault_b = Account::default();
        let mut user_a = Account::default();
        let mut user_b = Account::default();
        let mut pool_lp = Account::default();
        let mut user_lp = Account::default();

        let definition_token_a_id = AccountId::new([1;32]);
        let definition_token_b_id = AccountId::new([2;32]); 

        user_a.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 15u128 }
        );

        user_b.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_b_id.clone(),
                balance: 15u128 }
        );

        
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


        let user_a = AccountWithMetadata {
            account: user_a.clone(),
            is_authorized: true,
            account_id: AccountId::new([5; 32])};
    
        let user_b = AccountWithMetadata {
            account: user_b.clone(),
            is_authorized: true,
            account_id: AccountId::new([6; 32])};

        let user_lp = AccountWithMetadata {
            account: user_lp.clone(),
            is_authorized: true,
            account_id: AccountId::new([7; 32])
        };

        let vault_a = AccountWithMetadata {
            account: vault_a.clone(),
            is_authorized: true,
            account_id: AccountId::new([2; 32])};

        let vault_b = AccountWithMetadata {
            account: vault_b.clone(),
            is_authorized: true,
            account_id: AccountId::new([3; 32])};

        let pool_lp = AccountWithMetadata {
            account: pool_lp.clone(),
            is_authorized: true,
            account_id: AccountId::new([4; 32])};

        let pre_states = vec![AccountWithMetadata {
            account: pool.clone(),
            is_authorized: true,
            account_id: AccountId::new([0; 32])},
            vault_a.clone(),
            vault_b.clone(),
            pool_lp.clone(),
            user_a.clone(),
            user_b.clone(),
            user_lp.clone(),
        ];
        let balance_a = 15u128;
        let balance_b = 15u128;
        let token_program_id: [u32;8] = [0; 8];
        let (post_states, chained_calls) = new_definition(&pre_states, &[balance_a, balance_b], token_program_id);

        let chained_call_lp = chained_calls[0].clone();
        let chained_call_b = chained_calls[1].clone();
        let chained_call_a = chained_calls[2].clone();
        
        //Expected chain_call for Token A
        let mut instruction: [u8;32] = [0; 32];
        instruction[0] = 1;      
        instruction[1..17].copy_from_slice(&balance_a.to_le_bytes());
        let instruction_data = risc0_zkvm::serde::to_vec(&instruction).unwrap();
        let expected_chained_call_a = ChainedCall{
            program_id: token_program_id,
            instruction_data,
            pre_states: vec![user_a.clone(), vault_a.clone()],
        };

        //Expected chain call for Token B
        let mut instruction: [u8;32] = [0; 32];
        instruction[0] = 1;      
        instruction[1..17].copy_from_slice(&balance_b.to_le_bytes());
        let instruction_data = risc0_zkvm::serde::to_vec(&instruction).unwrap();
        let expected_chained_call_b = ChainedCall{
            program_id: token_program_id,
            instruction_data,
            pre_states: vec![user_b.clone(), vault_b.clone()],
        };

        //Expected chain call for LP
        let mut instruction: [u8;32] = [0; 32];
        instruction[0] = 1;      
        instruction[1..17].copy_from_slice(&balance_a.to_le_bytes());
        let instruction_data = risc0_zkvm::serde::to_vec(&instruction).unwrap();
        let expected_chained_call_lp = ChainedCall{
            program_id: token_program_id,
            instruction_data,
            pre_states: vec![pool_lp.clone(), user_lp.clone()],
        };
        
        assert!(chained_call_a.program_id == expected_chained_call_a.program_id);
        assert!(chained_call_a.instruction_data == expected_chained_call_a.instruction_data);
        assert!(chained_call_a.pre_states[0].account == expected_chained_call_a.pre_states[0].account);
        assert!(chained_call_a.pre_states[1].account == expected_chained_call_a.pre_states[1].account);
        assert!(chained_call_b.program_id == expected_chained_call_b.program_id);
        assert!(chained_call_b.instruction_data == expected_chained_call_b.instruction_data);
        assert!(chained_call_b.pre_states[0].account == expected_chained_call_b.pre_states[0].account);
        assert!(chained_call_b.pre_states[1].account == expected_chained_call_b.pre_states[1].account);
        assert!(chained_call_lp.program_id == expected_chained_call_lp.program_id);
        assert!(chained_call_lp.instruction_data == expected_chained_call_lp.instruction_data);
        assert!(chained_call_lp.pre_states[0].account == expected_chained_call_lp.pre_states[0].account);
        assert!(chained_call_lp.pre_states[1].account == expected_chained_call_lp.pre_states[1].account);
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

    #[test]
    fn test_call_remove_liquidity_chain_call_success() {
        let mut pool = Account::default();
        let mut vault_a = Account::default();
        let mut vault_b = Account::default();
        let mut user_a = Account::default();
        let mut user_b = Account::default();
        let mut pool_lp = Account::default();
        let mut user_lp = Account::default();

        let definition_token_a_id = AccountId::new([1;32]);
        let definition_token_b_id = AccountId::new([2;32]); 
        
        vault_a.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 15u128 }
        );

        vault_b.data = TokenHolding::into_data(
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
        let reserve_a: u128 = 30;
        let reserve_b: u128 = 20;
        let user_lp_amt: u128 = 10;
        let token_program_id: [u32;8] = [0; 8];

        pool.data = PoolDefinition::into_data( PoolDefinition {
            definition_token_a_id: definition_token_a_id.clone(),
            definition_token_b_id: definition_token_b_id.clone(),
            vault_a_addr: vault_a_addr.clone(),
            vault_b_addr: vault_b_addr.clone(),
            liquidity_pool_id,
            liquidity_pool_cap: reserve_a,
            reserve_a,
            reserve_b,
            token_program_id,
        });
        
        user_a.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 5u128 }
        );

        user_b.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_b_id.clone(),
                balance: 5u128 }
        );

        user_lp.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id: AccountId::new([3;32]),
                balance: user_lp_amt }
        );

        let user_a = AccountWithMetadata {
            account: user_a.clone(),
            is_authorized: true,
            account_id: AccountId::new([5; 32])};
    
        let user_b = AccountWithMetadata {
            account: user_b.clone(),
            is_authorized: true,
            account_id: AccountId::new([6; 32])};

        let user_lp = AccountWithMetadata {
            account: user_lp.clone(),
            is_authorized: true,
            account_id: AccountId::new([7; 32])
        };

        let vault_a = AccountWithMetadata {
            account: vault_a.clone(),
            is_authorized: true,
            account_id: vault_a_addr.clone(),};

        let vault_b = AccountWithMetadata {
            account: vault_b.clone(),
            is_authorized: true,
            account_id: vault_b_addr.clone()};

        let pool_lp = AccountWithMetadata {
            account: pool_lp.clone(),
            is_authorized: true,
            account_id: AccountId::new([4; 32])};

        let pre_states = vec![AccountWithMetadata {
            account: pool.clone(),
            is_authorized: true,
            account_id: AccountId::new([0; 32])},
            vault_a.clone(),
            vault_b.clone(),
            pool_lp.clone(),
            user_a.clone(),
            user_b.clone(),
            user_lp.clone(),
        ];
        let (post_states, chained_calls) = remove_liquidity(&pre_states);

        let chained_call_lp = chained_calls[0].clone();
        let chained_call_b = chained_calls[1].clone();
        let chained_call_a = chained_calls[2].clone();
        
        //Expected withdraw
        let withdraw_amount_a = reserve_a * (user_lp_amt/reserve_a);
        let withdraw_amount_b = reserve_b * (user_lp_amt/reserve_a);

        //Expected chain_call for Token A
        let mut instruction: [u8;32] = [0; 32];
        instruction[0] = 1;      
        instruction[1..17].copy_from_slice(&withdraw_amount_a.to_le_bytes());
        let instruction_data = risc0_zkvm::serde::to_vec(&instruction).unwrap();
        let expected_chained_call_a = ChainedCall{
            program_id: token_program_id,
            instruction_data,
            pre_states: vec![vault_a.clone(), user_a.clone()],
        };

        //Expected chain call for Token B
        let mut instruction: [u8;32] = [0; 32];
        instruction[0] = 1;      
        instruction[1..17].copy_from_slice(&withdraw_amount_b.to_le_bytes());
        let instruction_data = risc0_zkvm::serde::to_vec(&instruction).unwrap();
        let expected_chained_call_b = ChainedCall{
            program_id: token_program_id,
            instruction_data,
            pre_states: vec![vault_b.clone(), user_b.clone()],
        };

        //Expected chain call for LP
        let mut instruction: [u8;32] = [0; 32];
        instruction[0] = 1;      
        instruction[1..17].copy_from_slice(&user_lp_amt.to_le_bytes());
        let instruction_data = risc0_zkvm::serde::to_vec(&instruction).unwrap();
        let expected_chained_call_lp = ChainedCall{
            program_id: token_program_id,
            instruction_data,
            pre_states: vec![user_lp.clone(), pool_lp.clone()],
        };
        
        assert!(chained_call_a.program_id == expected_chained_call_a.program_id);
        assert!(chained_call_a.instruction_data == expected_chained_call_a.instruction_data);
        assert!(chained_call_a.pre_states[0].account == expected_chained_call_a.pre_states[0].account);
        assert!(chained_call_a.pre_states[1].account == expected_chained_call_a.pre_states[1].account);
        assert!(chained_call_b.program_id == expected_chained_call_b.program_id);
        assert!(chained_call_b.instruction_data == expected_chained_call_b.instruction_data);
        assert!(chained_call_b.pre_states[0].account == expected_chained_call_b.pre_states[0].account);
        assert!(chained_call_b.pre_states[1].account == expected_chained_call_b.pre_states[1].account);
        assert!(chained_call_lp.program_id == expected_chained_call_lp.program_id);
        assert!(chained_call_lp.instruction_data == expected_chained_call_lp.instruction_data);
        assert!(chained_call_lp.pre_states[0].account == expected_chained_call_lp.pre_states[0].account);
        assert!(chained_call_lp.pre_states[1].account == expected_chained_call_lp.pre_states[1].account);
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
        let _balance_b = 15u128;
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
                balance: 0u128 }
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

        vault2.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_b_id.clone(),
                balance: 0u128 }
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
        let mut user_a = Account::default();
        let mut user_b = Account::default();


        let definition_token_a_id = AccountId::new([1;32]);
        let definition_token_b_id = AccountId::new([2;32]); 

        vault1.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 15u128 }
        );

        user_a.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 10u128 }
        );

        vault1.balance = 15u128;
        vault2.balance = 15u128;

        vault2.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_b_id.clone(),
                balance: 15u128 }
        );

        user_b.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_b_id.clone(),
                balance: 40u128 }
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
        let mut user_a = Account::default();
        let mut user_b = Account::default();


        let definition_token_a_id = AccountId::new([1;32]);
        let definition_token_b_id = AccountId::new([2;32]); 

        vault1.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 15u128 }
        );

        user_a.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 40u128 }
        );

        vault1.balance = 15u128;
        vault2.balance = 15u128;

        vault2.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_b_id.clone(),
                balance: 15u128 }
        );

        user_b.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_b_id.clone(),
                balance: 10u128 }
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
        let main_token = definition_token_a_id.clone();
        let _post_states = add_liquidity(&pre_states, &[balance_a,balance_b], main_token);
    }

   #[should_panic(expected = "A trade amount is 0")]
    #[test]
    fn test_call_add_liquidity_actual_trade_insufficient() {
        let mut pool = Account::default();
        let mut vault1 = Account::default();
        let mut vault2 = Account::default();
        let mut pool_lp = Account::default();
        let mut user_a = Account::default();
        let mut user_b = Account::default();
        let definition_token_a_id = AccountId::new([1;32]);
        let definition_token_b_id = AccountId::new([2;32]); 

        vault1.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 1500u128 }
        );

        user_a.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 40u128 }
        );

        vault1.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 1500u128 }
        );

        vault2.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_b_id.clone(),
                balance: 2000u128 }
        );



        user_b.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 40u128 }
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
    fn test_call_add_liquidity_reserves_zero_1() {
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
        let reserve_a: u128 = 0;
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

        let mut user_a = Account::default();
        let mut user_b = Account::default();

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

    #[test]
    fn test_call_add_liquidity_chain_call_success_1() {
        let mut pool = Account::default();
        let mut vault_a = Account::default();
        let mut vault_b = Account::default();
        let mut user_a = Account::default();
        let mut user_b = Account::default();
        let mut pool_lp = Account::default();
        let mut user_lp = Account::default();

        let definition_token_a_id = AccountId::new([1;32]);
        let definition_token_b_id = AccountId::new([2;32]);
        
        vault_a.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 15u128 }
        );

        vault_b.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_b_id.clone(),
                balance: 15u128 }
        );

        pool_lp.data = vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];

        let liquidity_pool_id = AccountId::new([7;32]);
        let liquidity_pool_cap: u128 = 30u128;
        let reserve_a: u128 = 30;
        let reserve_b: u128 = 20;
        let user_lp_amt: u128 = 10;
        let token_program_id: [u32;8] = [0; 8];
        let vault_a_addr = AccountId::new([7;32]);
        let vault_b_addr = AccountId::new([9;32]);

        pool.data = PoolDefinition::into_data( PoolDefinition {
            definition_token_a_id: definition_token_a_id.clone(),
            definition_token_b_id: definition_token_b_id.clone(),
            vault_a_addr: vault_a_addr.clone(),
            vault_b_addr: vault_b_addr.clone(),
            liquidity_pool_id,
            liquidity_pool_cap: reserve_a,
            reserve_a,
            reserve_b,
            token_program_id,
        });
        
        user_a.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 50u128 }
        );

        user_b.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_b_id.clone(),
                balance: 50u128 }
        );

        user_lp.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id: AccountId::new([3;32]),
                balance: user_lp_amt }
        );

        let user_a = AccountWithMetadata {
            account: user_a.clone(),
            is_authorized: true,
            account_id: AccountId::new([5; 32])};
    
        let user_b = AccountWithMetadata {
            account: user_b.clone(),
            is_authorized: true,
            account_id: AccountId::new([6; 32])};

        let user_lp = AccountWithMetadata {
            account: user_lp.clone(),
            is_authorized: true,
            account_id: AccountId::new([7; 32])
        };

        let vault_a = AccountWithMetadata {
            account: vault_a.clone(),
            is_authorized: true,
            account_id: vault_a_addr.clone()};

        let vault_b = AccountWithMetadata {
            account: vault_b.clone(),
            is_authorized: true,
            account_id: vault_b_addr.clone()};

        let pool_lp = AccountWithMetadata {
            account: pool_lp.clone(),
            is_authorized: true,
            account_id: AccountId::new([4; 32])};

        let pre_states = vec![AccountWithMetadata {
            account: pool.clone(),
            is_authorized: true,
            account_id: AccountId::new([0; 32])},
            vault_a.clone(),
            vault_b.clone(),
            pool_lp.clone(),
            user_a.clone(),
            user_b.clone(),
            user_lp.clone(),
        ];

        let balance_a = 10u128;
        let balance_b = 30u128;
        let main_token = definition_token_a_id.clone();
        let (post_states, chained_calls) = add_liquidity(&pre_states, &[balance_a,balance_b], main_token);

        let chained_call_lp = chained_calls[0].clone();
        let chained_call_b = chained_calls[1].clone();
        let chained_call_a = chained_calls[2].clone();
        
        //Expected amounts
        let expected_actual_amount_a = balance_a;
        //Uses: (pool_def_data.reserve_b*actual_amount_a)/pool_def_data.reserve_a
        let expected_actual_amount_b = (reserve_b*expected_actual_amount_a)/reserve_a;
        let expected_delta_lp = (reserve_a * expected_actual_amount_b)/reserve_b;

        //Expected chain_call for Token A
        let mut instruction: [u8;23] = [0; 23];
        instruction[0] = 1;      
        instruction[1..17].copy_from_slice(&expected_actual_amount_a.to_le_bytes());
        let instruction_data = risc0_zkvm::serde::to_vec(&instruction).unwrap();
        let expected_chained_call_a = ChainedCall{
            program_id: token_program_id,
            instruction_data,
            pre_states: vec![user_a.clone(), vault_a.clone()],
        };

        //Expected chain call for Token B
        let mut instruction: [u8;23] = [0; 23];
        instruction[0] = 1;      
        instruction[1..17].copy_from_slice(&expected_actual_amount_b.to_le_bytes());
        let instruction_data = risc0_zkvm::serde::to_vec(&instruction).unwrap();
        let expected_chained_call_b = ChainedCall{
            program_id: token_program_id,
            instruction_data,
            pre_states: vec![user_b.clone(), vault_b.clone()],
        };

        //Expected chain call for LP
        let mut instruction: [u8;23] = [0; 23];
        instruction[0] = 1;      
        instruction[1..17].copy_from_slice(&expected_delta_lp.to_le_bytes());
        let instruction_data = risc0_zkvm::serde::to_vec(&instruction).unwrap();
        let expected_chained_call_lp = ChainedCall{
            program_id: token_program_id,
            instruction_data,
            pre_states: vec![pool_lp.clone(), user_lp.clone()],
        };
        
        assert!(chained_call_a.program_id == expected_chained_call_a.program_id);
        assert!(chained_call_a.instruction_data == expected_chained_call_a.instruction_data);
        assert!(chained_call_a.pre_states[0].account == expected_chained_call_a.pre_states[0].account);
        assert!(chained_call_a.pre_states[1].account == expected_chained_call_a.pre_states[1].account);
        assert!(chained_call_b.program_id == expected_chained_call_b.program_id);
        assert!(chained_call_b.instruction_data == expected_chained_call_b.instruction_data);
        assert!(chained_call_b.pre_states[0].account == expected_chained_call_b.pre_states[0].account);
        assert!(chained_call_b.pre_states[1].account == expected_chained_call_b.pre_states[1].account);
        assert!(chained_call_lp.program_id == expected_chained_call_lp.program_id);
        assert!(chained_call_lp.instruction_data == expected_chained_call_lp.instruction_data);
        assert!(chained_call_lp.pre_states[0].account == expected_chained_call_lp.pre_states[0].account);
        assert!(chained_call_lp.pre_states[1].account == expected_chained_call_lp.pre_states[1].account);
    }
  
    #[test]
    fn test_call_add_liquidity_chain_call_success_2() {
        let mut pool = Account::default();
        let mut vault_a = Account::default();
        let mut vault_b = Account::default();
        let mut user_a = Account::default();
        let mut user_b = Account::default();
        let mut pool_lp = Account::default();
        let mut user_lp = Account::default();

        let definition_token_a_id = AccountId::new([1;32]);
        let definition_token_b_id = AccountId::new([2;32]); 
        
        vault_a.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 15u128 }
        );

        vault_b.data = TokenHolding::into_data(
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
        let reserve_a: u128 = 30;
        let reserve_b: u128 = 20;
        let user_lp_amt: u128 = 10;
        let token_program_id: [u32;8] = [0; 8];
        let vault_a_addr = AccountId::new([2;32]);
        let vault_b_addr = AccountId::new([3;32]);

        pool.data = PoolDefinition::into_data( PoolDefinition {
            definition_token_a_id: definition_token_a_id.clone(),
            definition_token_b_id: definition_token_b_id.clone(),
            vault_a_addr: vault_a_addr.clone(),
            vault_b_addr: vault_b_addr.clone(),
            liquidity_pool_id,
            liquidity_pool_cap: reserve_a,
            reserve_a,
            reserve_b,
            token_program_id,
        });
        
        user_a.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 50u128 }
        );

        user_b.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_b_id.clone(),
                balance: 50u128 }
        );

        user_lp.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id: AccountId::new([3;32]),
                balance: user_lp_amt }
        );

        let user_a = AccountWithMetadata {
            account: user_a.clone(),
            is_authorized: true,
            account_id: AccountId::new([5; 32])};
    
        let user_b = AccountWithMetadata {
            account: user_b.clone(),
            is_authorized: true,
            account_id: AccountId::new([6; 32])};

        let user_lp = AccountWithMetadata {
            account: user_lp.clone(),
            is_authorized: true,
            account_id: AccountId::new([7; 32])
        };

        let vault_a = AccountWithMetadata {
            account: vault_a.clone(),
            is_authorized: true,
            account_id: vault_a_addr.clone()};

        let vault_b = AccountWithMetadata {
            account: vault_b.clone(),
            is_authorized: true,
            account_id: vault_b_addr.clone()};

        let pool_lp = AccountWithMetadata {
            account: pool_lp.clone(),
            is_authorized: true,
            account_id: AccountId::new([4; 32])};

        let pre_states = vec![AccountWithMetadata {
            account: pool.clone(),
            is_authorized: true,
            account_id: AccountId::new([0; 32])},
            vault_a.clone(),
            vault_b.clone(),
            pool_lp.clone(),
            user_a.clone(),
            user_b.clone(),
            user_lp.clone(),
        ];

        let balance_a = 40u128;
        let balance_b = 20u128;
        let main_token = definition_token_b_id.clone();
        let (post_states, chained_calls) = add_liquidity(&pre_states, &[balance_a,balance_b], main_token);

        let chained_call_lp = chained_calls[0].clone();
        let chained_call_b = chained_calls[1].clone();
        let chained_call_a = chained_calls[2].clone();
        
        //Expected amounts
        let expected_actual_amount_b = balance_b;
        //Uses: (pool_def_data.reserve_b*actual_amount_a)/pool_def_data.reserve_a
        let expected_actual_amount_a = (reserve_a*expected_actual_amount_b)/reserve_b;
        let expected_delta_lp = (reserve_a * expected_actual_amount_b)/reserve_b;

        //Expected chain_call for Token A
        let mut instruction: [u8;23] = [0; 23];
        instruction[0] = 1;      
        instruction[1..17].copy_from_slice(&expected_actual_amount_a.to_le_bytes());
        let instruction_data = risc0_zkvm::serde::to_vec(&instruction).unwrap();
        let expected_chained_call_a = ChainedCall{
            program_id: token_program_id,
            instruction_data,
            pre_states: vec![user_a.clone(), vault_a.clone()],
        };

        //Expected chain call for Token B
        let mut instruction: [u8;23] = [0; 23];
        instruction[0] = 1;      
        instruction[1..17].copy_from_slice(&expected_actual_amount_b.to_le_bytes());
        let instruction_data = risc0_zkvm::serde::to_vec(&instruction).unwrap();
        let expected_chained_call_b = ChainedCall{
            program_id: token_program_id,
            instruction_data,
            pre_states: vec![user_b.clone(), vault_b.clone()],
        };

        //Expected chain call for LP
        let mut instruction: [u8;23] = [0; 23];
        instruction[0] = 1;      
        instruction[1..17].copy_from_slice(&expected_delta_lp.to_le_bytes());
        let instruction_data = risc0_zkvm::serde::to_vec(&instruction).unwrap();
        let expected_chained_call_lp = ChainedCall{
            program_id: token_program_id,
            instruction_data,
            pre_states: vec![pool_lp.clone(), user_lp.clone()],
        };
        
        assert!(chained_call_a.program_id == expected_chained_call_a.program_id);
        assert!(chained_call_a.instruction_data == expected_chained_call_a.instruction_data);
        assert!(chained_call_a.pre_states[0].account == expected_chained_call_a.pre_states[0].account);
        assert!(chained_call_a.pre_states[1].account == expected_chained_call_a.pre_states[1].account);
        assert!(chained_call_b.program_id == expected_chained_call_b.program_id);
        assert!(chained_call_b.instruction_data == expected_chained_call_b.instruction_data);
        assert!(chained_call_b.pre_states[0].account == expected_chained_call_b.pre_states[0].account);
        assert!(chained_call_b.pre_states[1].account == expected_chained_call_b.pre_states[1].account);
        assert!(chained_call_lp.program_id == expected_chained_call_lp.program_id);
        assert!(chained_call_lp.instruction_data == expected_chained_call_lp.instruction_data);
        assert!(chained_call_lp.pre_states[0].account == expected_chained_call_lp.pre_states[0].account);
        assert!(chained_call_lp.pre_states[1].account == expected_chained_call_lp.pre_states[1].account);
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

    #[should_panic(expected = "AccountId is not a token type for the pool")]
    #[test]
    fn test_call_swap_incorrect_token_type() {
        let mut pool = Account::default();
        let mut vault_a = Account::default();
        let mut vault_b = Account::default();
        let mut pool_lp = Account::default();
        let mut user_a = Account::default();
        let mut user_b = Account::default();

        let definition_token_a_id = AccountId::new([1;32]);
        let definition_token_b_id = AccountId::new([2;32]); 

        vault_a.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 15u128 }
        );

        vault_b.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_b_id.clone(),
                balance: 20u128 }
        );

        pool_lp.data = vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];
        user_a.data = vec![
                0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];
        user_b.data = vec![
                1, 1, 1, 1, 1, 1, 1, 12, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];

        let definition_token_a_id = AccountId::new([1;32]);
        let definition_token_b_id = AccountId::new([2;32]);
        let vault_a_addr = AccountId::new([5;32]);
        let vault_b_addr = AccountId::new([6;32]);
        let liquidity_pool_id = AccountId::new([7;32]);
        let liquidity_pool_cap: u128 = 30u128;
        let reserve_a: u128 = 15;
        let reserve_b: u128 = 20;
        let token_program_id: [u32;8] = [5; 8];

        pool.data = PoolDefinition::into_data(
            PoolDefinition {
                definition_token_a_id: definition_token_a_id.clone(),
                definition_token_b_id: definition_token_b_id.clone(),
                vault_a_addr: vault_a_addr.clone(),
                vault_b_addr: vault_b_addr.clone(),
                liquidity_pool_id,
                liquidity_pool_cap,
                reserve_a,
                reserve_b,
                token_program_id,
            }
         );

        let pre_states = vec![AccountWithMetadata {
            account: pool.clone(),
            is_authorized: true,
            account_id: AccountId::new([0; 32])},
            AccountWithMetadata {
            account: vault_a.clone(),
            is_authorized: true,
            account_id: vault_a_addr.clone()},
            AccountWithMetadata {
            account: vault_b.clone(),
            is_authorized: true,
            account_id: vault_b_addr.clone()},
            AccountWithMetadata {
            account: user_a.clone(),
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
            AccountWithMetadata {
            account: user_b.clone(),
            is_authorized: true,
            account_id: AccountId::new([5; 32])}
        ];
        let amount = 15u128;
        let token_addr = AccountId::new([42;32]);
        let (post_accounts, chain_calls) = swap(&pre_states, amount, token_addr);
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

    #[test]
    fn test_call_swap_successful_chain_call_1() {
        let mut pool = Account::default();
        let mut vault_a = Account::default();
        let mut vault_b = Account::default();
        let mut pool_lp = Account::default();
        let mut user_a = Account::default();
        let mut user_b = Account::default();

        let definition_token_a_id = AccountId::new([1;32]);
        let definition_token_b_id = AccountId::new([2;32]); 

        vault_a.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 15u128 }
        );

        vault_b.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_b_id.clone(),
                balance: 20u128 }
        );

        pool_lp.data = vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];
        user_a.data = vec![
                0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];
        user_b.data = vec![
                1, 1, 1, 1, 1, 1, 1, 12, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];

        let definition_token_a_id = AccountId::new([1;32]);
        let definition_token_b_id = AccountId::new([2;32]);
        let vault_a_addr = AccountId::new([5;32]);
        let vault_b_addr = AccountId::new([6;32]);
        let liquidity_pool_id = AccountId::new([7;32]);
        let liquidity_pool_cap: u128 = 30u128;
        let reserve_a: u128 = 15;
        let reserve_b: u128 = 20;
        let token_program_id: [u32;8] = [5; 8];

        user_a.program_owner = token_program_id;
        user_b.program_owner = token_program_id;
        vault_a.program_owner = token_program_id;
        vault_b.program_owner = token_program_id;

        pool.data = PoolDefinition::into_data(
            PoolDefinition {
                definition_token_a_id: definition_token_a_id.clone(),
                definition_token_b_id: definition_token_b_id.clone(),
                vault_a_addr: vault_a_addr.clone(),
                vault_b_addr: vault_b_addr.clone(),
                liquidity_pool_id,
                liquidity_pool_cap,
                reserve_a,
                reserve_b,
                token_program_id,
            }
         );

        let pre_states = vec![AccountWithMetadata {
            account: pool.clone(),
            is_authorized: true,
            account_id: AccountId::new([0; 32])},
            AccountWithMetadata {
            account: vault_a.clone(),
            is_authorized: true,
            account_id: vault_a_addr.clone()},
            AccountWithMetadata {
            account: vault_b.clone(),
            is_authorized: true,
            account_id: vault_b_addr.clone()},
            AccountWithMetadata {
            account: user_a.clone(),
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
            AccountWithMetadata {
            account: user_b.clone(),
            is_authorized: true,
            account_id: AccountId::new([5; 32])}
        ];
        let amount = 15u128;
        
        let token_addr = definition_token_a_id;
        let (post_accounts, chain_calls) = swap(&pre_states, amount, token_addr);

        let pool_post = post_accounts[0].clone();
        let pool_pre_data = PoolDefinition::parse(&pool.data).unwrap();
        let pool_post_data = PoolDefinition::parse(&pool_post.data).unwrap();
        assert!(pool_post_data.reserve_a == pool_pre_data.reserve_a + amount);
        
        let expected_withdraw = (pool_pre_data.reserve_b * amount)/(pool_pre_data.reserve_a + amount);
        assert!(pool_post_data.reserve_b  == pool_pre_data.reserve_b - expected_withdraw);
                
        let chain_call_a = chain_calls[0].clone();
        let chain_call_b = chain_calls[1].clone();
            
        assert!(chain_call_b.program_id == token_program_id);
        assert!(chain_call_a.program_id == token_program_id);
        
        let mut instruction_data = [0; 23];
        instruction_data[0] = 1;
        instruction_data[1..17].copy_from_slice(&amount.to_le_bytes());
        let expected_instruction_data_0 = risc0_zkvm::serde::to_vec(&instruction_data).unwrap();
        let mut instruction_data = [0; 23];
        instruction_data[0] = 1;
        instruction_data[1..17].copy_from_slice(&expected_withdraw.to_le_bytes());
        let expected_instruction_data_1 = risc0_zkvm::serde::to_vec(&instruction_data).unwrap();

        let chain_call_a_account0 = chain_call_a.pre_states[0].account.clone();
        let chain_call_a_account1 = chain_call_a.pre_states[1].account.clone();

        let chain_call_b_account0 = chain_call_b.pre_states[0].account.clone();
        let chain_call_b_account1 = chain_call_b.pre_states[1].account.clone();

        assert!(chain_call_a.instruction_data == expected_instruction_data_0);
        assert!(chain_call_a_account0 == user_a);
        assert!(chain_call_a_account1 == vault_a);
        assert!(chain_call_b.instruction_data == expected_instruction_data_1);
        assert!(chain_call_b_account0 == vault_b);
        assert!(chain_call_b_account1 == user_b);
    }

    #[test]
    fn test_call_swap_successful_chain_call_2() {
        let mut pool = Account::default();
        let mut vault_a = Account::default();
        let mut vault_b = Account::default();
        let mut pool_lp = Account::default();
        let mut user_a = Account::default();
        let mut user_b = Account::default();

        let definition_token_a_id = AccountId::new([1;32]);
        let definition_token_b_id = AccountId::new([2;32]); 

        vault_a.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 15u128 }
        );

        vault_b.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_b_id.clone(),
                balance: 20u128 }
        );

        pool_lp.data = vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];
        user_a.data = vec![
                0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];
        user_b.data = vec![
                1, 1, 1, 1, 1, 1, 1, 12, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];

        let definition_token_a_id = AccountId::new([1;32]);
        let definition_token_b_id = AccountId::new([2;32]);
        let vault_a_addr = AccountId::new([5;32]);
        let vault_b_addr = AccountId::new([6;32]);
        let liquidity_pool_id = AccountId::new([7;32]);
        let liquidity_pool_cap: u128 = 30u128;
        let reserve_a: u128 = 15;
        let reserve_b: u128 = 20;
        let token_program_id: [u32;8] = [5; 8];

        user_a.program_owner = token_program_id;
        user_b.program_owner = token_program_id;
        vault_a.program_owner = token_program_id;
        vault_b.program_owner = token_program_id;

        pool.data = PoolDefinition::into_data(
            PoolDefinition {
                definition_token_a_id: definition_token_a_id.clone(),
                definition_token_b_id: definition_token_b_id.clone(),
                vault_a_addr: vault_a_addr.clone(),
                vault_b_addr: vault_b_addr.clone(),
                liquidity_pool_id,
                liquidity_pool_cap,
                reserve_a,
                reserve_b,
                token_program_id,
            }
         );

        let pre_states = vec![AccountWithMetadata {
            account: pool.clone(),
            is_authorized: true,
            account_id: AccountId::new([0; 32])},
            AccountWithMetadata {
            account: vault_a.clone(),
            is_authorized: true,
            account_id: vault_a_addr.clone()},
            AccountWithMetadata {
            account: vault_b.clone(),
            is_authorized: true,
            account_id: vault_b_addr.clone()},
            AccountWithMetadata {
            account: user_a.clone(),
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
            AccountWithMetadata {
            account: user_b.clone(),
            is_authorized: true,
            account_id: AccountId::new([5; 32])}
        ];
        let amount = 15u128;
        let token_addr = definition_token_b_id;
        let (post_accounts, chain_calls) = swap(&pre_states, amount, token_addr);

        let pool_post = post_accounts[0].clone();
        let pool_pre_data = PoolDefinition::parse(&pool.data).unwrap();
        let pool_post_data = PoolDefinition::parse(&pool_post.data).unwrap();
        assert!(pool_post_data.reserve_b == pool_pre_data.reserve_b + amount);

        let expected_withdraw = (pool_pre_data.reserve_a * amount)/(pool_pre_data.reserve_b + amount);
        assert!(pool_post_data.reserve_a  == pool_pre_data.reserve_a - expected_withdraw);

        let chain_call_b = chain_calls[0].clone();
        let chain_call_a = chain_calls[1].clone();

        assert!(chain_call_b.program_id == token_program_id);
        assert!(chain_call_a.program_id == token_program_id);

        let mut instruction_data = [0; 23];
        instruction_data[0] = 1;
        instruction_data[1..17].copy_from_slice(&expected_withdraw.to_le_bytes());
        let expected_instruction_data_0 = risc0_zkvm::serde::to_vec(&instruction_data).unwrap();
        let mut instruction_data = [0; 23];
        instruction_data[0] = 1;
        instruction_data[1..17].copy_from_slice(&amount.to_le_bytes());
        let expected_instruction_data_1 = risc0_zkvm::serde::to_vec(&instruction_data).unwrap();

        let chain_call_a_account0 = chain_call_a.pre_states[0].account.clone();
        let chain_call_a_account1 = chain_call_a.pre_states[1].account.clone();

        let chain_call_b_account0 = chain_call_b.pre_states[0].account.clone();
        let chain_call_b_account1 = chain_call_b.pre_states[1].account.clone();

        assert!(chain_call_a.instruction_data == expected_instruction_data_0);
        assert!(chain_call_a_account0 == vault_a);
        assert!(chain_call_a_account1 == user_a);
        assert!(chain_call_b.instruction_data == expected_instruction_data_1);
        assert!(chain_call_b_account0 == user_b);
        assert!(chain_call_b_account1 == vault_b);

    }
}