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

const POOL_DEFINITION_DATA_SIZE: usize = 209;
const MAX_NUMBER_POOLS: usize = 32;
const AMM_DEFINITION_DATA_SIZE: usize = 1024;

struct AMMDefinition {
    pool_ids: Vec<AccountId>,
}

impl AMMDefinition {
    fn into_data(self) -> Vec<u8> {
        let size_of_pool: usize = self.pool_ids.len();

        let mut bytes = [0; AMM_DEFINITION_DATA_SIZE];
        for i in 0..size_of_pool-1 {
            bytes[32*i..32*(i+1)].copy_from_slice(&self.pool_ids[i].to_bytes())
        }

        for i in size_of_pool..MAX_NUMBER_POOLS {
            bytes[32*i..32*(i+1)].copy_from_slice(&AccountId::default().to_bytes())
        }

        bytes.into()
    }

    fn parse(data: &[u8]) -> Option<Self> {
        if data.len() % 32 != 0 {
            panic!("AMM data should be divisible by 32 (number of bytes per of AccountId");
        }

        let size_of_pool = data.len()/32;

        let mut pool_ids = Vec::<AccountId>::new();

        for i in 0..size_of_pool {
            pool_ids.push(
                AccountId::new(data[i*32..(i+1)*32].try_into().expect("Parse data: The AMM program must be provided a valid AccountIds"))
            );
        }

        for _ in size_of_pool..MAX_NUMBER_POOLS {
            pool_ids.push( AccountId::default() );
        }

        Some( Self{
            pool_ids
        })
    }
}

struct PoolDefinition{
    definition_token_a_id: AccountId,
    definition_token_b_id: AccountId,
    vault_a_addr: AccountId,
    vault_b_addr: AccountId,
    liquidity_pool_id: AccountId,
    liquidity_pool_supply: u128,
    reserve_a: u128,
    reserve_b: u128,
    active: bool
}

impl PoolDefinition {
    fn into_data(self) -> Vec<u8> {
        let mut bytes = [0; POOL_DEFINITION_DATA_SIZE];
        bytes[0..32].copy_from_slice(&self.definition_token_a_id.to_bytes());
        bytes[32..64].copy_from_slice(&self.definition_token_b_id.to_bytes());
        bytes[64..96].copy_from_slice(&self.vault_a_addr.to_bytes());
        bytes[96..128].copy_from_slice(&self.vault_b_addr.to_bytes());
        bytes[128..160].copy_from_slice(&self.liquidity_pool_id.to_bytes());
        bytes[160..176].copy_from_slice(&self.liquidity_pool_supply.to_le_bytes());
        bytes[176..192].copy_from_slice(&self.reserve_a.to_le_bytes());
        bytes[192..208].copy_from_slice(&self.reserve_b.to_le_bytes());
        bytes[208] = self.active as u8;
        bytes.into()
    }

    fn parse(data: &[u8]) -> Option<Self> {
        if data.len() != POOL_DEFINITION_DATA_SIZE {
            None
        } else {
            let definition_token_a_id = AccountId::new(data[0..32].try_into().expect("Parse data: The AMM program must be provided a valid AccountId for Token A definition"));
            let definition_token_b_id = AccountId::new(data[32..64].try_into().expect("Parse data: The AMM program must be provided a valid AccountId for Vault B definition"));
            let vault_a_addr = AccountId::new(data[64..96].try_into().expect("Parse data: The AMM program must be provided a valid AccountId for Vault A"));
            let vault_b_addr = AccountId::new(data[96..128].try_into().expect("Parse data: The AMM program must be provided a valid AccountId for Vault B"));
            let liquidity_pool_id = AccountId::new(data[128..160].try_into().expect("Parse data: The AMM program must be provided a valid AccountId for Token liquidity pool definition"));
            let liquidity_pool_supply = u128::from_le_bytes(data[160..176].try_into().expect("Parse data: The AMM program must be provided a valid u128 for liquidity cap"));
            let reserve_a = u128::from_le_bytes(data[176..192].try_into().expect("Parse data: The AMM program must be provided a valid u128 for reserve A balance"));
            let reserve_b = u128::from_le_bytes(data[192..208].try_into().expect("Parse data: The AMM program must be provided a valid u128 for reserve B balance"));
            
            let active = match data[208] {
                0 => false,
                1 => true,
                _ => panic!("Parse data: The AMM program must be provided a valid bool for active"),
            };
            
            Some(Self {
                definition_token_a_id,
                definition_token_b_id,
                vault_a_addr,
                vault_b_addr,
                liquidity_pool_id,
                liquidity_pool_supply,
                reserve_a,
                reserve_b,
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
    fn into_data(self) -> Vec<u8> {
        let mut bytes = [0; TOKEN_DEFINITION_DATA_SIZE];
        bytes[0] = self.account_type;
        bytes[1..7].copy_from_slice(&self.name);
        bytes[7..].copy_from_slice(&self.total_supply.to_le_bytes());
        bytes.into()
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
            let balance_a: u128 = u128::from_le_bytes(instruction[1..17].try_into().expect("New definition: AMM Program expects u128 for balance a"));
            let balance_b: u128 = u128::from_le_bytes(instruction[17..33].try_into().expect("New definition: AMM Program expects u128 for balance b"));
            
            // Convert Vec<u8> to ProgramId ([u32;8])
            let mut token_program_id: [u32;8] = [0;8];
            token_program_id[0] = u32::from_le_bytes(instruction[33..37].try_into().expect("New definition: AMM Program expects valid u32"));
            token_program_id[1] = u32::from_le_bytes(instruction[37..41].try_into().expect("New definition: AMM Program expects valid u32"));
            token_program_id[2] = u32::from_le_bytes(instruction[41..45].try_into().expect("New definition: AMM Program expects valid u32"));
            token_program_id[3] = u32::from_le_bytes(instruction[45..49].try_into().expect("New definition: AMM Program expects valid u32"));
            token_program_id[4] = u32::from_le_bytes(instruction[49..53].try_into().expect("New definition: AMM Program expects valid u32"));
            token_program_id[5] = u32::from_le_bytes(instruction[53..57].try_into().expect("New definition: AMM Program expects valid u32"));
            token_program_id[6] = u32::from_le_bytes(instruction[57..61].try_into().expect("New definition: AMM Program expects valid u32"));
            token_program_id[7] = u32::from_le_bytes(instruction[61..65].try_into().expect("New definition: AMM Program expects valid u32"));

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
            
            let amount = u128::from_le_bytes(instruction[1..17].try_into().expect("Swap: AMM Program expects valid u128 for balance to move"));

            let (post_states, chained_call) = swap(&pre_states, amount, token_addr);

            write_nssa_outputs_with_chained_call(pre_states, post_states, chained_call);
        }
        2 => {

            let balance_a = u128::from_le_bytes(instruction[1..17].try_into().expect("Add liquidity: AMM Program expects valid u128 for balance a"));
            let balance_b = u128::from_le_bytes(instruction[17..33].try_into().expect("Add liquidity: AMM Program expects valid u128 for balance b"));

            let mut token_addr: [u8;32] = [0;32];
            token_addr[0..].copy_from_slice(&instruction[33..65]);
            let token_addr = AccountId::new(token_addr);

            let (post_states, chained_call) = add_liquidity(&pre_states,
                        &[balance_a, balance_b], token_addr.clone());
           write_nssa_outputs_with_chained_call(pre_states, post_states, chained_call);
        }
        3 => {

            let balance_lp = u128::from_le_bytes(instruction[1..17].try_into().expect("Remove liquidity: AMM Program expects valid u128 for balance liquidity"));
            let balance_a = u128::from_le_bytes(instruction[17..33].try_into().expect("Remove liquidity: AMM Program expects valid u128 for balance a"));
            let balance_b = u128::from_le_bytes(instruction[33..49].try_into().expect("Remove liquidity: AMM Program expects valid u128 for balance b"));

            let (post_states, chained_call) = remove_liquidity(&pre_states, &[balance_lp, balance_a, balance_b]);

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
    let user_holding_a = &pre_states[4];
    let user_holding_b = &pre_states[5];
    let user_holding_lp = &pre_states[6];

    if pool.account != Account::default() || !pool.is_authorized {
        panic!("Pool account is initiated or not authorized");
    }

    // TODO: temporary band-aid to prevent vault's from being
    // owned by the amm program.
    if vault_a.account == Account::default() || vault_b.account == Account::default() {
        panic!("Vault accounts uninitialized")
    }

    let amount_a = balance_in[0];
    let amount_b = balance_in[1];

    // Prevents pool constant coefficient (k) from being 0.
    if amount_a == 0 || amount_b == 0 {
        panic!("Balances must be nonzero")
    }

    // Verify token_a and token_b are different
    let definition_token_a_id = TokenHolding::parse(&user_holding_a.account.data).expect("New definition: AMM Program expects valid Token Holding account for Token A").definition_id;
    let definition_token_b_id = TokenHolding::parse(&user_holding_b.account.data).expect("New definition: AMM Program expects valid Token Holding account for Token B").definition_id;
   
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
            liquidity_pool_id: TokenHolding::parse(&pool_lp.account.data).expect("New definition: AMM Program expects valid Token Holding account for liquidity pool").definition_id,
            liquidity_pool_supply: amount_a,
            reserve_a: amount_a,
            reserve_b: amount_b,
            active: true, 
    };

    pool_post.data = pool_post_definition.into_data();

    let mut chained_call = Vec::new();
   
    //Chain call for Token A (user_holding_a -> Vault_A)
    let mut instruction: [u8;32] = [0; 32];
    instruction[0] = 1;      
    instruction[1..17].copy_from_slice(&amount_a.to_le_bytes());
    let instruction_data = risc0_zkvm::serde::to_vec(&instruction).expect("New definition: AMM Program expects valid instruction_data");

    let call_token_a = ChainedCall{
            program_id: token_program,
            instruction_data: instruction_data,
            pre_states: vec![user_holding_a.clone(), vault_a.clone()]
        };
        
    //Chain call for Token B (user_holding_b -> Vault_B)
    instruction[1..17].copy_from_slice(&amount_b.to_le_bytes());
    let instruction_data = risc0_zkvm::serde::to_vec(&instruction).expect("New definition: AMM Program expects valid instruction_data");

    let call_token_b = ChainedCall{
            program_id: token_program,
            instruction_data: instruction_data,
            pre_states: vec![user_holding_b.clone(), vault_b.clone()]
        };

    //Chain call for LP (Pool_LP -> user_holding_lp)
    instruction[1..17].copy_from_slice(&amount_a.to_le_bytes());
    let instruction_data = risc0_zkvm::serde::to_vec(&instruction).expect("New definition: AMM Program expects valid instruction_data");

    let call_token_lp = ChainedCall{
            program_id: token_program,
            instruction_data: instruction_data,
            pre_states: vec![pool_lp.clone(), user_holding_lp.clone()]
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
    let user_holding_a = &pre_states[3];
    let user_holding_b = &pre_states[4];

    // Verify vaults are in fact vaults
    let pool_def_data = PoolDefinition::parse(&pool.account.data).expect("Swap: AMM Program expects a valid Pool Definition Account");

    if !pool_def_data.active {
        panic!("Pool is inactive");
    }

    if vault_a.account_id != pool_def_data.vault_a_addr {  
        panic!("Vault A was not provided");
    }
        
    if vault_b.account_id != pool_def_data.vault_b_addr {
        panic!("Vault B was not provided");
    }

    // fetch pool reserves
    //validates reserves is at least the vaults' balances
    assert!(TokenHolding::parse(&vault_a.account.data).expect("Swap: AMM Program expects a valid Token Holding Account for Vault A").balance >= pool_def_data.reserve_a);
    assert!(TokenHolding::parse(&vault_b.account.data).expect("Swap: AMM Program expects a valid Token Holding Account for Vault B").balance >= pool_def_data.reserve_b);
    //Cannot swap if a reserve is 0
    assert!(pool_def_data.reserve_a > 0);
    assert!(pool_def_data.reserve_b > 0);

    let (chained_call, [deposit_a, withdraw_a], [deposit_b, withdraw_b])
    = if token_id == pool_def_data.definition_token_a_id {
        let (chained_call, withdraw_b) = swap_logic(&[user_holding_a.clone(), vault_a.clone(), vault_b.clone(), user_holding_b.clone()],
                    amount,
                    &[pool_def_data.reserve_a, pool_def_data.reserve_b]);
                
        (chained_call, [amount, 0], [0, withdraw_b])
    } else if token_id == pool_def_data.definition_token_b_id {
        let (chained_call, withdraw_a) = swap_logic(&[user_holding_b.clone(), vault_b.clone(), vault_a.clone(), user_holding_a.clone()],
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
            liquidity_pool_supply: pool_def_data.liquidity_pool_supply.clone(),
            reserve_a: pool_def_data.reserve_a + deposit_a - withdraw_a,
            reserve_b: pool_def_data.reserve_b + deposit_b - withdraw_b,
            active: true, 
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
    let instruction_data = risc0_zkvm::serde::to_vec(&instruction_data).expect("Swap Logic: AMM Program expects valid transaction instruction data");
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
    let instruction_data = risc0_zkvm::serde::to_vec(&instruction_data).expect("Swap Logic: AMM Program expects valid transaction instruction data");
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

    //TODO: add logic for re-initialized

    let pool = &pre_states[0];
    let vault_a = &pre_states[1];
    let vault_b = &pre_states[2];
    let pool_lp = &pre_states[3];
    let user_holding_a = &pre_states[4];
    let user_holding_b = &pre_states[5];
    let user_holding_lp = &pre_states[6];

    // Verify vaults are in fact vaults
    let pool_def_data = PoolDefinition::parse(&pool.account.data).expect("Add liquidity: AMM Program expects valid Pool Definition Account");
    if vault_a.account_id != pool_def_data.vault_a_addr {
        panic!("Vault A was not provided");
    }

    if vault_b.account_id != pool_def_data.vault_b_addr {
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
    
    // 2. Determine deposit amount
    let vault_b_balance = TokenHolding::parse(&vault_b.account.data).expect("Add liquidity: AMM Program expects valid Token Holding Account for Vault B").balance;
    let vault_a_balance = TokenHolding::parse(&vault_a.account.data).expect("Add liquidity: AMM Program expects valid Token Holding Account for Vault A").balance;
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
    let user_holding_a_balance = TokenHolding::parse(&user_holding_a.account.data).expect("Add liquidity: AMM Program expects a valid Token Holding Account for User A").balance;
    let user_holding_b_balance = TokenHolding::parse(&user_holding_b.account.data).expect("Add liquidity: AMM Program expects a valid Token Holding Account for User B").balance;
    assert!(max_amount_a >= actual_amount_a && max_amount_b >= actual_amount_b);
    if user_holding_a_balance < actual_amount_a {
        panic!("Insufficient balance");
    }

    if  user_holding_b_balance < actual_amount_b {
        panic!("Insufficient balance");
    }
    
    if actual_amount_a == 0 || actual_amount_b == 0 {
        panic!("A trade amount is 0");
    }
    
    // 4. Calculate LP to mint
    let delta_lp = (pool_def_data.liquidity_pool_supply * actual_amount_b)/pool_def_data.reserve_b;

    // 5. Update pool account
    let mut pool_post = pool.account.clone();
    let pool_post_definition = PoolDefinition {
            definition_token_a_id: pool_def_data.definition_token_a_id.clone(),
            definition_token_b_id: pool_def_data.definition_token_b_id.clone(),
            vault_a_addr: pool_def_data.vault_a_addr.clone(),
            vault_b_addr: pool_def_data.vault_b_addr.clone(),
            liquidity_pool_id: pool_def_data.liquidity_pool_id.clone(),
            liquidity_pool_supply: pool_def_data.liquidity_pool_supply + delta_lp,
            reserve_a: pool_def_data.reserve_a + actual_amount_a,
            reserve_b: pool_def_data.reserve_b + actual_amount_b,
            active: true,  
    };
    
    pool_post.data = pool_post_definition.into_data();
    let mut chained_call = Vec::new();

    // Chain call for Token A (user_holding_a -> Vault_A)
    let mut instruction_data = [0; 23];
    instruction_data[0] = 1;
    instruction_data[1..17].copy_from_slice(&actual_amount_a.to_le_bytes());
    let instruction_data = risc0_zkvm::serde::to_vec(&instruction_data).expect("Add liquidity: AMM Program expects valid token transfer instruction data");
    let call_token_a = ChainedCall{
            program_id: vault_a.account.program_owner,
            instruction_data: instruction_data,
            pre_states: vec![user_holding_a.clone(), vault_a.clone()]
        };

    // Chain call for Token B (user_holding_b -> Vault_B)        
    let mut instruction_data = [0; 23];
    instruction_data[0] = 1;
    instruction_data[1..17].copy_from_slice(&actual_amount_b.to_le_bytes());
    let instruction_data = risc0_zkvm::serde::to_vec(&instruction_data).expect("Add liquidity: AMM Program expects valid token transfer instruction data");
    let call_token_b = ChainedCall{
            program_id: vault_b.account.program_owner,
            instruction_data: instruction_data,
            pre_states: vec![user_holding_b.clone(), vault_b.clone()]
        };

    // Chain call for LP (user_holding_lp -> Pool_LP)    
    let mut instruction_data = [0; 23];
    instruction_data[0] = 1;
    instruction_data[1..17].copy_from_slice(&delta_lp.to_le_bytes());
    let instruction_data = risc0_zkvm::serde::to_vec(&instruction_data).expect("Add liquidity: AMM Program expects valid token transfer instruction data");
    let call_token_lp = ChainedCall{
            program_id: pool_lp.account.program_owner,
            instruction_data: instruction_data,
            pre_states: vec![pool_lp.clone(), user_holding_lp.clone()]
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

fn remove_liquidity(pre_states: &[AccountWithMetadata],
    amounts: &[u128]   
) -> (Vec<Account>, Vec<ChainedCall>)
{
    if pre_states.len() != 7 {
       panic!("Invalid number of input accounts");
    }

    let pool = &pre_states[0];
    let vault_a = &pre_states[1];
    let vault_b = &pre_states[2];
    let pool_lp = &pre_states[3];
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

    if vault_a.account_id != pool_def_data.vault_a_addr {
        panic!("Vault A was not provided");
    }

    if vault_b.account_id != pool_def_data.vault_b_addr {
        panic!("Vault B was not provided");
    }
    
    // 2. Compute withdrawal amounts
    let user_holding_lp_data = TokenHolding::parse(&user_holding_lp.account.data).expect("Remove liquidity: AMM Program expects a valid Token Account for liquidity token");
 
    if user_holding_lp_data.balance > pool_def_data.liquidity_pool_supply || user_holding_lp_data.definition_id != pool_def_data.liquidity_pool_id {
        panic!("Invalid liquidity account provided");
    }

    if user_holding_lp_data.balance < amount_lp {
        panic!("Invalid liquidity amount provided");
    }

    let withdraw_amount_a = pool_def_data.reserve_a * (amount_lp/pool_def_data.liquidity_pool_supply);
    let withdraw_amount_b = pool_def_data.reserve_b * (amount_lp/pool_def_data.liquidity_pool_supply);

    // 3. Validate and slippage check
    if withdraw_amount_a < amount_min_a {
        panic!("Insufficient minimal withdraw amount (Token A) provided for liquidity amount");
    }
    if withdraw_amount_b < amount_min_b {
        panic!("Insufficient minimal withdraw amount (Token B) provided for liquidity amount");
    }

    // 4. Calculate LP to reduce cap by
    let delta_lp : u128 = (pool_def_data.liquidity_pool_supply*amount_lp)/pool_def_data.liquidity_pool_supply;

    // 5. Update pool account
    let mut pool_post = pool.account.clone();
    let pool_post_definition = PoolDefinition {
            definition_token_a_id: pool_def_data.definition_token_a_id.clone(),
            definition_token_b_id: pool_def_data.definition_token_b_id.clone(),
            vault_a_addr: pool_def_data.vault_a_addr.clone(),
            vault_b_addr: pool_def_data.vault_b_addr.clone(),
            liquidity_pool_id: pool_def_data.liquidity_pool_id.clone(),
            liquidity_pool_supply: pool_def_data.liquidity_pool_supply - delta_lp,
            reserve_a: pool_def_data.reserve_a - withdraw_amount_a,
            reserve_b: pool_def_data.reserve_b - withdraw_amount_b,
            active: true,  
    };

    pool_post.data = pool_post_definition.into_data();

    let mut chained_call = Vec::new();

    let mut instruction_data = [0; 23];
    instruction_data[0] = 1;

    //Chaincall for Token A withdraw
    let mut instruction: [u8;32] = [0; 32];
    instruction[0] = 1;      
    instruction[1..17].copy_from_slice(&withdraw_amount_a.to_le_bytes());
    let instruction_data = risc0_zkvm::serde::to_vec(&instruction).expect("Remove liquidity: AMM Program expects valid token transfer instruction data");
    let call_token_a = ChainedCall{
            program_id: vault_a.account.program_owner,
            instruction_data: instruction_data,
            pre_states: vec![vault_a.clone(), user_holding_a.clone()]
        };

    //Chaincall for Token B withdraw
    let mut instruction: [u8;32] = [0; 32];
    instruction[0] = 1;      
    instruction[1..17].copy_from_slice(&withdraw_amount_b.to_le_bytes());
    let instruction_data = risc0_zkvm::serde::to_vec(&instruction).expect("Remove liquidity: AMM Program expects valid token transfer instruction data");
    let call_token_b = ChainedCall{
            program_id: vault_b.account.program_owner,
            instruction_data: instruction_data,
            pre_states: vec![vault_b.clone(), user_holding_b.clone()]
        };

    //TODO: make this a call for burn once implemented in
    // Token Program
    //Chaincall for LP adjustment        
    let mut instruction: [u8;32] = [0; 32];
    instruction[0] = 1;      
    instruction[1..17].copy_from_slice(&delta_lp.to_le_bytes());
    let instruction_data = risc0_zkvm::serde::to_vec(&instruction).expect("Remove liquidity: AMM Program expects valid token transfer instruction data");
    let call_token_lp = ChainedCall{
            program_id: pool_lp.account.program_owner,
            instruction_data: instruction_data,
            pre_states: vec![user_holding_lp.clone(), pool_lp.clone()]
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
    use nssa_core::{{account::{Account, AccountId, AccountWithMetadata, Data}, program::ChainedCall}, program::ProgramId};

    use crate::{PoolDefinition, TOKEN_HOLDING_TYPE, TokenHolding, add_liquidity, new_definition, remove_liquidity, swap};


    const TOKEN_PROGRAM_ID: ProgramId = [15;8];

    enum AccountEnum {
        user_holding_b,
        user_holding_a,
        vault_a_uninit,
        vault_b_uninit,
        vault_a_init,
        vault_b_init,
        vault_a_wrong_acc_id,
        vault_b_wrong_acc_id,
        pool_lp_uninit,
        pool_lp_init,
        pool_lp_wrong_acc_id,
        user_holding_lp_uninit,
        user_holding_lp_init,
        pool_definition_uninit,
        pool_definition_init,
        pool_definition_unauth,
    }

    enum BalanceEnum {
        vault_a_reserve_init,
        vault_b_reserve_init,
        user_token_a_bal,
        user_token_b_bal,
        user_token_lp_bal,
        remove_min_amount_a,
        remove_min_amount_b,
        remove_amount_lp,
        remove_amount_lp_too_large,
        add_amount_a,
        add_amount_b,
    }

    fn helper_balance_constructor(selection: BalanceEnum) -> u128 {
        match selection {
            BalanceEnum::vault_a_reserve_init => 1000,
            BalanceEnum::vault_b_reserve_init => 250,
            BalanceEnum::user_token_a_bal => 1000,
            BalanceEnum::user_token_b_bal => 500,
            BalanceEnum::user_token_lp_bal => 100,
            BalanceEnum::remove_min_amount_a => 50,
            BalanceEnum::remove_min_amount_b => 50,
            BalanceEnum::remove_amount_lp => 50,
            BalanceEnum::remove_amount_lp_too_large => 150,
            BalanceEnum::add_amount_a => 500,
            BalanceEnum::add_amount_b => 200,
            _ => panic!("Invalid selection")
        }
    } 

    enum IdEnum {
        token_a_definition_id,
        token_b_definition_id,
        token_lp_definition_id,
        user_token_a_id,
        user_token_b_id,
        user_token_lp_id,
        pool_definition_id,
        vault_a_id,
        vault_b_id,
        pool_lp_id,
    }

    fn helper_id_constructor(selection: IdEnum) -> AccountId {

        match selection {
            IdEnum::token_a_definition_id => AccountId::new([42;32]),
            IdEnum::token_b_definition_id => AccountId::new([43;32]),
            IdEnum::token_lp_definition_id => AccountId::new([44;32]),
            IdEnum::user_token_a_id => AccountId::new([45;32]),
            IdEnum::user_token_b_id => AccountId::new([46;32]),
            IdEnum::user_token_lp_id => AccountId::new([47;32]),
            IdEnum::pool_definition_id => AccountId::new([48;32]),
            IdEnum::vault_a_id => AccountId::new([45;32]),
            IdEnum::vault_b_id => AccountId::new([46;32]),
            IdEnum::pool_lp_id => AccountId::new([47;32]),
            _ => panic!("Invalid selection")
        }
    }

    fn helper_account_constructor(selection: AccountEnum) -> AccountWithMetadata {
        let amm_program_id: ProgramId = [16;8];
        
        match selection {
            AccountEnum::user_holding_a => AccountWithMetadata {
                account: Account {
                    program_owner:  TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(
                        TokenHolding{
                            account_type: 1u8,
                            definition_id: helper_id_constructor(IdEnum::token_a_definition_id),
                            balance: helper_balance_constructor(BalanceEnum::user_token_a_bal),
                        }),
                        nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::user_token_a_id),
            },
            AccountEnum::user_holding_b => AccountWithMetadata {
                    account: Account {
                        program_owner:  TOKEN_PROGRAM_ID,
                        balance: 0u128,
                        data: TokenHolding::into_data(
                            TokenHolding{
                                account_type: 1u8,
                                definition_id: helper_id_constructor(IdEnum::token_b_definition_id),
                                balance: helper_balance_constructor(BalanceEnum::user_token_b_bal),
                            }),
                        nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::user_token_b_id),
            },
            AccountEnum::vault_a_uninit => AccountWithMetadata {
                account: Account {
                    program_owner:  TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(
                        TokenHolding{
                            account_type: 1u8,
                            definition_id: helper_id_constructor(IdEnum::token_a_definition_id),
                            balance: 0,
                        }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::vault_a_id),
            },
            AccountEnum::vault_b_uninit => AccountWithMetadata {
                account: Account {
                    program_owner:  TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(
                        TokenHolding{
                            account_type: 1u8,
                            definition_id: helper_id_constructor(IdEnum::token_b_definition_id),
                            balance: 0,
                        }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::vault_b_id),
            },
            AccountEnum::vault_a_init => AccountWithMetadata {
                account: Account {
                    program_owner:  TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(
                        TokenHolding{
                            account_type: 1u8,
                            definition_id: helper_id_constructor(IdEnum::token_a_definition_id),
                            balance: helper_balance_constructor(BalanceEnum::vault_a_reserve_init),
                        }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::vault_a_id),
            },
            AccountEnum::vault_b_init => AccountWithMetadata {
                account: Account {
                    program_owner:  TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(
                        TokenHolding{
                            account_type: 1u8,
                            definition_id: helper_id_constructor(IdEnum::token_b_definition_id),
                            balance: helper_balance_constructor(BalanceEnum::vault_b_reserve_init),
                        }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::vault_b_id),
            },
            AccountEnum::vault_a_wrong_acc_id => AccountWithMetadata {
                account: Account {
                    program_owner:  TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(
                        TokenHolding{
                            account_type: 1u8,
                            definition_id: helper_id_constructor(IdEnum::token_a_definition_id),
                            balance: helper_balance_constructor(BalanceEnum::vault_a_reserve_init),
                        }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::vault_b_id),
            },
            AccountEnum::vault_b_wrong_acc_id => AccountWithMetadata {
                account: Account {
                    program_owner:  TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(
                        TokenHolding{
                            account_type: 1u8,
                            definition_id: helper_id_constructor(IdEnum::token_b_definition_id),
                            balance: helper_balance_constructor(BalanceEnum::vault_b_reserve_init),
                        }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::vault_a_id),
            },
            AccountEnum::pool_lp_uninit => AccountWithMetadata {
                account: Account {
                    program_owner:  TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(
                        TokenHolding{
                            account_type: 1u8,
                            definition_id: helper_id_constructor(IdEnum::token_lp_definition_id),
                            balance: 0,
                        }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::pool_lp_id),
            },
            AccountEnum::pool_lp_init => AccountWithMetadata {
                account: Account {
                    program_owner:  TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(
                        TokenHolding{
                            account_type: 1u8,
                            definition_id: helper_id_constructor(IdEnum::token_lp_definition_id),
                            balance: helper_balance_constructor(BalanceEnum::vault_a_reserve_init),
                        }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::pool_lp_id),
            },
            AccountEnum::pool_lp_wrong_acc_id => AccountWithMetadata {
                account: Account {
                    program_owner:  TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(
                        TokenHolding{
                            account_type: 1u8,
                            definition_id: helper_id_constructor(IdEnum::token_lp_definition_id),
                            balance: helper_balance_constructor(BalanceEnum::vault_a_reserve_init),
                        }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::vault_a_id),
            },
            AccountEnum::user_holding_lp_uninit => AccountWithMetadata {
                account: Account {
                    program_owner:  TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(
                        TokenHolding{
                            account_type: 1u8,
                            definition_id: helper_id_constructor(IdEnum::token_lp_definition_id),
                            balance: 0,
                        }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::user_token_lp_id),
            },
            AccountEnum::user_holding_lp_init => AccountWithMetadata {
                account: Account {
                    program_owner:  TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(
                        TokenHolding{
                            account_type: 1u8,
                            definition_id: helper_id_constructor(IdEnum::token_lp_definition_id),
                            balance: helper_balance_constructor(BalanceEnum::user_token_lp_bal),
                        }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::user_token_lp_id),
            },
            AccountEnum::pool_definition_uninit => AccountWithMetadata {
                account: Account::default(),
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::pool_definition_id),
            },
            AccountEnum::pool_definition_init => AccountWithMetadata {
                account: Account {
                        program_owner:  amm_program_id,
                        balance: 0u128,
                        data: PoolDefinition::into_data(
                        PoolDefinition {
                            definition_token_a_id: helper_id_constructor(IdEnum::token_a_definition_id),
                            definition_token_b_id: helper_id_constructor(IdEnum::token_b_definition_id),
                            vault_a_addr: helper_id_constructor(IdEnum::vault_a_id),
                            vault_b_addr: helper_id_constructor(IdEnum::vault_b_id),
                            liquidity_pool_id: helper_id_constructor(IdEnum::token_lp_definition_id),
                            liquidity_pool_supply: helper_balance_constructor(BalanceEnum::vault_a_reserve_init),
                            reserve_a: helper_balance_constructor(BalanceEnum::vault_a_reserve_init),
                            reserve_b: helper_balance_constructor(BalanceEnum::vault_b_reserve_init),
                            active: true,
                        }),
                        nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::pool_definition_id),
            },
            AccountEnum::pool_definition_unauth => AccountWithMetadata {
                account: Account {
                        program_owner:  amm_program_id,
                        balance: 0u128,
                        data: PoolDefinition::into_data(
                        PoolDefinition {
                            definition_token_a_id: helper_id_constructor(IdEnum::token_a_definition_id),
                            definition_token_b_id: helper_id_constructor(IdEnum::token_b_definition_id),
                            vault_a_addr: helper_id_constructor(IdEnum::vault_a_id),
                            vault_b_addr: helper_id_constructor(IdEnum::vault_b_id),
                            liquidity_pool_id: helper_id_constructor(IdEnum::token_lp_definition_id),
                            liquidity_pool_supply: helper_balance_constructor(BalanceEnum::vault_a_reserve_init),
                            reserve_a: helper_balance_constructor(BalanceEnum::vault_a_reserve_init),
                            reserve_b: helper_balance_constructor(BalanceEnum::vault_b_reserve_init),
                            active: true,
                        }),
                        nonce: 0,
                },
                is_authorized: false,
                account_id: helper_id_constructor(IdEnum::pool_definition_id),
            },
            _ => panic!("Invalid selection"),
        }
    }


/*
TODO: delete
    fn helper_account_constructor(selection: AccountEnum) -> AccountWithMetadata {
        let amm_program_id: ProgramId = [15;8];
        let token_program_id: ProgramId = [16;8];
        let helper_id_constructor(IdEnum::token_a_definition_id) = AccountId::new([42;32]);
        let helper_id_constructor(IdEnum::token_b_definition_id) = AccountId::new([43;32]);
        let helper_id_constructor(IdEnum::token_lp_definition_id) = AccountId::new([44;32]);
        let user_token_a_id = AccountId::new([45;32]);
        let user_token_b_id = AccountId::new([46;32]);
        let user_token_lp_id = AccountId::new([47;32]);
        let pool_definition_id = AccountId::new([48;32]);
        let vault_a_id = AccountId::new([45;32]);
        let vault_b_id = AccountId::new([46;32]);
        let pool_lp_id = AccountId::new([47;32]);

        let helper_balance_constructor(BalanceEnum::vault_a_reserve_init): u128 = 1000;
        let helper_balance_constructor(BalanceEnum::vault_b_reserve_init): u128 = 250;
        let user_token_a_bal: u128 = 500;
        let helper_balance_constructor(BalanceEnum::user_token_b_bal): u128 = 250;
        let helper_balance_constructor(BalanceEnum::user_token_lp_bal): u128 = 100;

            enum AccountEnum {
        account_a_holding,
        account_b_holding,
        vault_a_uninit,
        vault_b_uninit,
        vault_a_init,
        vault_b_init,
        vault_a_wrong_acc_id,
        vault_b_wrong_acc_id,
        pool_lp_uninit,
        pool_lp_init,
        pool_lp_wrong_acc_id,
        account_lp_holding_uninit,
        account_lp_holding_init,
        pool_definition_uninit,
        pool_definition_init,

            enum BalanceEnum {
        uninit_balance,
        vault_a_reserve_init,
        vault_b_reserve_init,
        user_token_a_bal,
        user_token_b_bal,
        user_token_lp_bal
    }

    }

    //        amm_pool is a default account that will initiate the amm definition account values
//        vault_holding_a is a token holding account for token a
//        vault_holding_b is a token holding account for token b
//        pool_lp is a token holding account for the pool's lp token 
//        user_holding_a is a token holding account for token a
//        user_holding_b is a token holding account for token b
//        user_holding_lp is a token holding account for lp token

*/


    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]    
    fn test_call_new_definition_with_invalid_number_of_accounts_1() {
        let pre_states = vec![ helper_account_constructor(AccountEnum::pool_definition_uninit),]
        ;
        let _post_states = new_definition(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::user_token_a_bal),
                    helper_balance_constructor(BalanceEnum::user_token_b_bal)],
                    TOKEN_PROGRAM_ID);
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_new_definition_with_invalid_number_of_accounts_2() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_uninit),
                helper_account_constructor(AccountEnum::vault_a_uninit),
                ];
        let _post_states = new_definition(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::user_token_a_bal),
                    helper_balance_constructor(BalanceEnum::user_token_b_bal)],
                    TOKEN_PROGRAM_ID);
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_new_definition_with_invalid_number_of_accounts_3() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_uninit),
                helper_account_constructor(AccountEnum::vault_a_uninit),
                helper_account_constructor(AccountEnum::vault_b_uninit),
                ];
        let _post_states = new_definition(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::user_token_a_bal),
                    helper_balance_constructor(BalanceEnum::user_token_b_bal)],
                    TOKEN_PROGRAM_ID);
   }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_new_definition_with_invalid_number_of_accounts_4() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_uninit),
                helper_account_constructor(AccountEnum::vault_a_uninit),
                helper_account_constructor(AccountEnum::vault_b_uninit),
                helper_account_constructor(AccountEnum::pool_lp_uninit),
                ];
        let _post_states = new_definition(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::user_token_a_bal),
                    helper_balance_constructor(BalanceEnum::user_token_b_bal)],
                    TOKEN_PROGRAM_ID);
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_new_definition_with_invalid_number_of_accounts_5() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_uninit),
                helper_account_constructor(AccountEnum::vault_a_uninit),
                helper_account_constructor(AccountEnum::vault_b_uninit),
                helper_account_constructor(AccountEnum::pool_lp_uninit),
                helper_account_constructor(AccountEnum::user_holding_a),
                ];
        let _post_states = new_definition(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::user_token_a_bal),
                    helper_balance_constructor(BalanceEnum::user_token_b_bal)],
                    TOKEN_PROGRAM_ID);
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_new_definition_with_invalid_number_of_accounts_6() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_uninit),
                helper_account_constructor(AccountEnum::vault_a_uninit),
                helper_account_constructor(AccountEnum::vault_b_uninit),
                helper_account_constructor(AccountEnum::pool_lp_uninit),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                ];
        let _post_states = new_definition(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::user_token_a_bal),
                    helper_balance_constructor(BalanceEnum::user_token_b_bal)],
                    TOKEN_PROGRAM_ID);
    }

    #[should_panic(expected = "Invalid number of balance")]
    #[test]
    fn test_call_new_definition_with_invalid_number_of_balances_1() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_uninit),
                helper_account_constructor(AccountEnum::vault_a_uninit),
                helper_account_constructor(AccountEnum::vault_b_uninit),
                helper_account_constructor(AccountEnum::pool_lp_uninit),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                helper_account_constructor(AccountEnum::user_holding_lp_uninit),
                ];
        let _post_states = new_definition(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::user_token_a_bal)],
                    TOKEN_PROGRAM_ID);
    }
    
    #[should_panic(expected = "Pool account is initiated or not authorized")]
    #[test]
    fn test_call_new_definition_with_initiated_pool() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_uninit),
                helper_account_constructor(AccountEnum::vault_b_uninit),
                helper_account_constructor(AccountEnum::pool_lp_uninit),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                helper_account_constructor(AccountEnum::user_holding_lp_uninit),
                ];
        let _post_states = new_definition(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::user_token_a_bal),
                    helper_balance_constructor(BalanceEnum::user_token_b_bal)],
                    TOKEN_PROGRAM_ID);
    }

    #[should_panic(expected = "Pool account is initiated or not authorized")]
    #[test]
    fn test_call_new_definition_with_unauthorized_pool() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_unauth),
                helper_account_constructor(AccountEnum::vault_a_uninit),
                helper_account_constructor(AccountEnum::vault_b_uninit),
                helper_account_constructor(AccountEnum::pool_lp_uninit),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                helper_account_constructor(AccountEnum::user_holding_lp_uninit),
                ];
        let _post_states = new_definition(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::user_token_a_bal),
                    helper_balance_constructor(BalanceEnum::user_token_b_bal)],
                    TOKEN_PROGRAM_ID);
    }
    
    #[should_panic(expected = "Balances must be nonzero")]
    #[test]
    fn test_call_new_definition_with_balance_zero_1() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_uninit),
                helper_account_constructor(AccountEnum::vault_a_uninit),
                helper_account_constructor(AccountEnum::vault_b_uninit),
                helper_account_constructor(AccountEnum::pool_lp_uninit),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                helper_account_constructor(AccountEnum::user_holding_lp_uninit),
                ];
        let _post_states = new_definition(&pre_states, 
                    &[0,
                    helper_balance_constructor(BalanceEnum::user_token_b_bal)],
                    TOKEN_PROGRAM_ID);
    }      

    #[should_panic(expected = "Balances must be nonzero")]
    #[test]
    fn test_call_new_definition_with_balance_zero_2() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_uninit),
                helper_account_constructor(AccountEnum::vault_a_uninit),
                helper_account_constructor(AccountEnum::vault_b_uninit),
                helper_account_constructor(AccountEnum::pool_lp_uninit),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                helper_account_constructor(AccountEnum::user_holding_lp_uninit),
                ];
        let _post_states = new_definition(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::user_token_a_bal),
                    0],
                    TOKEN_PROGRAM_ID);
    }
    
    #[should_panic(expected = "Cannot set up a swap for a token with itself.")]
    #[test]
    fn test_call_new_definition_same_token() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_uninit),
                helper_account_constructor(AccountEnum::vault_a_uninit),
                helper_account_constructor(AccountEnum::vault_b_uninit),
                helper_account_constructor(AccountEnum::pool_lp_uninit),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_lp_uninit),
                ];
        let _post_states = new_definition(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::user_token_a_bal), 
                    helper_balance_constructor(BalanceEnum::user_token_b_bal)],
                    TOKEN_PROGRAM_ID);
    }

    //TODO: fix this
    #[test]
    fn test_call_new_definition_chain_call_success() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_uninit),
                helper_account_constructor(AccountEnum::vault_a_uninit),
                helper_account_constructor(AccountEnum::vault_b_uninit),
                helper_account_constructor(AccountEnum::pool_lp_uninit),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                helper_account_constructor(AccountEnum::user_holding_lp_uninit),
                ];
        let post_states = new_definition(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::user_token_a_bal), 
                    helper_balance_constructor(BalanceEnum::user_token_b_bal)],
                    TOKEN_PROGRAM_ID);
    }



    /*TODO: ^^^ need to chain call checks
        let user_holding_a = AccountWithMetadata {
            account: user_holding_a.clone(),
            is_authorized: true,
            account_id: AccountId::new([5; 32])};
    
        let user_holding_b = AccountWithMetadata {
            account: user_holding_b.clone(),
            is_authorized: true,
            account_id: AccountId::new([6; 32])};

        let user_holding_lp = AccountWithMetadata {
            account: user_holding_lp.clone(),
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
            user_holding_a.clone(),
            user_holding_b.clone(),
            user_holding_lp.clone(),
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
            pre_states: vec![user_holding_a.clone(), vault_a.clone()],
        };

        //Expected chain call for Token B
        let mut instruction: [u8;32] = [0; 32];
        instruction[0] = 1;      
        instruction[1..17].copy_from_slice(&balance_b.to_le_bytes());
        let instruction_data = risc0_zkvm::serde::to_vec(&instruction).unwrap();
        let expected_chained_call_b = ChainedCall{
            program_id: token_program_id,
            instruction_data,
            pre_states: vec![user_holding_b.clone(), vault_b.clone()],
        };

        //Expected chain call for LP
        let mut instruction: [u8;32] = [0; 32];
        instruction[0] = 1;      
        instruction[1..17].copy_from_slice(&balance_a.to_le_bytes());
        let instruction_data = risc0_zkvm::serde::to_vec(&instruction).unwrap();
        let expected_chained_call_lp = ChainedCall{
            program_id: token_program_id,
            instruction_data,
            pre_states: vec![pool_lp.clone(), user_holding_lp.clone()],
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
    }*/

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]    
    fn test_call_remove_liquidity_with_invalid_number_of_accounts_1() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                ];
        let _post_states = remove_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::remove_amount_lp), 
                    helper_balance_constructor(BalanceEnum::remove_min_amount_a),
                    helper_balance_constructor(BalanceEnum::remove_min_amount_b)],
                    );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_remove_liquidity_with_invalid_number_of_accounts_2() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_init),
                ];
        let _post_states = remove_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::remove_amount_lp), 
                    helper_balance_constructor(BalanceEnum::remove_min_amount_a),
                    helper_balance_constructor(BalanceEnum::remove_min_amount_b)],
                    );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_remove_liquidity_with_invalid_number_of_accounts_3() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_init),
                helper_account_constructor(AccountEnum::vault_b_init),
                ];
        let _post_states = remove_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::remove_amount_lp), 
                    helper_balance_constructor(BalanceEnum::remove_min_amount_a),
                    helper_balance_constructor(BalanceEnum::remove_min_amount_b)],
                    );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_remove_liquidity_with_invalid_number_of_accounts_4() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_init),
                helper_account_constructor(AccountEnum::vault_b_init),
                helper_account_constructor(AccountEnum::pool_lp_init),
                ];
        let _post_states = remove_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::remove_amount_lp), 
                    helper_balance_constructor(BalanceEnum::remove_min_amount_a),
                    helper_balance_constructor(BalanceEnum::remove_min_amount_b)],
                    );
    }
 
    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_remove_liquidity_with_invalid_number_of_accounts_5() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_init),
                helper_account_constructor(AccountEnum::vault_b_init),
                helper_account_constructor(AccountEnum::pool_lp_init),
                helper_account_constructor(AccountEnum::user_holding_a),
                ];
        let _post_states = remove_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::remove_amount_lp), 
                    helper_balance_constructor(BalanceEnum::remove_min_amount_a),
                    helper_balance_constructor(BalanceEnum::remove_min_amount_b)],
                    );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_remove_liquidity_with_invalid_number_of_accounts_6() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_init),
                helper_account_constructor(AccountEnum::vault_b_init),
                helper_account_constructor(AccountEnum::pool_lp_init),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                ];
        let _post_states = remove_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::remove_amount_lp), 
                    helper_balance_constructor(BalanceEnum::remove_min_amount_a),
                    helper_balance_constructor(BalanceEnum::remove_min_amount_b)],
                    );
    }

    #[should_panic(expected = "Vault A was not provided")]
    #[test]
    fn test_call_remove_liquidity_vault_a_omitted() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_wrong_acc_id),
                helper_account_constructor(AccountEnum::vault_b_init),
                helper_account_constructor(AccountEnum::pool_lp_init),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                helper_account_constructor(AccountEnum::user_holding_lp_init),
                ];
        let _post_states = remove_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::remove_amount_lp), 
                    helper_balance_constructor(BalanceEnum::remove_min_amount_a),
                    helper_balance_constructor(BalanceEnum::remove_min_amount_b)],
                    );
    }
    
    #[should_panic(expected = "Vault B was not provided")]
    #[test]
    fn test_call_remove_liquidity_vault_b_omitted() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_init),
                helper_account_constructor(AccountEnum::vault_b_wrong_acc_id),
                helper_account_constructor(AccountEnum::pool_lp_init),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                helper_account_constructor(AccountEnum::user_holding_lp_init),
                ];
        let _post_states = remove_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::remove_amount_lp), 
                    helper_balance_constructor(BalanceEnum::remove_min_amount_a),
                    helper_balance_constructor(BalanceEnum::remove_min_amount_b)],
                    );
    }

    #[test]
    //TODO: need to fix this test
    fn test_call_remove_liquidity_chain_call_success() {
        let mut pool = Account::default();
        let mut vault_a = Account::default();
        let mut vault_b = Account::default();
        let mut user_holding_a = Account::default();
        let mut user_holding_b = Account::default();
        let mut pool_lp = Account::default();
        let mut user_holding_lp = Account::default();

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
        let liquidity_pool_supply: u128 = 30u128;
        let reserve_a: u128 = 30;
        let reserve_b: u128 = 20;
        let user_holding_lp_amt: u128 = 10;
        let token_program_id: [u32;8] = [0; 8];

        pool.data = PoolDefinition::into_data( PoolDefinition {
            definition_token_a_id: definition_token_a_id.clone(),
            definition_token_b_id: definition_token_b_id.clone(),
            vault_a_addr: vault_a_addr.clone(),
            vault_b_addr: vault_b_addr.clone(),
            liquidity_pool_id,
            liquidity_pool_supply: reserve_a,
            reserve_a,
            reserve_b,
            active: true,
        });
        
        user_holding_a.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 5u128 }
        );

        user_holding_b.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_b_id.clone(),
                balance: 5u128 }
        );

        user_holding_lp.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id: AccountId::new([3;32]),
                balance: user_holding_lp_amt }
        );

        let user_holding_a = AccountWithMetadata {
            account: user_holding_a.clone(),
            is_authorized: true,
            account_id: AccountId::new([5; 32])};
    
        let user_holding_b = AccountWithMetadata {
            account: user_holding_b.clone(),
            is_authorized: true,
            account_id: AccountId::new([6; 32])};

        let user_holding_lp = AccountWithMetadata {
            account: user_holding_lp.clone(),
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
            user_holding_a.clone(),
            user_holding_b.clone(),
            user_holding_lp.clone(),
        ];

        let amount_lp =  5;
        let amount_min_a = 2;
        let amount_min_b = 2;
        let (post_states, chained_calls) = remove_liquidity(&pre_states, &[amount_lp, amount_min_a, amount_min_b]);

        let chained_call_lp = chained_calls[0].clone();
        let chained_call_b = chained_calls[1].clone();
        let chained_call_a = chained_calls[2].clone();
        
        //Expected withdraw
        let withdraw_amount_a = reserve_a * (amount_lp/reserve_a);
        let withdraw_amount_b = reserve_b * (amount_lp/reserve_a);

        //Expected chain_call for Token A
        let mut instruction: [u8;32] = [0; 32];
        instruction[0] = 1;      
        instruction[1..17].copy_from_slice(&withdraw_amount_a.to_le_bytes());
        let instruction_data = risc0_zkvm::serde::to_vec(&instruction).unwrap();
        let expected_chained_call_a = ChainedCall{
            program_id: token_program_id,
            instruction_data,
            pre_states: vec![vault_a.clone(), user_holding_a.clone()],
        };

        //Expected chain call for Token B
        let mut instruction: [u8;32] = [0; 32];
        instruction[0] = 1;      
        instruction[1..17].copy_from_slice(&withdraw_amount_b.to_le_bytes());
        let instruction_data = risc0_zkvm::serde::to_vec(&instruction).unwrap();
        let expected_chained_call_b = ChainedCall{
            program_id: token_program_id,
            instruction_data,
            pre_states: vec![vault_b.clone(), user_holding_b.clone()],
        };

        //Expected chain call for LP
        let mut instruction: [u8;32] = [0; 32];
        instruction[0] = 1;      
        instruction[1..17].copy_from_slice(&user_holding_lp_amt.to_le_bytes());
        let instruction_data = risc0_zkvm::serde::to_vec(&instruction).unwrap();
        let expected_chained_call_lp = ChainedCall{
            program_id: token_program_id,
            instruction_data,
            pre_states: vec![user_holding_lp.clone(), pool_lp.clone()],
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
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                ];
        let _post_states = add_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::remove_amount_lp), 
                    helper_balance_constructor(BalanceEnum::add_amount_a),
                    helper_balance_constructor(BalanceEnum::add_amount_b)],
                    helper_id_constructor(IdEnum::vault_a_id),
                    );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_add_liquidity_with_invalid_number_of_accounts_2() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_init),
                ];
        let _post_states = add_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::remove_amount_lp), 
                    helper_balance_constructor(BalanceEnum::add_amount_a),
                    helper_balance_constructor(BalanceEnum::add_amount_b)],
                    helper_id_constructor(IdEnum::vault_a_id),
                    );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_add_liquidity_with_invalid_number_of_accounts_3() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_init),
                helper_account_constructor(AccountEnum::vault_b_init),
                ];
        let _post_states = add_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::remove_amount_lp), 
                    helper_balance_constructor(BalanceEnum::add_amount_a),
                    helper_balance_constructor(BalanceEnum::add_amount_b)],
                    helper_id_constructor(IdEnum::vault_a_id),
                    );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_add_liquidity_with_invalid_number_of_accounts_4() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_init),
                helper_account_constructor(AccountEnum::vault_b_init),
                helper_account_constructor(AccountEnum::pool_lp_init),
                ];
        let _post_states = add_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::remove_amount_lp), 
                    helper_balance_constructor(BalanceEnum::add_amount_a),
                    helper_balance_constructor(BalanceEnum::add_amount_b)],
                    helper_id_constructor(IdEnum::vault_a_id),
                    );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_add_liquidity_with_invalid_number_of_accounts_5() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_init),
                helper_account_constructor(AccountEnum::vault_b_init),
                helper_account_constructor(AccountEnum::pool_lp_init),
                helper_account_constructor(AccountEnum::user_holding_a),
                ];
        let _post_states = add_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::remove_amount_lp), 
                    helper_balance_constructor(BalanceEnum::add_amount_a),
                    helper_balance_constructor(BalanceEnum::add_amount_b)],
                    helper_id_constructor(IdEnum::vault_a_id),
                    );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_add_liquidity_with_invalid_number_of_accounts_6() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_init),
                helper_account_constructor(AccountEnum::vault_b_init),
                helper_account_constructor(AccountEnum::pool_lp_init),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                ];
        let _post_states = add_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::remove_amount_lp), 
                    helper_balance_constructor(BalanceEnum::add_amount_a),
                    helper_balance_constructor(BalanceEnum::add_amount_b)],
                    helper_id_constructor(IdEnum::vault_a_id),
                    );
    }

        /*
    
                    helper_account_constructor(AccountEnum::vault_b_init),
                helper_account_constructor(AccountEnum::pool_lp_init),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                helper_account_constructor(AccountEnum::user_holding_lp_init), */

    
    #[should_panic(expected = "Invalid number of input balances")]
    #[test]
    fn test_call_add_liquidity_invalid_number_of_balances_1() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_init),
                helper_account_constructor(AccountEnum::vault_b_init),
                helper_account_constructor(AccountEnum::pool_lp_init),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                helper_account_constructor(AccountEnum::user_holding_lp_init),
                ];
        let _post_states = add_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::add_amount_a),],
                    helper_id_constructor(IdEnum::vault_a_id),
                    );
    }

    #[should_panic(expected = "Vault A was not provided")]
    #[test]
    fn test_call_add_liquidity_vault_a_omitted() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_wrong_acc_id),
                helper_account_constructor(AccountEnum::vault_b_init),
                helper_account_constructor(AccountEnum::pool_lp_init),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                helper_account_constructor(AccountEnum::user_holding_lp_init),
                ];
        let _post_states = add_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::add_amount_a), 
                    helper_balance_constructor(BalanceEnum::add_amount_b),],
                    helper_id_constructor(IdEnum::vault_a_id),
                    );
    }        

    #[should_panic(expected = "Vault B was not provided")]
    #[test]
    fn test_call_add_liquidity_vault_b_omitted() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_init),
                helper_account_constructor(AccountEnum::vault_b_wrong_acc_id),
                helper_account_constructor(AccountEnum::pool_lp_init),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                helper_account_constructor(AccountEnum::user_holding_lp_init),
                ];
        let _post_states = add_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::add_amount_a), 
                    helper_balance_constructor(BalanceEnum::add_amount_b),],
                    helper_id_constructor(IdEnum::vault_a_id),
                    );
    }    

    #[should_panic(expected = "Both max-balances must be nonzero")]
    #[test]
    fn test_call_add_liquidity_zero_balance_1() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_init),
                helper_account_constructor(AccountEnum::vault_b_init),
                helper_account_constructor(AccountEnum::pool_lp_init),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                helper_account_constructor(AccountEnum::user_holding_lp_init),
                ];
        let _post_states = add_liquidity(&pre_states, 
                    &[0, 
                    helper_balance_constructor(BalanceEnum::add_amount_b),],
                    helper_id_constructor(IdEnum::vault_a_id),
                    );
    }

    #[should_panic(expected = "Both max-balances must be nonzero")]
    #[test]
    fn test_call_add_liquidity_zero_balance_2() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_init),
                helper_account_constructor(AccountEnum::vault_b_init),
                helper_account_constructor(AccountEnum::pool_lp_init),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                helper_account_constructor(AccountEnum::user_holding_lp_init),
                ];
        let _post_states = add_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::add_amount_a), 
                    0,],
                    helper_id_constructor(IdEnum::vault_a_id),
                    );
    }

    /*
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
        let liquidity_pool_supply: u128 = 30u128;
        let reserve_a: u128 = 10;
        let reserve_b: u128 = 20;
        let token_program_id: [u32;8] = [0; 8];

        pool.data = PoolDefinition::into_data( PoolDefinition {
            definition_token_a_id,
            definition_token_b_id,
            vault_a_addr: vault_a_addr.clone(),
            vault_b_addr: vault_b_addr.clone(),
            liquidity_pool_id,
            liquidity_pool_supply,
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
        let liquidity_pool_supply: u128 = 30u128;
        let reserve_a: u128 = 10;
        let reserve_b: u128 = 20;
        let token_program_id: [u32;8] = [0; 8];

        pool.data = PoolDefinition::into_data( PoolDefinition {
            definition_token_a_id: definition_token_a_id.clone(),
            definition_token_b_id: definition_token_b_id.clone(),
            vault_a_addr: vault_a_addr.clone(),
            vault_b_addr: vault_b_addr.clone(),
            liquidity_pool_id,
            liquidity_pool_supply,
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
        let liquidity_pool_supply: u128 = 30u128;
        let reserve_a: u128 = 10;
        let reserve_b: u128 = 20;
        let token_program_id: [u32;8] = [0; 8];

        pool.data = PoolDefinition::into_data( PoolDefinition {
            definition_token_a_id: definition_token_a_id.clone(),
            definition_token_b_id: definition_token_b_id.clone(),
            vault_a_addr: vault_a_addr.clone(),
            vault_b_addr: vault_b_addr.clone(),
            liquidity_pool_id,
            liquidity_pool_supply,
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
        let mut user_holding_a = Account::default();
        let mut user_holding_b = Account::default();


        let definition_token_a_id = AccountId::new([1;32]);
        let definition_token_b_id = AccountId::new([2;32]); 

        vault1.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 15u128 }
        );

        user_holding_a.data = TokenHolding::into_data(
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

        user_holding_b.data = TokenHolding::into_data(
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
        let liquidity_pool_supply: u128 = 30u128;
        let reserve_a: u128 = 15;
        let reserve_b: u128 = 15;
        let token_program_id: [u32;8] = [0; 8];

        pool.data = PoolDefinition::into_data( PoolDefinition {
            definition_token_a_id: definition_token_a_id.clone(),
            definition_token_b_id: definition_token_b_id.clone(),
            vault_a_addr: vault_a_addr.clone(),
            vault_b_addr: vault_b_addr.clone(),
            liquidity_pool_id,
            liquidity_pool_supply,
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
            account: user_holding_a,
            is_authorized: true,
            account_id: AccountId::new([5; 32])},
            AccountWithMetadata {
            account: user_holding_b,
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
        let mut user_holding_a = Account::default();
        let mut user_holding_b = Account::default();


        let definition_token_a_id = AccountId::new([1;32]);
        let definition_token_b_id = AccountId::new([2;32]); 

        vault1.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 15u128 }
        );

        user_holding_a.data = TokenHolding::into_data(
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

        user_holding_b.data = TokenHolding::into_data(
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
        let liquidity_pool_supply: u128 = 30u128;
        let reserve_a: u128 = 15;
        let reserve_b: u128 = 15;
        let token_program_id: [u32;8] = [0; 8];

        pool.data = PoolDefinition::into_data( PoolDefinition {
            definition_token_a_id: definition_token_a_id.clone(),
            definition_token_b_id: definition_token_b_id.clone(),
            vault_a_addr: vault_a_addr.clone(),
            vault_b_addr: vault_b_addr.clone(),
            liquidity_pool_id,
            liquidity_pool_supply,
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
            account: user_holding_a,
            is_authorized: true,
            account_id: AccountId::new([5; 32])},
            AccountWithMetadata {
            account: user_holding_b,
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
        let mut user_holding_a = Account::default();
        let mut user_holding_b = Account::default();
        let definition_token_a_id = AccountId::new([1;32]);
        let definition_token_b_id = AccountId::new([2;32]); 

        vault1.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 1500u128 }
        );

        user_holding_a.data = TokenHolding::into_data(
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



        user_holding_b.data = TokenHolding::into_data(
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
        let liquidity_pool_supply: u128 = 30u128;
        let reserve_a: u128 = 1500u128;
        let reserve_b: u128 = 2000u128;
        let token_program_id: [u32;8] = [0; 8];

        pool.data = PoolDefinition::into_data( PoolDefinition {
            definition_token_a_id: definition_token_a_id.clone(),
            definition_token_b_id: definition_token_b_id.clone(),
            vault_a_addr: vault_a_addr.clone(),
            vault_b_addr: vault_b_addr.clone(),
            liquidity_pool_id,
            liquidity_pool_supply,
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
            account: user_holding_a,
            is_authorized: true,
            account_id: AccountId::new([5; 32])},
            AccountWithMetadata {
            account: user_holding_b,
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
        let liquidity_pool_supply: u128 = 30u128;
        let reserve_a: u128 = 15;
        let reserve_b: u128 = 0;
        let token_program_id: [u32;8] = [0; 8];

        pool.data = PoolDefinition::into_data( PoolDefinition {
            definition_token_a_id: definition_token_a_id.clone(),
            definition_token_b_id: definition_token_b_id.clone(),
            vault_a_addr: vault_a_addr.clone(),
            vault_b_addr: vault_b_addr.clone(),
            liquidity_pool_id,
            liquidity_pool_supply,
            reserve_a,
            reserve_b,
            token_program_id,
        });

        let mut user_holding_a = Account::default();
        let mut user_holding_b = Account::default();

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
            account: user_holding_a,
            is_authorized: true,
            account_id: AccountId::new([5; 32])},
            AccountWithMetadata {
            account: user_holding_b,
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
        let liquidity_pool_supply: u128 = 30u128;
        let reserve_a: u128 = 0;
        let reserve_b: u128 = 15;
        let token_program_id: [u32;8] = [0; 8];

        pool.data = PoolDefinition::into_data( PoolDefinition {
            definition_token_a_id: definition_token_a_id.clone(),
            definition_token_b_id: definition_token_b_id.clone(),
            vault_a_addr: vault_a_addr.clone(),
            vault_b_addr: vault_b_addr.clone(),
            liquidity_pool_id,
            liquidity_pool_supply,
            reserve_a,
            reserve_b,
            token_program_id,
        });

        let mut user_holding_a = Account::default();
        let mut user_holding_b = Account::default();

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
            account: user_holding_a,
            is_authorized: true,
            account_id: AccountId::new([5; 32])},
            AccountWithMetadata {
            account: user_holding_b,
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
        let mut user_holding_a = Account::default();
        let mut user_holding_b = Account::default();
        let mut pool_lp = Account::default();
        let mut user_holding_lp = Account::default();

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
        let liquidity_pool_supply: u128 = 30u128;
        let reserve_a: u128 = 30;
        let reserve_b: u128 = 20;
        let user_holding_lp_amt: u128 = 10;
        let token_program_id: [u32;8] = [0; 8];
        let vault_a_addr = AccountId::new([7;32]);
        let vault_b_addr = AccountId::new([9;32]);

        pool.data = PoolDefinition::into_data( PoolDefinition {
            definition_token_a_id: definition_token_a_id.clone(),
            definition_token_b_id: definition_token_b_id.clone(),
            vault_a_addr: vault_a_addr.clone(),
            vault_b_addr: vault_b_addr.clone(),
            liquidity_pool_id,
            liquidity_pool_supply: reserve_a,
            reserve_a,
            reserve_b,
            token_program_id,
        });
        
        user_holding_a.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 50u128 }
        );

        user_holding_b.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_b_id.clone(),
                balance: 50u128 }
        );

        user_holding_lp.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id: AccountId::new([3;32]),
                balance: user_holding_lp_amt }
        );

        let user_holding_a = AccountWithMetadata {
            account: user_holding_a.clone(),
            is_authorized: true,
            account_id: AccountId::new([5; 32])};
    
        let user_holding_b = AccountWithMetadata {
            account: user_holding_b.clone(),
            is_authorized: true,
            account_id: AccountId::new([6; 32])};

        let user_holding_lp = AccountWithMetadata {
            account: user_holding_lp.clone(),
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
            user_holding_a.clone(),
            user_holding_b.clone(),
            user_holding_lp.clone(),
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
            pre_states: vec![user_holding_a.clone(), vault_a.clone()],
        };

        //Expected chain call for Token B
        let mut instruction: [u8;23] = [0; 23];
        instruction[0] = 1;      
        instruction[1..17].copy_from_slice(&expected_actual_amount_b.to_le_bytes());
        let instruction_data = risc0_zkvm::serde::to_vec(&instruction).unwrap();
        let expected_chained_call_b = ChainedCall{
            program_id: token_program_id,
            instruction_data,
            pre_states: vec![user_holding_b.clone(), vault_b.clone()],
        };

        //Expected chain call for LP
        let mut instruction: [u8;23] = [0; 23];
        instruction[0] = 1;      
        instruction[1..17].copy_from_slice(&expected_delta_lp.to_le_bytes());
        let instruction_data = risc0_zkvm::serde::to_vec(&instruction).unwrap();
        let expected_chained_call_lp = ChainedCall{
            program_id: token_program_id,
            instruction_data,
            pre_states: vec![pool_lp.clone(), user_holding_lp.clone()],
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
        let mut user_holding_a = Account::default();
        let mut user_holding_b = Account::default();
        let mut pool_lp = Account::default();
        let mut user_holding_lp = Account::default();

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
        let liquidity_pool_supply: u128 = 30u128;
        let reserve_a: u128 = 30;
        let reserve_b: u128 = 20;
        let user_holding_lp_amt: u128 = 10;
        let token_program_id: [u32;8] = [0; 8];
        let vault_a_addr = AccountId::new([2;32]);
        let vault_b_addr = AccountId::new([3;32]);

        pool.data = PoolDefinition::into_data( PoolDefinition {
            definition_token_a_id: definition_token_a_id.clone(),
            definition_token_b_id: definition_token_b_id.clone(),
            vault_a_addr: vault_a_addr.clone(),
            vault_b_addr: vault_b_addr.clone(),
            liquidity_pool_id,
            liquidity_pool_supply: reserve_a,
            reserve_a,
            reserve_b,
            token_program_id,
        });
        
        user_holding_a.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_a_id.clone(),
                balance: 50u128 }
        );

        user_holding_b.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id:definition_token_b_id.clone(),
                balance: 50u128 }
        );

        user_holding_lp.data = TokenHolding::into_data(
            TokenHolding { account_type: TOKEN_HOLDING_TYPE,
                definition_id: AccountId::new([3;32]),
                balance: user_holding_lp_amt }
        );

        let user_holding_a = AccountWithMetadata {
            account: user_holding_a.clone(),
            is_authorized: true,
            account_id: AccountId::new([5; 32])};
    
        let user_holding_b = AccountWithMetadata {
            account: user_holding_b.clone(),
            is_authorized: true,
            account_id: AccountId::new([6; 32])};

        let user_holding_lp = AccountWithMetadata {
            account: user_holding_lp.clone(),
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
            user_holding_a.clone(),
            user_holding_b.clone(),
            user_holding_lp.clone(),
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
            pre_states: vec![user_holding_a.clone(), vault_a.clone()],
        };

        //Expected chain call for Token B
        let mut instruction: [u8;23] = [0; 23];
        instruction[0] = 1;      
        instruction[1..17].copy_from_slice(&expected_actual_amount_b.to_le_bytes());
        let instruction_data = risc0_zkvm::serde::to_vec(&instruction).unwrap();
        let expected_chained_call_b = ChainedCall{
            program_id: token_program_id,
            instruction_data,
            pre_states: vec![user_holding_b.clone(), vault_b.clone()],
        };

        //Expected chain call for LP
        let mut instruction: [u8;23] = [0; 23];
        instruction[0] = 1;      
        instruction[1..17].copy_from_slice(&expected_delta_lp.to_le_bytes());
        let instruction_data = risc0_zkvm::serde::to_vec(&instruction).unwrap();
        let expected_chained_call_lp = ChainedCall{
            program_id: token_program_id,
            instruction_data,
            pre_states: vec![pool_lp.clone(), user_holding_lp.clone()],
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
        let mut user_holding_a = Account::default();
        let mut user_holding_b = Account::default();

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
        user_holding_a.data = vec![
                0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];
        user_holding_b.data = vec![
                1, 1, 1, 1, 1, 1, 1, 12, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];

        let definition_token_a_id = AccountId::new([1;32]);
        let definition_token_b_id = AccountId::new([2;32]);
        let vault_a_addr = AccountId::new([5;32]);
        let vault_b_addr = AccountId::new([6;32]);
        let liquidity_pool_id = AccountId::new([7;32]);
        let liquidity_pool_supply: u128 = 30u128;
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
                liquidity_pool_supply,
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
            account: user_holding_a.clone(),
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
            AccountWithMetadata {
            account: user_holding_b.clone(),
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
        let liquidity_pool_supply: u128 = 30u128;
        let reserve_a: u128 = 10;
        let reserve_b: u128 = 20;
        let token_program_id: [u32;8] = [0; 8];

        pool.data = PoolDefinition::into_data( PoolDefinition {
            definition_token_a_id,
            definition_token_b_id,
            vault_a_addr: vault_a_addr.clone(),
            vault_b_addr: vault_b_addr.clone(),
            liquidity_pool_id,
            liquidity_pool_supply,
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
        let liquidity_pool_supply: u128 = 30u128;
        let reserve_a: u128 = 10;
        let reserve_b: u128 = 20;
        let token_program_id: [u32;8] = [0; 8];

        pool.data = PoolDefinition::into_data( PoolDefinition {
            definition_token_a_id,
            definition_token_b_id,
            vault_a_addr: vault_a_addr.clone(),
            vault_b_addr: vault_b_addr.clone(),
            liquidity_pool_id,
            liquidity_pool_supply,
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
        let mut user_holding_a = Account::default();
        let mut user_holding_b = Account::default();

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
        user_holding_a.data = vec![
                0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];
        user_holding_b.data = vec![
                1, 1, 1, 1, 1, 1, 1, 12, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];

        let definition_token_a_id = AccountId::new([1;32]);
        let definition_token_b_id = AccountId::new([2;32]);
        let vault_a_addr = AccountId::new([5;32]);
        let vault_b_addr = AccountId::new([6;32]);
        let liquidity_pool_id = AccountId::new([7;32]);
        let liquidity_pool_supply: u128 = 30u128;
        let reserve_a: u128 = 15;
        let reserve_b: u128 = 20;
        let token_program_id: [u32;8] = [5; 8];

        user_holding_a.program_owner = token_program_id;
        user_holding_b.program_owner = token_program_id;
        vault_a.program_owner = token_program_id;
        vault_b.program_owner = token_program_id;

        pool.data = PoolDefinition::into_data(
            PoolDefinition {
                definition_token_a_id: definition_token_a_id.clone(),
                definition_token_b_id: definition_token_b_id.clone(),
                vault_a_addr: vault_a_addr.clone(),
                vault_b_addr: vault_b_addr.clone(),
                liquidity_pool_id,
                liquidity_pool_supply,
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
            account: user_holding_a.clone(),
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
            AccountWithMetadata {
            account: user_holding_b.clone(),
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
        assert!(chain_call_a_account0 == user_holding_a);
        assert!(chain_call_a_account1 == vault_a);
        assert!(chain_call_b.instruction_data == expected_instruction_data_1);
        assert!(chain_call_b_account0 == vault_b);
        assert!(chain_call_b_account1 == user_holding_b);
    }

    #[test]
    fn test_call_swap_successful_chain_call_2() {
        let mut pool = Account::default();
        let mut vault_a = Account::default();
        let mut vault_b = Account::default();
        let mut pool_lp = Account::default();
        let mut user_holding_a = Account::default();
        let mut user_holding_b = Account::default();

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
        user_holding_a.data = vec![
                0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];
        user_holding_b.data = vec![
                1, 1, 1, 1, 1, 1, 1, 12, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ];

        let definition_token_a_id = AccountId::new([1;32]);
        let definition_token_b_id = AccountId::new([2;32]);
        let vault_a_addr = AccountId::new([5;32]);
        let vault_b_addr = AccountId::new([6;32]);
        let liquidity_pool_id = AccountId::new([7;32]);
        let liquidity_pool_supply: u128 = 30u128;
        let reserve_a: u128 = 15;
        let reserve_b: u128 = 20;
        let token_program_id: [u32;8] = [5; 8];

        user_holding_a.program_owner = token_program_id;
        user_holding_b.program_owner = token_program_id;
        vault_a.program_owner = token_program_id;
        vault_b.program_owner = token_program_id;

        pool.data = PoolDefinition::into_data(
            PoolDefinition {
                definition_token_a_id: definition_token_a_id.clone(),
                definition_token_b_id: definition_token_b_id.clone(),
                vault_a_addr: vault_a_addr.clone(),
                vault_b_addr: vault_b_addr.clone(),
                liquidity_pool_id,
                liquidity_pool_supply,
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
            account: user_holding_a.clone(),
            is_authorized: true,
            account_id: AccountId::new([4; 32])},
            AccountWithMetadata {
            account: user_holding_b.clone(),
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
        assert!(chain_call_a_account1 == user_holding_a);
        assert!(chain_call_b.instruction_data == expected_instruction_data_1);
        assert!(chain_call_b_account0 == user_holding_b);
        assert!(chain_call_b_account1 == vault_b);

    }

    */
}