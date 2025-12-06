use nssa_core::{
    account::{Account, AccountId, AccountWithMetadata, Data},
    program::{ProgramId, ProgramInput, ChainedCall, read_nssa_inputs, write_nssa_outputs_with_chained_call},
};

//TODO update comments
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

const POOL_DEFINITION_DATA_SIZE: usize = 225;
const MAX_NUMBER_POOLS: usize = 31;
const AMM_DEFINITION_DATA_SIZE: usize = 1024;

struct AMMDefinition {
    name: [u8;32],
    pool_ids: Vec<AccountId>,
}

impl AMMDefinition {
    fn new(name: &[u8;32]) -> Vec<u8> {

        let mut bytes = [0; AMM_DEFINITION_DATA_SIZE];
        bytes[0..32].copy_from_slice(name);
        bytes.into()
    }

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

        let mut name: [u8;32] = [0;32];
        name.copy_from_slice(&data[0..32]);

        let mut pool_ids = Vec::<AccountId>::new();

        for i in 1..size_of_pool+1 {
            pool_ids.push(
                AccountId::new(data[i*32..(i+1)*32].try_into().expect("Parse data: The AMM program must be provided a valid AccountIds"))
            );
        }

        for _ in size_of_pool..MAX_NUMBER_POOLS {
            pool_ids.push( AccountId::default() );
        }

        Some( Self{
            name,
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
    fees: u128,
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
        bytes[208..224].copy_from_slice(&self.fees.to_le_bytes());
        bytes[224] = self.active as u8;
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
            let fees = u128::from_le_bytes(data[208..224].try_into().expect("Parse data: The AMM program must be provided a valid u128 for fees"));

            let active = match data[224] {
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

            let (post_states, chained_call) = new_pool(&pre_states,
                &[balance_a, balance_b],
                token_program_id
                );

            write_nssa_outputs_with_chained_call(pre_states, post_states, chained_call);
        }
        1 => {
            let mut token_addr: [u8;32] = [0;32];
            token_addr[0..].copy_from_slice(&instruction[33..65]);
            
            let token_addr = AccountId::new(token_addr);
            
            let amount_in = u128::from_le_bytes(instruction[1..17].try_into().expect("Swap: AMM Program expects valid u128 for balance to move"));
            let min_amount_out = u128::from_le_bytes(instruction[17..33].try_into().expect("Swap: AMM Program expects valid u128 for balance to move"));

            let (post_states, chained_call) = swap(&pre_states, &[amount_in, min_amount_out], token_addr);

            write_nssa_outputs_with_chained_call(pre_states, post_states, chained_call);
        }
        2 => {
            let min_amount_lp = u128::from_le_bytes(instruction[1..17].try_into().expect("Add liquidity: AMM Program expects valid u128 for min amount lp")); 
            let max_amount_a = u128::from_le_bytes(instruction[17..33].try_into().expect("Add liquidity: AMM Program expects valid u128 for max amount a"));
            let max_amount_b = u128::from_le_bytes(instruction[33..49].try_into().expect("Add liquidity: AMM Program expects valid u128 for max amount b"));
            
            let (post_states, chained_call) = add_liquidity(&pre_states,
                        &[min_amount_lp, max_amount_a, max_amount_b]);
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

//TODO: test
//add access to
fn new_definition (
        pre_states: &[AccountWithMetadata],
        name: &[u8;32],
    ) -> Vec<Account> {

    if pre_states.len() != 1 {
        panic!("Invalid number of input accounts");
    }

    let mut new_amm_post = pre_states[0].account.clone();

    new_amm_post.data = AMMDefinition::new(name);

    vec![new_amm_post]
}

//TODO: fix this
fn new_pool (
        pre_states: &[AccountWithMetadata],
        balance_in: &[u128],
        token_program: ProgramId,
    ) -> (Vec<Account>, Vec<ChainedCall>) {



    //Pool accounts: pool itself, and its 2 vaults and LP token
    //2 accounts for funding tokens
    //initial funder's LP account
    //TODO: update this test
    if pre_states.len() != 8 {
        panic!("Invalid number of input accounts")
    }

    if balance_in.len() != 2 {
        panic!("Invalid number of balance")
    }

    let amm = &pre_states[0];
    let pool = &pre_states[1];
    let vault_a = &pre_states[2];
    let vault_b = &pre_states[3];
    let pool_lp = &pre_states[4];
    let user_holding_a = &pre_states[5];
    let user_holding_b = &pre_states[6];
    let user_holding_lp = &pre_states[7];

    if amm.account == Account::default() {
        panic!("AMM is not initialized");
    }

    //TODO: ignore inactive for now.
    if !pool.is_authorized {
        panic!("Pool account is not authorized");
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

    let amm_data = AMMDefinition::parse(&amm.account.data).expect("AMM program expects a valid AMM account definition");
/*
    for i in 0..MAX_NUMBER_POOLS {
        if( 
            amm_d
        )
    }
*/
//pool data


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
            fees: 0u128, //TODO: we assume all fees are 0 for now.
            active: true, 
    };

    pool_post.data = pool_post_definition.into_data();

    let mut chained_call = Vec::new();
   
    //Chain call for Token A (user_holding_a -> Vault_A)
    let mut instruction: [u8;23] = [0; 23];
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
        amounts: &[u128],
        token_id: AccountId,
    ) -> (Vec<Account>, Vec<ChainedCall>) {

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

    if vault_a.account_id != pool_def_data.vault_a_addr {  
        panic!("Vault A was not provided");
    }
        
    if vault_b.account_id != pool_def_data.vault_b_addr {
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

    let (chained_call, [deposit_a, withdraw_a], [deposit_b, withdraw_b])
    = if token_id == pool_def_data.definition_token_a_id {
        let (chained_call, withdraw_b) = swap_logic(&[user_holding_a.clone(), vault_a.clone(), vault_b.clone(), user_holding_b.clone()],
                    amount_in,
                    &[pool_def_data.reserve_a, pool_def_data.reserve_b],
                    min_amount_out);
                
        (chained_call, [amount_in, 0], [0, withdraw_b])
    } else if token_id == pool_def_data.definition_token_b_id {
        let (chained_call, withdraw_a) = swap_logic(&[user_holding_b.clone(), vault_b.clone(), vault_a.clone(), user_holding_a.clone()],
                        amount_in,
                        &[pool_def_data.reserve_b, pool_def_data.reserve_a],
                        min_amount_out);

        (chained_call, [0, withdraw_a], [amount_in, 0])
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
            fees: 0u128,
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
    min_amount_out: u128,
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
    if min_amount_out > withdraw_amount {
        panic!("Withdraw amount is less than minimal amount out");
    }

    if withdraw_amount == 0 {
        panic!("Withdraw amount should be nonzero");
    }

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
    balances: &[u128]) -> (Vec<Account>, Vec<ChainedCall>) {

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
    if vault_a.account_id != pool_def_data.vault_a_addr {
        panic!("Vault A was not provided");
    }

    // TODO: need to check this one
    if pool_def_data.liquidity_pool_id != pool_definition_lp.account_id {
        panic!("LP definition mismatch");
    }

    if vault_b.account_id != pool_def_data.vault_b_addr {
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
            vault_a_addr: pool_def_data.vault_a_addr.clone(),
            vault_b_addr: pool_def_data.vault_b_addr.clone(),
            liquidity_pool_id: pool_def_data.liquidity_pool_id.clone(),
            liquidity_pool_supply: pool_def_data.liquidity_pool_supply + delta_lp,
            reserve_a: pool_def_data.reserve_a + actual_amount_a,
            reserve_b: pool_def_data.reserve_b + actual_amount_b,
            fees: 0u128,
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

    // Chain call for LP (mint new tokens for user_holding_lp)   
    let mut instruction_data = [0; 23];
    instruction_data[0] = 4;
    instruction_data[1..17].copy_from_slice(&delta_lp.to_le_bytes());
    let instruction_data = risc0_zkvm::serde::to_vec(&instruction_data).expect("Add liquidity: AMM Program expects valid token transfer instruction data");
    let call_token_lp = ChainedCall{
            program_id: pool_definition_lp.account.program_owner,
            instruction_data: instruction_data,
            pre_states: vec![pool_definition_lp.clone(), user_holding_lp.clone()]
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

    // TODO: need to check this one
    if pool_def_data.liquidity_pool_id != pool_definition_lp.account_id {
        panic!("LP definition mismatch");
    }


    if vault_a.account_id != pool_def_data.vault_a_addr {
        panic!("Vault A was not provided");
    }

    if vault_b.account_id != pool_def_data.vault_b_addr {
        panic!("Vault B was not provided");
    }
    
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
            vault_a_addr: pool_def_data.vault_a_addr.clone(),
            vault_b_addr: pool_def_data.vault_b_addr.clone(),
            liquidity_pool_id: pool_def_data.liquidity_pool_id.clone(),
            liquidity_pool_supply: pool_def_data.liquidity_pool_supply - delta_lp,
            reserve_a: pool_def_data.reserve_a - withdraw_amount_a,
            reserve_b: pool_def_data.reserve_b - withdraw_amount_b,
            fees: 0u128,
            active,  
    };

    pool_post.data = pool_post_definition.into_data();

    let mut chained_call = Vec::new();

    //Chaincall for Token A withdraw
    let mut instruction: [u8;23] = [0; 23];
    instruction[0] = 1;      
    instruction[1..17].copy_from_slice(&withdraw_amount_a.to_le_bytes());
    let instruction_data = risc0_zkvm::serde::to_vec(&instruction).expect("Remove liquidity: AMM Program expects valid token transfer instruction data");
    let call_token_a = ChainedCall{
            program_id: vault_a.account.program_owner,
            instruction_data: instruction_data,
            pre_states: vec![vault_a.clone(), user_holding_a.clone()]
        };

    //Chaincall for Token B withdraw
    let mut instruction: [u8;23] = [0; 23];
    instruction[0] = 1;      
    instruction[1..17].copy_from_slice(&withdraw_amount_b.to_le_bytes());
    let instruction_data = risc0_zkvm::serde::to_vec(&instruction).expect("Remove liquidity: AMM Program expects valid token transfer instruction data");
    let call_token_b = ChainedCall{
            program_id: vault_b.account.program_owner,
            instruction_data: instruction_data,
            pre_states: vec![vault_b.clone(), user_holding_b.clone()]
        };

    //Chaincall for LP adjustment        
    let mut instruction: [u8;23] = [0; 23];
    instruction[0] = 3;      
    instruction[1..17].copy_from_slice(&delta_lp.to_le_bytes());
    let instruction_data = risc0_zkvm::serde::to_vec(&instruction).expect("Remove liquidity: AMM Program expects valid token transfer instruction data");
    let call_token_lp = ChainedCall{
            program_id: pool_definition_lp.account.program_owner,
            instruction_data: instruction_data,
            pre_states: vec![pool_definition_lp.clone(), user_holding_lp.clone()]
        };

    chained_call.push(call_token_lp);
    chained_call.push(call_token_b);
    chained_call.push(call_token_a);
        
    let post_states = vec!
        [
        pool_post.clone(), 
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
    use nssa_core::{{account::{Account, AccountId, AccountWithMetadata}, program::ChainedCall}, program::ProgramId};

    use crate::{PoolDefinition, TokenDefinition, TokenHolding, add_liquidity, new_pool, remove_liquidity, swap};

    const TOKEN_PROGRAM_ID: ProgramId = [15;8];

    enum AccountEnum {
        user_holding_b,
        user_holding_a,
        vault_a_uninit,
        vault_b_uninit,
        vault_a_init,
        vault_b_init,
        vault_a_init_high,
        vault_b_init_high,
        vault_a_init_low,
        vault_b_init_low,
        vault_a_init_zero,
        vault_b_init_zero,
        vault_a_wrong_acc_id,
        vault_b_wrong_acc_id,
        pool_lp_uninit,
        pool_lp_init,
        pool_lp_wrong_acc_id, //TODO use?
        user_holding_lp_uninit,
        user_holding_lp_init,
        pool_definition_uninit,
        pool_definition_init,
        pool_definition_init_reserve_a_zero,
        pool_definition_init_reserve_b_zero,
        pool_definition_init_reserve_a_low,
        pool_definition_init_reserve_b_low,
        pool_definition_unauth,
        pool_definition_swap_test_1,
        pool_definition_swap_test_2,
        pool_definition_add_zero_lp,
        pool_definition_add_successful,
        pool_definition_remove_successful,
    }

    enum BalanceEnum {
        vault_a_reserve_init,
        vault_b_reserve_init,
        vault_a_reserve_low,
        vault_b_reserve_low,
        vault_a_reserve_high,
        vault_b_reserve_high,
        user_token_a_bal,
        user_token_b_bal,
        user_token_lp_bal,
        remove_min_amount_a,
        remove_min_amount_b,
        remove_actual_a_successful,
        remove_min_amount_b_low,
        remove_min_amount_a_low, //TODO use?
        remove_amount_lp,
        remove_amount_lp_1,
        add_max_amount_a_low,
        add_max_amount_b_low,
        add_max_amount_b_high, //TODO use?
        add_max_amount_a,
        add_max_amount_b,
        add_min_amount_lp,
        vault_a_swap_test_1,
        vault_a_swap_test_2,
        vault_b_swap_test_1,
        vault_b_swap_test_2,
        min_amount_out,
        vault_a_add_successful,
        vault_b_add_successful,
        add_successful_amount_a_lp,
        add_successful_amount_b,
        vault_a_remove_successful,
        vault_b_remove_successful,
    }

    fn helper_balance_constructor(selection: BalanceEnum) -> u128 {
        match selection {
            BalanceEnum::vault_a_reserve_init => 1_000,
            BalanceEnum::vault_b_reserve_init => 500,
            BalanceEnum::vault_a_reserve_low => 10,
            BalanceEnum::vault_b_reserve_low => 10,
            BalanceEnum::vault_a_reserve_high => 500_000,
            BalanceEnum::vault_b_reserve_high => 500_000,
            BalanceEnum::user_token_a_bal => 1_000,
            BalanceEnum::user_token_b_bal => 500,
            BalanceEnum::user_token_lp_bal => 100,
            BalanceEnum::remove_min_amount_a => 50,
            BalanceEnum::remove_min_amount_b => 100,
            BalanceEnum::remove_actual_a_successful => 100,
            BalanceEnum::remove_min_amount_b_low => 50,
            BalanceEnum::remove_min_amount_a_low => 10,
            BalanceEnum::remove_amount_lp => 100,
            BalanceEnum::remove_amount_lp_1 => 30,
            BalanceEnum::add_max_amount_a => 500,
            BalanceEnum::add_max_amount_b => 200,
            BalanceEnum::add_max_amount_b_high => 20_000,
            BalanceEnum::add_max_amount_a_low => 10,
            BalanceEnum::add_max_amount_b_low => 10,
            BalanceEnum::add_min_amount_lp => 20,
            BalanceEnum::vault_a_swap_test_1 => 1_500,
            BalanceEnum::vault_a_swap_test_2 => 715,
            BalanceEnum::vault_b_swap_test_1 => 334,
            BalanceEnum::vault_b_swap_test_2 => 700,
            BalanceEnum::min_amount_out => 200,
            BalanceEnum::vault_a_add_successful => 1_400,
            BalanceEnum::vault_b_add_successful => 700,
            BalanceEnum::add_successful_amount_a_lp => 400,
            BalanceEnum::add_successful_amount_b => 200,
            BalanceEnum::vault_a_remove_successful => 900,
            BalanceEnum::vault_b_remove_successful => 450,
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

    enum ChainedCallsEnum {
        cc_token_a_initialization,
        cc_token_b_initialization,
        cc_pool_lp_initialization,
        cc_swap_token_a_test_1,
        cc_swap_token_b_test_1,
        cc_swap_token_a_test_2,
        cc_swap_token_b_test_2,
        cc_add_token_a,
        cc_add_token_b,
        cc_add_pool_lp,
        cc_remove_token_a,
        cc_remove_token_b,
        cc_remove_pool_lp,
    }

    fn helper_chained_call_constructor(selection: ChainedCallsEnum) -> ChainedCall {
        match selection {
            ChainedCallsEnum::cc_token_a_initialization => {
                let mut instruction: [u8;23] = [0; 23];
                instruction[0] = 1;      
                instruction[1..17].copy_from_slice(
                    &helper_balance_constructor(BalanceEnum::user_token_a_bal)
                    .to_le_bytes());
                let instruction_data = risc0_zkvm::serde::to_vec(&instruction).expect("AMM Program expects valid transaction instruction data");
                ChainedCall{
                    program_id: TOKEN_PROGRAM_ID,
                    instruction_data,
                    pre_states: vec![
                            helper_account_constructor(AccountEnum::user_holding_a),
                            helper_account_constructor(AccountEnum::vault_a_uninit)],
                }
            }
            ChainedCallsEnum::cc_token_b_initialization => {
                let mut instruction: [u8;23] = [0; 23];
                instruction[0] = 1;      
                instruction[1..17].copy_from_slice(
                    &helper_balance_constructor(BalanceEnum::user_token_b_bal)
                    .to_le_bytes());
                let instruction_data = risc0_zkvm::serde::to_vec(&instruction).expect("AMM Program expects valid transaction instruction data");
                ChainedCall{
                    program_id: TOKEN_PROGRAM_ID,
                    instruction_data,
                    pre_states: vec![
                            helper_account_constructor(AccountEnum::user_holding_b),
                            helper_account_constructor(AccountEnum::vault_b_uninit)],
                }
            }
            ChainedCallsEnum::cc_pool_lp_initialization => {
                let mut instruction: [u8;23] = [0; 23];
                instruction[0] = 1;      
                instruction[1..17].copy_from_slice(
                    &helper_balance_constructor(BalanceEnum::user_token_a_bal)
                    .to_le_bytes());
                let instruction_data = risc0_zkvm::serde::to_vec(&instruction).expect("AMM Program expects valid transaction instruction data");
                ChainedCall{
                    program_id: TOKEN_PROGRAM_ID,
                    instruction_data,
                    pre_states: vec![
                            helper_account_constructor(AccountEnum::pool_lp_uninit),
                            helper_account_constructor(AccountEnum::user_holding_lp_uninit)],
                }
            }
            ChainedCallsEnum::cc_swap_token_a_test_1 => {
                let mut instruction_data: [u8;23] = [0; 23];
                instruction_data[0] = 1;      
                instruction_data[1..17].copy_from_slice(
                    &helper_balance_constructor(BalanceEnum::add_max_amount_a)
                    .to_le_bytes());
                let instruction_data = risc0_zkvm::serde::to_vec(&instruction_data).expect("AMM Program expects valid transaction instruction data");
                ChainedCall{
                    program_id: TOKEN_PROGRAM_ID,
                    instruction_data,
                    pre_states: vec![
                            helper_account_constructor(AccountEnum::user_holding_a),
                            helper_account_constructor(AccountEnum::vault_a_init)],
                }
            }
            ChainedCallsEnum::cc_swap_token_b_test_1 => {
                let swap_amount: u128 = 166;
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
                            helper_account_constructor(AccountEnum::vault_b_init),
                            helper_account_constructor(AccountEnum::user_holding_b)],
                }
            }
            ChainedCallsEnum::cc_swap_token_a_test_2 => {
                let swap_amount: u128 = 285;
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
                            helper_account_constructor(AccountEnum::vault_a_init),
                            helper_account_constructor(AccountEnum::user_holding_a)],
                }
            }
            ChainedCallsEnum::cc_swap_token_b_test_2 => {
                let mut instruction: [u8;23] = [0; 23];
                instruction[0] = 1;      
                instruction[1..17].copy_from_slice(
                    &helper_balance_constructor(BalanceEnum::add_max_amount_b)
                    .to_le_bytes());
                let instruction_data = risc0_zkvm::serde::to_vec(&instruction).expect("AMM Program expects valid transaction instruction data");
                ChainedCall{
                    program_id: TOKEN_PROGRAM_ID,
                    instruction_data,
                    pre_states: vec![
                            helper_account_constructor(AccountEnum::user_holding_b),
                            helper_account_constructor(AccountEnum::vault_b_init)],
                }
            }
            ChainedCallsEnum::cc_add_token_a => {
                let mut instruction: [u8;23] = [0; 23];
                instruction[0] = 1;      
                instruction[1..17].copy_from_slice(
                    &helper_balance_constructor(BalanceEnum::add_successful_amount_a_lp)
                    .to_le_bytes());
                let instruction_data = risc0_zkvm::serde::to_vec(&instruction).expect("AMM Program expects valid transaction instruction data");
                ChainedCall{
                    program_id: TOKEN_PROGRAM_ID,
                    instruction_data,
                    pre_states: vec![
                            helper_account_constructor(AccountEnum::user_holding_a),
                            helper_account_constructor(AccountEnum::vault_a_init)],
                }
            }
            ChainedCallsEnum::cc_add_token_b => {
                let mut instruction: [u8;23] = [0; 23];
                instruction[0] = 1;      
                instruction[1..17].copy_from_slice(
                    &helper_balance_constructor(BalanceEnum::add_successful_amount_b)
                    .to_le_bytes());
                let instruction_data = risc0_zkvm::serde::to_vec(&instruction).expect("Swap Logic: AMM Program expects valid transaction instruction data");
                ChainedCall{
                    program_id: TOKEN_PROGRAM_ID,
                    instruction_data,
                    pre_states: vec![
                            helper_account_constructor(AccountEnum::user_holding_b),
                            helper_account_constructor(AccountEnum::vault_b_init)],
                }
            }
            ChainedCallsEnum::cc_add_pool_lp => {
                let mut instruction: [u8;23] = [0; 23];
                instruction[0] = 4;      
                instruction[1..17].copy_from_slice(
                    &helper_balance_constructor(BalanceEnum::add_successful_amount_a_lp)
                    .to_le_bytes());
                let instruction_data = risc0_zkvm::serde::to_vec(&instruction).expect("Swap Logic: AMM Program expects valid transaction instruction data");
                ChainedCall{
                    program_id: TOKEN_PROGRAM_ID,
                    instruction_data,
                    pre_states: vec![
                            helper_account_constructor(AccountEnum::pool_lp_init),
                            helper_account_constructor(AccountEnum::user_holding_lp_init)],
                }
            }
            ChainedCallsEnum::cc_remove_token_a => {
                let mut instruction: [u8;23] = [0; 23];
                instruction[0] = 1;      
                instruction[1..17].copy_from_slice(
                    &helper_balance_constructor(BalanceEnum::remove_actual_a_successful)
                    .to_le_bytes());
                let instruction_data = risc0_zkvm::serde::to_vec(&instruction).expect("AMM Program expects valid transaction instruction data");
                ChainedCall{
                    program_id: TOKEN_PROGRAM_ID,
                    instruction_data,
                    pre_states: vec![
                            helper_account_constructor(AccountEnum::vault_a_init),
                            helper_account_constructor(AccountEnum::user_holding_a),],
                }
            }
            ChainedCallsEnum::cc_remove_token_b => {
                let mut instruction: [u8;23] = [0; 23];
                instruction[0] = 1;      
                instruction[1..17].copy_from_slice(
                    &helper_balance_constructor(BalanceEnum::remove_min_amount_b_low)
                    .to_le_bytes());
                let instruction_data = risc0_zkvm::serde::to_vec(&instruction).expect("AMM Program expects valid transaction instruction data");
                ChainedCall{
                    program_id: TOKEN_PROGRAM_ID,
                    instruction_data,
                    pre_states: vec![
                            helper_account_constructor(AccountEnum::vault_b_init),
                            helper_account_constructor(AccountEnum::user_holding_b),],
                }
            }
            ChainedCallsEnum::cc_remove_pool_lp => {
                let mut instruction: [u8;23] = [0; 23];
                instruction[0] = 3;      
                instruction[1..17].copy_from_slice(
                    &helper_balance_constructor(BalanceEnum::remove_actual_a_successful)
                    .to_le_bytes());
                let instruction_data = risc0_zkvm::serde::to_vec(&instruction).expect("AMM Program expects valid transaction instruction data");
                ChainedCall{
                    program_id: TOKEN_PROGRAM_ID,
                    instruction_data,
                    pre_states: vec![
                            helper_account_constructor(AccountEnum::user_holding_lp_init),
                            helper_account_constructor(AccountEnum::pool_lp_init),],
                }
            }

           _ => panic!("Invalid selection")
        }
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
            AccountEnum::vault_a_init_high => AccountWithMetadata {
                account: Account {
                    program_owner:  TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(
                        TokenHolding{
                            account_type: 1u8,
                            definition_id: helper_id_constructor(IdEnum::token_a_definition_id),
                            balance: helper_balance_constructor(BalanceEnum::vault_a_reserve_high),
                        }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::vault_a_id),
            },
            AccountEnum::vault_b_init_high => AccountWithMetadata {
                account: Account {
                    program_owner:  TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(
                        TokenHolding{
                            account_type: 1u8,
                            definition_id: helper_id_constructor(IdEnum::token_b_definition_id),
                            balance: helper_balance_constructor(BalanceEnum::vault_b_reserve_high),
                        }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::vault_b_id),
            },
            AccountEnum::vault_a_init_low => AccountWithMetadata {
                account: Account {
                    program_owner:  TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(
                        TokenHolding{
                            account_type: 1u8,
                            definition_id: helper_id_constructor(IdEnum::token_a_definition_id),
                            balance: helper_balance_constructor(BalanceEnum::vault_a_reserve_low),
                        }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::vault_a_id),
            },
            AccountEnum::vault_b_init_low => AccountWithMetadata {
                account: Account {
                    program_owner:  TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(
                        TokenHolding{
                            account_type: 1u8,
                            definition_id: helper_id_constructor(IdEnum::token_b_definition_id),
                            balance: helper_balance_constructor(BalanceEnum::vault_b_reserve_low),
                        }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::vault_b_id),
            },
            AccountEnum::vault_a_init_zero => AccountWithMetadata {
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
            AccountEnum::vault_b_init_zero => AccountWithMetadata {
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
                    data: TokenDefinition::into_data(
                        TokenDefinition{
                            account_type: 0u8,
                            name: [1;6],
                            total_supply: 0u128,
                        }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::token_lp_definition_id),
            },
            AccountEnum::pool_lp_init => AccountWithMetadata {
                account: Account {
                    program_owner:  TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenDefinition::into_data(
                        TokenDefinition{
                            account_type: 0u8,
                            name: [1;6],
                            total_supply: helper_balance_constructor(BalanceEnum::vault_a_reserve_init),
                        }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::token_lp_definition_id),
            },
            AccountEnum::pool_lp_wrong_acc_id => AccountWithMetadata {
              account: Account {
                    program_owner:  TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenDefinition::into_data(
                        TokenDefinition{
                            account_type: 0u8,
                            name: [1;6],
                            total_supply: helper_balance_constructor(BalanceEnum::vault_a_reserve_init),
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
                        program_owner:  ProgramId::default(),
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
                            fees: 0u128,
                            active: true,
                        }),
                        nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::pool_definition_id),
            },
            AccountEnum::pool_definition_init_reserve_a_zero => AccountWithMetadata {
                account: Account {
                        program_owner:  ProgramId::default(),
                        balance: 0u128,
                        data: PoolDefinition::into_data(
                        PoolDefinition {
                            definition_token_a_id: helper_id_constructor(IdEnum::token_a_definition_id),
                            definition_token_b_id: helper_id_constructor(IdEnum::token_b_definition_id),
                            vault_a_addr: helper_id_constructor(IdEnum::vault_a_id),
                            vault_b_addr: helper_id_constructor(IdEnum::vault_b_id),
                            liquidity_pool_id: helper_id_constructor(IdEnum::token_lp_definition_id),
                            liquidity_pool_supply: helper_balance_constructor(BalanceEnum::vault_a_reserve_init),
                            reserve_a: 0,
                            reserve_b: helper_balance_constructor(BalanceEnum::vault_b_reserve_init),
                            fees: 0u128,
                            active: true,
                        }),
                        nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::pool_definition_id),
            },
            AccountEnum::pool_definition_init_reserve_b_zero => AccountWithMetadata {
                account: Account {
                        program_owner:  ProgramId::default(),
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
                            reserve_b: 0,
                            fees: 0u128,
                            active: true,
                        }),
                        nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::pool_definition_id),
            },
            AccountEnum::pool_definition_init_reserve_a_low => AccountWithMetadata {
                account: Account {
                        program_owner:  ProgramId::default(),
                        balance: 0u128,
                        data: PoolDefinition::into_data(
                        PoolDefinition {
                            definition_token_a_id: helper_id_constructor(IdEnum::token_a_definition_id),
                            definition_token_b_id: helper_id_constructor(IdEnum::token_b_definition_id),
                            vault_a_addr: helper_id_constructor(IdEnum::vault_a_id),
                            vault_b_addr: helper_id_constructor(IdEnum::vault_b_id),
                            liquidity_pool_id: helper_id_constructor(IdEnum::token_lp_definition_id),
                            liquidity_pool_supply: helper_balance_constructor(BalanceEnum::vault_a_reserve_low),
                            reserve_a: helper_balance_constructor(BalanceEnum::vault_a_reserve_low),
                            reserve_b: helper_balance_constructor(BalanceEnum::vault_b_reserve_high),
                            fees: 0u128,
                            active: true,
                        }),
                        nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::pool_definition_id),
            },
            AccountEnum::pool_definition_init_reserve_b_low => AccountWithMetadata {
                account: Account {
                        program_owner:  ProgramId::default(),
                        balance: 0u128,
                        data: PoolDefinition::into_data(
                        PoolDefinition {
                            definition_token_a_id: helper_id_constructor(IdEnum::token_a_definition_id),
                            definition_token_b_id: helper_id_constructor(IdEnum::token_b_definition_id),
                            vault_a_addr: helper_id_constructor(IdEnum::vault_a_id),
                            vault_b_addr: helper_id_constructor(IdEnum::vault_b_id),
                            liquidity_pool_id: helper_id_constructor(IdEnum::token_lp_definition_id),
                            liquidity_pool_supply: helper_balance_constructor(BalanceEnum::vault_a_reserve_high),
                            reserve_a: helper_balance_constructor(BalanceEnum::vault_a_reserve_high),
                            reserve_b: helper_balance_constructor(BalanceEnum::vault_b_reserve_low),
                            fees: 0u128,
                            active: true,
                        }),
                        nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::pool_definition_id),
            },
            AccountEnum::pool_definition_unauth => AccountWithMetadata {
                account: Account {
                        program_owner:  ProgramId::default(),
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
                            fees: 0u128,
                            active: true,
                        }),
                        nonce: 0,
                },
                is_authorized: false,
                account_id: helper_id_constructor(IdEnum::pool_definition_id),
            },
            AccountEnum::pool_definition_swap_test_1 => AccountWithMetadata {
                account: Account {
                        program_owner:  ProgramId::default(),
                        balance: 0u128,
                        data: PoolDefinition::into_data(
                        PoolDefinition {
                            definition_token_a_id: helper_id_constructor(IdEnum::token_a_definition_id),
                            definition_token_b_id: helper_id_constructor(IdEnum::token_b_definition_id),
                            vault_a_addr: helper_id_constructor(IdEnum::vault_a_id),
                            vault_b_addr: helper_id_constructor(IdEnum::vault_b_id),
                            liquidity_pool_id: helper_id_constructor(IdEnum::token_lp_definition_id),
                            liquidity_pool_supply: helper_balance_constructor(BalanceEnum::vault_a_reserve_init),
                            reserve_a: helper_balance_constructor(BalanceEnum::vault_a_swap_test_1),
                            reserve_b: helper_balance_constructor(BalanceEnum::vault_b_swap_test_1),
                            fees: 0u128,
                            active: true,
                        }),
                        nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::pool_definition_id),
            },
            AccountEnum::pool_definition_swap_test_2 => AccountWithMetadata {
                account: Account {
                        program_owner:  ProgramId::default(),
                        balance: 0u128,
                        data: PoolDefinition::into_data(
                        PoolDefinition {
                            definition_token_a_id: helper_id_constructor(IdEnum::token_a_definition_id),
                            definition_token_b_id: helper_id_constructor(IdEnum::token_b_definition_id),
                            vault_a_addr: helper_id_constructor(IdEnum::vault_a_id),
                            vault_b_addr: helper_id_constructor(IdEnum::vault_b_id),
                            liquidity_pool_id: helper_id_constructor(IdEnum::token_lp_definition_id),
                            liquidity_pool_supply: helper_balance_constructor(BalanceEnum::vault_a_reserve_init),
                            reserve_a: helper_balance_constructor(BalanceEnum::vault_a_swap_test_2),
                            reserve_b: helper_balance_constructor(BalanceEnum::vault_b_swap_test_2),
                            fees: 0u128,
                            active: true,
                        }),
                        nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::pool_definition_id),
            },
            AccountEnum::pool_definition_add_zero_lp => AccountWithMetadata {
                account: Account {
                        program_owner:  ProgramId::default(),
                        balance: 0u128,
                        data: PoolDefinition::into_data(
                        PoolDefinition {
                            definition_token_a_id: helper_id_constructor(IdEnum::token_a_definition_id),
                            definition_token_b_id: helper_id_constructor(IdEnum::token_b_definition_id),
                            vault_a_addr: helper_id_constructor(IdEnum::vault_a_id),
                            vault_b_addr: helper_id_constructor(IdEnum::vault_b_id),
                            liquidity_pool_id: helper_id_constructor(IdEnum::token_lp_definition_id),
                            liquidity_pool_supply: helper_balance_constructor(BalanceEnum::vault_a_reserve_low),
                            reserve_a: helper_balance_constructor(BalanceEnum::vault_a_reserve_init),
                            reserve_b: helper_balance_constructor(BalanceEnum::vault_b_reserve_init),
                            fees: 0u128,
                            active: true,
                        }),
                        nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::pool_definition_id),
            },
            AccountEnum::pool_definition_add_successful => AccountWithMetadata {
                account: Account {
                        program_owner:  ProgramId::default(),
                        balance: 0u128,
                        data: PoolDefinition::into_data(
                        PoolDefinition {
                            definition_token_a_id: helper_id_constructor(IdEnum::token_a_definition_id),
                            definition_token_b_id: helper_id_constructor(IdEnum::token_b_definition_id),
                            vault_a_addr: helper_id_constructor(IdEnum::vault_a_id),
                            vault_b_addr: helper_id_constructor(IdEnum::vault_b_id),
                            liquidity_pool_id: helper_id_constructor(IdEnum::token_lp_definition_id),
                            liquidity_pool_supply: helper_balance_constructor(BalanceEnum::vault_a_add_successful),
                            reserve_a: helper_balance_constructor(BalanceEnum::vault_a_add_successful),
                            reserve_b: helper_balance_constructor(BalanceEnum::vault_b_add_successful),
                            fees: 0u128,
                            active: true,
                        }),
                        nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::pool_definition_id),
            },
            AccountEnum::pool_definition_remove_successful => AccountWithMetadata {
                account: Account {
                        program_owner:  ProgramId::default(),
                        balance: 0u128,
                        data: PoolDefinition::into_data(
                        PoolDefinition {
                            definition_token_a_id: helper_id_constructor(IdEnum::token_a_definition_id),
                            definition_token_b_id: helper_id_constructor(IdEnum::token_b_definition_id),
                            vault_a_addr: helper_id_constructor(IdEnum::vault_a_id),
                            vault_b_addr: helper_id_constructor(IdEnum::vault_b_id),
                            liquidity_pool_id: helper_id_constructor(IdEnum::token_lp_definition_id),
                            liquidity_pool_supply: helper_balance_constructor(BalanceEnum::vault_a_remove_successful),
                            reserve_a: helper_balance_constructor(BalanceEnum::vault_a_remove_successful),
                            reserve_b: helper_balance_constructor(BalanceEnum::vault_b_remove_successful),
                            fees: 0u128,
                            active: true,
                        }),
                        nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::pool_definition_id),
            },
            _ => panic!("Invalid selection"),
        }
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]    
    fn test_call_new_pool_with_invalid_number_of_accounts_1() {
        let pre_states = vec![ helper_account_constructor(AccountEnum::pool_definition_uninit),]
        ;
        let _post_states = new_pool(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::user_token_a_bal),
                    helper_balance_constructor(BalanceEnum::user_token_b_bal)],
                    TOKEN_PROGRAM_ID);
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_new_pool_with_invalid_number_of_accounts_2() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_uninit),
                helper_account_constructor(AccountEnum::vault_a_uninit),
                ];
        let _post_states = new_pool(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::user_token_a_bal),
                    helper_balance_constructor(BalanceEnum::user_token_b_bal)],
                    TOKEN_PROGRAM_ID);
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_new_pool_with_invalid_number_of_accounts_3() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_uninit),
                helper_account_constructor(AccountEnum::vault_a_uninit),
                helper_account_constructor(AccountEnum::vault_b_uninit),
                ];
        let _post_states = new_pool(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::user_token_a_bal),
                    helper_balance_constructor(BalanceEnum::user_token_b_bal)],
                    TOKEN_PROGRAM_ID);
   }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_new_pool_with_invalid_number_of_accounts_4() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_uninit),
                helper_account_constructor(AccountEnum::vault_a_uninit),
                helper_account_constructor(AccountEnum::vault_b_uninit),
                helper_account_constructor(AccountEnum::pool_lp_uninit),
                ];
        let _post_states = new_pool(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::user_token_a_bal),
                    helper_balance_constructor(BalanceEnum::user_token_b_bal)],
                    TOKEN_PROGRAM_ID);
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_new_pool_with_invalid_number_of_accounts_5() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_uninit),
                helper_account_constructor(AccountEnum::vault_a_uninit),
                helper_account_constructor(AccountEnum::vault_b_uninit),
                helper_account_constructor(AccountEnum::pool_lp_uninit),
                helper_account_constructor(AccountEnum::user_holding_a),
                ];
        let _post_states = new_pool(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::user_token_a_bal),
                    helper_balance_constructor(BalanceEnum::user_token_b_bal)],
                    TOKEN_PROGRAM_ID);
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_new_pool_with_invalid_number_of_accounts_6() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_uninit),
                helper_account_constructor(AccountEnum::vault_a_uninit),
                helper_account_constructor(AccountEnum::vault_b_uninit),
                helper_account_constructor(AccountEnum::pool_lp_uninit),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                ];
        let _post_states = new_pool(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::user_token_a_bal),
                    helper_balance_constructor(BalanceEnum::user_token_b_bal)],
                    TOKEN_PROGRAM_ID);
    }

    #[should_panic(expected = "Invalid number of balance")]
    #[test]
    fn test_call_new_pool_with_invalid_number_of_balances_1() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_uninit),
                helper_account_constructor(AccountEnum::vault_a_uninit),
                helper_account_constructor(AccountEnum::vault_b_uninit),
                helper_account_constructor(AccountEnum::pool_lp_uninit),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                helper_account_constructor(AccountEnum::user_holding_lp_uninit),
                ];
        let _post_states = new_pool(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::user_token_a_bal)],
                    TOKEN_PROGRAM_ID);
    }
    
    #[should_panic(expected = "Pool account is initiated or not authorized")]
    #[test]
    fn test_call_new_pool_with_initiated_pool() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_uninit),
                helper_account_constructor(AccountEnum::vault_b_uninit),
                helper_account_constructor(AccountEnum::pool_lp_uninit),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                helper_account_constructor(AccountEnum::user_holding_lp_uninit),
                ];
        let _post_states = new_pool(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::user_token_a_bal),
                    helper_balance_constructor(BalanceEnum::user_token_b_bal)],
                    TOKEN_PROGRAM_ID);
    }

    #[should_panic(expected = "Pool account is initiated or not authorized")]
    #[test]
    fn test_call_new_pool_with_unauthorized_pool() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_unauth),
                helper_account_constructor(AccountEnum::vault_a_uninit),
                helper_account_constructor(AccountEnum::vault_b_uninit),
                helper_account_constructor(AccountEnum::pool_lp_uninit),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                helper_account_constructor(AccountEnum::user_holding_lp_uninit),
                ];
        let _post_states = new_pool(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::user_token_a_bal),
                    helper_balance_constructor(BalanceEnum::user_token_b_bal)],
                    TOKEN_PROGRAM_ID);
    }
    
    #[should_panic(expected = "Balances must be nonzero")]
    #[test]
    fn test_call_new_pool_with_balance_zero_1() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_uninit),
                helper_account_constructor(AccountEnum::vault_a_uninit),
                helper_account_constructor(AccountEnum::vault_b_uninit),
                helper_account_constructor(AccountEnum::pool_lp_uninit),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                helper_account_constructor(AccountEnum::user_holding_lp_uninit),
                ];
        let _post_states = new_pool(&pre_states, 
                    &[0,
                    helper_balance_constructor(BalanceEnum::user_token_b_bal)],
                    TOKEN_PROGRAM_ID);
    }      

    #[should_panic(expected = "Balances must be nonzero")]
    #[test]
    fn test_call_new_pool_with_balance_zero_2() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_uninit),
                helper_account_constructor(AccountEnum::vault_a_uninit),
                helper_account_constructor(AccountEnum::vault_b_uninit),
                helper_account_constructor(AccountEnum::pool_lp_uninit),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                helper_account_constructor(AccountEnum::user_holding_lp_uninit),
                ];
        let _post_states = new_pool(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::user_token_a_bal),
                    0],
                    TOKEN_PROGRAM_ID);
    }
    
    #[should_panic(expected = "Cannot set up a swap for a token with itself.")]
    #[test]
    fn test_call_new_pool_same_token() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_uninit),
                helper_account_constructor(AccountEnum::vault_a_uninit),
                helper_account_constructor(AccountEnum::vault_b_uninit),
                helper_account_constructor(AccountEnum::pool_lp_uninit),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_lp_uninit),
                ];
        let _post_states = new_pool(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::user_token_a_bal), 
                    helper_balance_constructor(BalanceEnum::user_token_b_bal)],
                    TOKEN_PROGRAM_ID);
    }

    #[test]
    fn test_call_new_pool_chain_call_success() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_uninit),
                helper_account_constructor(AccountEnum::vault_a_uninit),
                helper_account_constructor(AccountEnum::vault_b_uninit),
                helper_account_constructor(AccountEnum::pool_lp_uninit),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                helper_account_constructor(AccountEnum::user_holding_lp_uninit),
                ];
        let (post_states, chained_calls) = new_pool(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::user_token_a_bal), 
                    helper_balance_constructor(BalanceEnum::user_token_b_bal)],
                    TOKEN_PROGRAM_ID);

        let pool_post = post_states[0].clone();

        let pool_data = PoolDefinition::parse(&pool_post.data).unwrap();
        assert!(helper_account_constructor(AccountEnum::pool_definition_init).account ==
                    pool_post);

        let chained_call_lp = chained_calls[0].clone();
        let chained_call_b = chained_calls[1].clone();
        let chained_call_a = chained_calls[2].clone();

        assert!(chained_call_lp == helper_chained_call_constructor(ChainedCallsEnum::cc_pool_lp_initialization));
        assert!(chained_call_a == helper_chained_call_constructor(ChainedCallsEnum::cc_token_a_initialization));
        assert!(chained_call_b == helper_chained_call_constructor(ChainedCallsEnum::cc_token_b_initialization));
    }

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

    #[should_panic(expected = "Invalid liquidity account provided")]
    #[test]
    fn test_call_remove_liquidity_insufficient_liquidity_amount() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_init),
                helper_account_constructor(AccountEnum::vault_b_init),
                helper_account_constructor(AccountEnum::pool_lp_init),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                helper_account_constructor(AccountEnum::user_holding_a), //different token account than lp to create desired error
                ];
        let _post_states = remove_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::remove_amount_lp), 
                    helper_balance_constructor(BalanceEnum::remove_min_amount_a),
                    helper_balance_constructor(BalanceEnum::remove_min_amount_b)],
                    );
    }

    #[should_panic(expected = "Insufficient minimal withdraw amount (Token A) provided for liquidity amount")]
    #[test]
    fn test_call_remove_liquidity_insufficient_balance_1() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_init),
                helper_account_constructor(AccountEnum::vault_b_init),
                helper_account_constructor(AccountEnum::pool_lp_init),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                helper_account_constructor(AccountEnum::user_holding_lp_init),
                ];
        let _post_states = remove_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::remove_amount_lp_1), 
                    helper_balance_constructor(BalanceEnum::remove_min_amount_a),
                    helper_balance_constructor(BalanceEnum::remove_min_amount_b)],
                    );
    }

    #[should_panic(expected = "Insufficient minimal withdraw amount (Token B) provided for liquidity amount")]
    #[test]
    fn test_call_remove_liquidity_insufficient_balance_2() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_init),
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

    #[should_panic(expected = "Minimum withdraw amount must be nonzero")]
    #[test]
    fn test_call_remove_liquidity_min_bal_zero_1() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_init),
                helper_account_constructor(AccountEnum::vault_b_init),
                helper_account_constructor(AccountEnum::pool_lp_init),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                helper_account_constructor(AccountEnum::user_holding_lp_init),
                ];
        let _post_states = remove_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::remove_amount_lp), 
                    0,
                    helper_balance_constructor(BalanceEnum::remove_min_amount_b)],
                    );
    }

    #[should_panic(expected = "Minimum withdraw amount must be nonzero")]
    #[test]
    fn test_call_remove_liquidity_min_bal_zero_2() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_init),
                helper_account_constructor(AccountEnum::vault_b_init),
                helper_account_constructor(AccountEnum::pool_lp_init),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                helper_account_constructor(AccountEnum::user_holding_lp_init),
                ];
        let _post_states = remove_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::remove_amount_lp), 
                    helper_balance_constructor(BalanceEnum::remove_min_amount_a),
                    0],
                    );
    }

    #[should_panic(expected = "Liquidity amount must be nonzero")]
    #[test]
    fn test_call_remove_liquidity_lp_bal_zero() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_init),
                helper_account_constructor(AccountEnum::vault_b_init),
                helper_account_constructor(AccountEnum::pool_lp_init),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                helper_account_constructor(AccountEnum::user_holding_lp_init),
                ];
        let _post_states = remove_liquidity(&pre_states, 
                    &[0, 
                    helper_balance_constructor(BalanceEnum::remove_min_amount_a),
                    helper_balance_constructor(BalanceEnum::remove_min_amount_b),],
                    );
    }    

    #[test]
    fn test_call_remove_liquidity_chained_call_successful() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_init),
                helper_account_constructor(AccountEnum::vault_b_init),
                helper_account_constructor(AccountEnum::pool_lp_init),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                helper_account_constructor(AccountEnum::user_holding_lp_init),
                ];
        let (post_states, chained_calls) = remove_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::remove_amount_lp), 
                    helper_balance_constructor(BalanceEnum::remove_min_amount_a),
                    helper_balance_constructor(BalanceEnum::remove_min_amount_b_low),],
                    );

        let pool_post = post_states[0].clone();

        assert!(helper_account_constructor(AccountEnum::pool_definition_remove_successful).account ==
                   pool_post);

        let chained_call_lp = chained_calls[0].clone();
        let chained_call_b = chained_calls[1].clone();           
        let chained_call_a = chained_calls[2].clone();     

        assert!(chained_call_a == helper_chained_call_constructor(ChainedCallsEnum::cc_remove_token_a));
        assert!(chained_call_b == helper_chained_call_constructor(ChainedCallsEnum::cc_remove_token_b));
        assert!(chained_call_lp.instruction_data == helper_chained_call_constructor(ChainedCallsEnum::cc_remove_pool_lp).instruction_data);
    }   

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]    
    fn test_call_add_liquidity_with_invalid_number_of_accounts_1() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                ];
        let _post_states = add_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::add_max_amount_a),
                    helper_balance_constructor(BalanceEnum::add_max_amount_b),
                    helper_balance_constructor(BalanceEnum::add_min_amount_lp),],
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
                    &[helper_balance_constructor(BalanceEnum::add_max_amount_a),
                    helper_balance_constructor(BalanceEnum::add_max_amount_b),
                    helper_balance_constructor(BalanceEnum::add_min_amount_lp),],
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
                    &[helper_balance_constructor(BalanceEnum::add_max_amount_a),
                    helper_balance_constructor(BalanceEnum::add_max_amount_b),
                    helper_balance_constructor(BalanceEnum::add_min_amount_lp),],
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
                    &[helper_balance_constructor(BalanceEnum::add_max_amount_a),
                    helper_balance_constructor(BalanceEnum::add_max_amount_b),
                    helper_balance_constructor(BalanceEnum::add_min_amount_lp),],
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
                    &[helper_balance_constructor(BalanceEnum::add_max_amount_a),
                    helper_balance_constructor(BalanceEnum::add_max_amount_b),
                    helper_balance_constructor(BalanceEnum::add_min_amount_lp),],
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
                    &[helper_balance_constructor(BalanceEnum::add_max_amount_a),
                    helper_balance_constructor(BalanceEnum::add_max_amount_b),
                    helper_balance_constructor(BalanceEnum::add_min_amount_lp),],
                    );
    }
  
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
                    &[helper_balance_constructor(BalanceEnum::add_max_amount_a),],
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
                    &[helper_balance_constructor(BalanceEnum::add_max_amount_a),
                    helper_balance_constructor(BalanceEnum::add_max_amount_b),
                    helper_balance_constructor(BalanceEnum::add_min_amount_lp),],
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
                    &[helper_balance_constructor(BalanceEnum::add_max_amount_a),
                    helper_balance_constructor(BalanceEnum::add_max_amount_b),
                    helper_balance_constructor(BalanceEnum::add_min_amount_lp),],
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
                    helper_balance_constructor(BalanceEnum::add_max_amount_b),
                    helper_balance_constructor(BalanceEnum::add_min_amount_lp),],
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
                    &[helper_balance_constructor(BalanceEnum::add_max_amount_a),
                    0,
                    helper_balance_constructor(BalanceEnum::add_min_amount_lp),],
                    );
    }

    #[should_panic(expected = "Min-lp must be nonzero")]
    #[test]
    fn test_call_add_liquidity_zero_min_lp() {
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
                    &[helper_balance_constructor(BalanceEnum::add_max_amount_a),
                    helper_balance_constructor(BalanceEnum::add_max_amount_b),
                    0],);
    }

    #[should_panic(expected = "Vaults' balances must be at least the reserve amounts")]
    #[test]
    fn test_call_add_liquidity_vault_insufficient_balance_1() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_init_zero),
                helper_account_constructor(AccountEnum::vault_b_init),
                helper_account_constructor(AccountEnum::pool_lp_init),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                helper_account_constructor(AccountEnum::user_holding_lp_init),
                ];
        let _post_states = add_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::add_max_amount_a), 
                    helper_balance_constructor(BalanceEnum::add_max_amount_b),
                    helper_balance_constructor(BalanceEnum::add_min_amount_lp),],
                    );
    }

    #[should_panic(expected = "Vaults' balances must be at least the reserve amounts")]
    #[test]
    fn test_call_add_liquidity_vault_insufficient_balance_2() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_init),
                helper_account_constructor(AccountEnum::vault_b_init_zero),
                helper_account_constructor(AccountEnum::pool_lp_init),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                helper_account_constructor(AccountEnum::user_holding_lp_init),
                ];
        let _post_states = add_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::add_max_amount_a), 
                    helper_balance_constructor(BalanceEnum::add_max_amount_b),
                    helper_balance_constructor(BalanceEnum::add_min_amount_lp),],
                    );
    }

    #[should_panic(expected = "A trade amount is 0")]
    #[test]
    fn test_call_add_liquidity_actual_amount_zero_1() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init_reserve_a_low),
                helper_account_constructor(AccountEnum::vault_a_init_low),
                helper_account_constructor(AccountEnum::vault_b_init_high),
                helper_account_constructor(AccountEnum::pool_lp_init),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                helper_account_constructor(AccountEnum::user_holding_lp_init),
                ];
        let _post_states = add_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::add_max_amount_a), 
                    helper_balance_constructor(BalanceEnum::add_max_amount_b),
                    helper_balance_constructor(BalanceEnum::add_min_amount_lp),],
                    );
    }

    #[should_panic(expected = "A trade amount is 0")]
    #[test]
    fn test_call_add_liquidity_actual_amount_zero_2() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init_reserve_b_low),
                helper_account_constructor(AccountEnum::vault_a_init_high),
                helper_account_constructor(AccountEnum::vault_b_init_low),
                helper_account_constructor(AccountEnum::pool_lp_init),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                helper_account_constructor(AccountEnum::user_holding_lp_init),
                ];
        let _post_states = add_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::add_max_amount_a_low), 
                    helper_balance_constructor(BalanceEnum::add_max_amount_b_low),
                    helper_balance_constructor(BalanceEnum::add_min_amount_lp),],
                    );
    }

    #[should_panic(expected = "Reserves must be nonzero")]
    #[test]
    fn test_call_add_liquidity_reserves_zero_1() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init_reserve_a_zero),
                helper_account_constructor(AccountEnum::vault_a_init),
                helper_account_constructor(AccountEnum::vault_b_init),
                helper_account_constructor(AccountEnum::pool_lp_init),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                helper_account_constructor(AccountEnum::user_holding_lp_init),
                ];
        let _post_states = add_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::add_max_amount_a),
                    helper_balance_constructor(BalanceEnum::add_max_amount_b),
                    helper_balance_constructor(BalanceEnum::add_min_amount_lp),],
                    );        
    }

    #[should_panic(expected = "Reserves must be nonzero")]
    #[test]
    fn test_call_add_liquidity_reserves_zero_2() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init_reserve_b_zero),
                helper_account_constructor(AccountEnum::vault_a_init),
                helper_account_constructor(AccountEnum::vault_b_init),
                helper_account_constructor(AccountEnum::pool_lp_init),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                helper_account_constructor(AccountEnum::user_holding_lp_init),
                ];
        let _post_states = add_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::add_max_amount_a),
                    helper_balance_constructor(BalanceEnum::add_max_amount_b),
                    helper_balance_constructor(BalanceEnum::add_min_amount_lp),],
                    );        
    }

    #[should_panic(expected = "Payable LP must be nonzero")]
    #[test]
    fn test_call_add_liquidity_payable_lp_zero() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_add_zero_lp),
                helper_account_constructor(AccountEnum::vault_a_init),
                helper_account_constructor(AccountEnum::vault_b_init),
                helper_account_constructor(AccountEnum::pool_lp_init),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                helper_account_constructor(AccountEnum::user_holding_lp_init),
                ];
        let _post_states = add_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::add_max_amount_a_low),
                    helper_balance_constructor(BalanceEnum::add_max_amount_b_low),
                    helper_balance_constructor(BalanceEnum::add_min_amount_lp),],
                    );
    }

    #[test]
    fn test_call_add_liquidity_successful_chain_call() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_init),
                helper_account_constructor(AccountEnum::vault_b_init),
                helper_account_constructor(AccountEnum::pool_lp_init),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                helper_account_constructor(AccountEnum::user_holding_lp_init),
                ];
        let (post_states, chained_calls) = add_liquidity(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::add_max_amount_a),
                    helper_balance_constructor(BalanceEnum::add_max_amount_b),
                    helper_balance_constructor(BalanceEnum::add_min_amount_lp),],
                    );
    
        let pool_post = post_states[0].clone();

        assert!(helper_account_constructor(AccountEnum::pool_definition_add_successful).account ==
                    pool_post);

        let chained_call_lp = chained_calls[0].clone();
        let chained_call_b = chained_calls[1].clone();
        let chained_call_a = chained_calls[2].clone();


        assert!(chained_call_a == helper_chained_call_constructor(ChainedCallsEnum::cc_add_token_a));
        assert!(chained_call_b == helper_chained_call_constructor(ChainedCallsEnum::cc_add_token_b));
        assert!(chained_call_lp == helper_chained_call_constructor(ChainedCallsEnum::cc_add_pool_lp));
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]    
    fn test_call_swap_with_invalid_number_of_accounts_1() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                ];
        let _post_states = swap(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::add_max_amount_a),
                    helper_balance_constructor(BalanceEnum::min_amount_out)],
                    helper_id_constructor(IdEnum::token_a_definition_id),
                    );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_swap_with_invalid_number_of_accounts_2() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_init),
                ];
        let _post_states = swap(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::add_max_amount_a),
                    helper_balance_constructor(BalanceEnum::min_amount_out)],
                    helper_id_constructor(IdEnum::token_a_definition_id),
                    );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_swap_with_invalid_number_of_accounts_3() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_init),
                helper_account_constructor(AccountEnum::vault_b_init),
                ];
        let _post_states = swap(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::add_max_amount_a),
                    helper_balance_constructor(BalanceEnum::min_amount_out)],
                    helper_id_constructor(IdEnum::token_a_definition_id),
                    );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_swap_with_invalid_number_of_accounts_4() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_init),
                helper_account_constructor(AccountEnum::vault_b_init),
                helper_account_constructor(AccountEnum::user_holding_a),
                ];
        let _post_states = swap(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::add_max_amount_a),
                    helper_balance_constructor(BalanceEnum::min_amount_out)],
                    helper_id_constructor(IdEnum::token_a_definition_id),
                    );
    }

    #[should_panic(expected = "Invalid number of amounts provided")]
    #[test]
    fn test_call_swap_with_invalid_number_of_amounts() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_init),
                helper_account_constructor(AccountEnum::vault_b_init),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                ];
        let _post_states = swap(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::add_max_amount_a)],
                    helper_id_constructor(IdEnum::token_lp_definition_id),
                    );
    }

    #[should_panic(expected = "AccountId is not a token type for the pool")]
    #[test]
    fn test_call_swap_incorrect_token_type() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_init),
                helper_account_constructor(AccountEnum::vault_b_init),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                ];
        let _post_states = swap(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::add_max_amount_a),
                    helper_balance_constructor(BalanceEnum::min_amount_out)],
                    helper_id_constructor(IdEnum::token_lp_definition_id),
                    );
    }

    #[should_panic(expected = "Vault A was not provided")]
    #[test]
    fn test_call_swap_vault_a_omitted() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_wrong_acc_id),
                helper_account_constructor(AccountEnum::vault_b_init),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                ];
        let _post_states = swap(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::add_max_amount_a),
                    helper_balance_constructor(BalanceEnum::min_amount_out)],
                    helper_id_constructor(IdEnum::token_a_definition_id),
                    );
    }

    #[should_panic(expected = "Vault B was not provided")]
    #[test]
    fn test_call_swap_vault_b_omitted() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_init),
                helper_account_constructor(AccountEnum::vault_b_wrong_acc_id),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                ];
        let _post_states = swap(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::add_max_amount_a),
                    helper_balance_constructor(BalanceEnum::min_amount_out)],
                    helper_id_constructor(IdEnum::token_a_definition_id),
                    );
    }

    #[should_panic(expected = "Reserve for Token A exceeds vault balance")]
    #[test]
    fn test_call_swap_reserves_vault_mismatch_1() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_init_low),
                helper_account_constructor(AccountEnum::vault_b_init),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                ];
        let _post_states = swap(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::add_max_amount_a),
                    helper_balance_constructor(BalanceEnum::min_amount_out)],
                    helper_id_constructor(IdEnum::token_a_definition_id),
                    );
    }

        #[should_panic(expected = "Reserve for Token B exceeds vault balance")]
    #[test]
    fn test_call_swap_reserves_vault_misatch_2() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_init),
                helper_account_constructor(AccountEnum::vault_b_init_low),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                ];
        let _post_states = swap(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::add_max_amount_a),
                    helper_balance_constructor(BalanceEnum::min_amount_out)],
                    helper_id_constructor(IdEnum::token_a_definition_id),
                    );
    }

    #[should_panic(expected = "Withdraw amount is less than minimal amount out")]
    #[test]
    fn test_call_swap_below_min_out() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_init),
                helper_account_constructor(AccountEnum::vault_b_init),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                ];
        let _post_states = swap(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::add_max_amount_a),
                    helper_balance_constructor(BalanceEnum::min_amount_out)],
                    helper_id_constructor(IdEnum::token_a_definition_id),
                    );
    }

    #[test]
    fn test_call_swap_successful_chain_call_1() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_init),
                helper_account_constructor(AccountEnum::vault_b_init),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                ];
        let (post_states, chained_calls) = swap(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::add_max_amount_a),
                    helper_balance_constructor(BalanceEnum::add_max_amount_a_low)],
                    helper_id_constructor(IdEnum::token_a_definition_id),
                    );
    
        let pool_post = post_states[0].clone();

        assert!(helper_account_constructor(AccountEnum::pool_definition_swap_test_1).account ==
                    pool_post);

        let chained_call_a = chained_calls[0].clone();            
        let chained_call_b = chained_calls[1].clone();

        assert!(chained_call_a == helper_chained_call_constructor(ChainedCallsEnum::cc_swap_token_a_test_1));
        assert!(chained_call_b == helper_chained_call_constructor(ChainedCallsEnum::cc_swap_token_b_test_1));
    }

    #[test]
    fn test_call_swap_successful_chain_call_2() {
        let pre_states = vec![
                helper_account_constructor(AccountEnum::pool_definition_init),
                helper_account_constructor(AccountEnum::vault_a_init),
                helper_account_constructor(AccountEnum::vault_b_init),
                helper_account_constructor(AccountEnum::user_holding_a),
                helper_account_constructor(AccountEnum::user_holding_b),
                ];
        let (post_states, chained_calls) = swap(&pre_states, 
                    &[helper_balance_constructor(BalanceEnum::add_max_amount_b),
                    helper_balance_constructor(BalanceEnum::min_amount_out)],
                    helper_id_constructor(IdEnum::token_b_definition_id),
                    );
    
        let pool_post = post_states[0].clone();

        assert!(helper_account_constructor(AccountEnum::pool_definition_swap_test_2).account ==
                    pool_post);

        let chained_call_a = chained_calls[1].clone();            
        let chained_call_b = chained_calls[0].clone();

        assert!(chained_call_a == helper_chained_call_constructor(ChainedCallsEnum::cc_swap_token_a_test_2));
        assert!(chained_call_b == helper_chained_call_constructor(ChainedCallsEnum::cc_swap_token_b_test_2));
    }
    
}