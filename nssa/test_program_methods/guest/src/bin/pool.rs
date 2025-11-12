use nssa_core::{
    account::{Account, AccountId, AccountWithMetadata, Data},
    program::{ProgramInput, read_nssa_inputs, write_nssa_outputs},
};

// The token program has two functions:
// 1. New token definition.
//    Arguments to this function are:
//      * Two **default** accounts: [definition_account, holding_account].
//        The first default account will be initialized with the token definition account values. The second account will
//        be initialized to a token holding account for the new token, holding the entire total supply.
//      * An instruction data of 23-bytes, indicating the total supply and the token name, with
//        the following layout:
//        [0x00 || total_supply (little-endian 16 bytes) || name (6 bytes)]
//        The name cannot be equal to [0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
// 2. Token transfer
//    Arguments to this function are:
//      * Two accounts: [sender_account, recipient_account].
//      * An instruction data byte string of length 23, indicating the total supply with the following layout
//        [0x01 || amount (little-endian 16 bytes) || 0x00 || 0x00 || 0x00 || 0x00 || 0x00 || 0x00].


//TODO: pool should have 2 tokens


//TODO: correct these values
const TOKEN_DEFINITION_TYPE: u8 = 0;
const POOL_DEFINITION_DATA_SIZE: usize = 19;

const TOKEN_HOLDING_TYPE: u8 = 1;
const TOKEN_HOLDING_DATA_SIZE: usize = 49;

struct PoolDefinition{
    account_type: u8,
    name_pool: [u8; 6], //TODO: unsure
    name_token_a: [u8; 6], //TODO: specifies token A
    name_token_b: [u8; 6], //TODO: specifies token B
}

struct PoolHolding {
    account_type: u8,
    definition_pool_id: AccountId,
    definition_token_a_id: AccountId,
    definition_token_b_id: AccountId,
    definition_token_lp_id: AccountId,
}


impl PoolDefinition {
    fn into_data(self) -> Vec<u8> {
        let mut bytes = [0; POOL_DEFINITION_DATA_SIZE];
        bytes[0] = self.account_type;
        bytes[1..7].copy_from_slice(&self.name_pool);
        bytes[7..13].copy_from_slice(&self.name_token_a);
        bytes[13..].copy_from_slice(&self.name_token_b);
        bytes.into();
    }
}

impl PoolHolding {
    fn new(definition_pool_id: &AccountId,
            definition_token_a_id: &AccountId,
            definition_token_b_id: &AccountId,
            definition_token_lp_id: &AccountId,
            ) -> Self {
        Self {
            account_type: TOKEN_HOLDING_TYPE, //TODO
            definition_pool_id: definition_pool_id.clone(),
            definition_token_a_id: definition_token_a_id.clone(),
            definition_token_b_id: definition_token_b_id.clone(),
            definition_token_lp_id: definition_token_lp_id.clone(),
        }
    }

    fn parse(data: &[u8]) -> Option<Self> {
        if data.len() != TOKEN_HOLDING_DATA_SIZE || data[0] != TOKEN_HOLDING_TYPE {
            None
        } else {
            let account_type = data[0];
            let definition_pool_id = AccountId::new(data[1..33].try_into().unwrap());
            let definition_token_a_id = AccountId::new(data[33..65].try_into().unwrap());
            let definition_token_b_id = AccountId::new(data[65..97].try_into().unwrap());
            let definition_token_lp_id = AccountId::new(data[97..129]);
            Some(Self {
                definition_pool_id,
                definition_token_a,
                definition_token_b,
                definition_token_lp_id,
            })
        }
    }

    fn into_data(self) -> Data {
        let mut bytes = [0; TOKEN_HOLDING_DATA_SIZE];
        bytes[0] = self.account_type;
        bytes[1..33].copy_from_slice(&self.definition_pool_id.to_bytes());
        bytes[33..65].copy_from_slice(&self.definition_token_a_id.to_bytes());
        bytes[65..97].copy_from_slice(&self.definition_token_b_id.to_bytes());
        bytes[97..].copy_from_slice(&self.definition_token_lp_id.to_bytes());
        bytes.into()
    }
}


fn initialize_pool(pre_state: &[AccountWithMetadata], balance_in: [u128]) {
    //Pool accounts: pool itself, and its 2 vaults and LP token
    //2 accounts for funding tokens
    //initial funder's LP account
    if pre_states.len() != 7 {
        panic!("Invalid number of input account")
    }

    if balance_in.len() != 2 {
        panic!("Invalid number of balance")
    }

    let mut pool = pre_state[0];
    let mut vault_a = pre_state[1];
    let mut vault_b = pre_state[2];
    let mut pool_lp = pre_state[3];
    let mut fund_a = pre_state[4];
    let mut fund_b = pre_state[5];
    let mut user_lp = pre_state[6];

    if pool.account != Account::default() || !pool.is_authorized {
        return;
    }

    if vault_a.account != Account::default() || !vault_a.is_authorized {
        return;
    }

    if pool_b.account != Account::default() || !vault_b.is_authorized {
        return;
    }

    if pool_lp.account != Account::default() || !pool_lp.account.is_authorized {
        return;
    }
    
    if !fund_a.is_authorized || !fund_b.is_authorized {
        return;
    }

    if user_lp.account != Account::default() || !user_lp.account.is_authorized {
        return;
    }
        

    let balance_a = balance_in[0];
    let balance_b = balance_in[1];

    // Prevents pool constant coefficient (k) from being 0.
    assert!(balance_a > 0);
    assert!(balance_b > 0);

    // Verify token_a and token_b are different
    token_a_id = fund_a.account.data.parse().definition_id;
    token_b_id = fund_b.account.data.parse().definition_id;
    assert!(token_a_id != token_b_id);

    // 1. Account verification
    //TODO: check a pool for (tokenA, tokenB) does not already exist?

        
    // 2. Initialize stake
    let pool_data = PoolDefinition::new(pool_id,
                    token_a_id,
                    token_b_id).into_data();

        
    // 3. LP token minting calculations
    //TODO

    // 4. Cross program calls
    //TODO
}

fn swap(pre_states: &[AccountWithMetadata], balance_in: [u128], min_amount_out: u128) {
    //Does not require pool's LP account
    if pre_states.len() != 5 {
        panic!("Invalid number of input accounts");
    }
    let pool = &pre_states[0];
    let vault_a = &pre_states[1];
    let vault_b = &pre_states[2];
    let user_a = &pre_states[3];
    let user_b = &pre_states[4];

    if balance_in.len() != 2 {
        panic!("Invalid number of input balances");
    }

    //TODO: return here
    let mut pool_holding =
        PoolHolding::parse(&pool.account.data).expect("Invalid pool data");

    //TODO: return here
    //TODO: a new account must be minted for the recipient regardless.
    //So, we should receive 3 accounts for pre_state.
    //TODO: fix sender_holding
    let mut user_holding = if recipient.account == Account::default() {
        TokenHolding::new(&sender_holding.definition_id);
    };


    // 1. Identify swap direction (a -> b or b -> a)
    // Exactly one should be 0.
    let in_a = balance_in[0];
    let in_b = balance_in[1];
    assert!( in_a == 0 || in_b == 0);
    assert!( in_a > 0 || in_b > 0);
    let a_to_b: bool = if in_a > 0 { true } else { false };

    // 2. fetch pool reserves
    assert!(vault_a.account.balance > 0);
    assert!(vault_b.account.balance > 0);

    // 3. Compute output amount
    // Note: no fees
    // Compute pool's exchange constant
    let k = vault_a.account.balance * vault_b.account.balance;
    let net_in_a = in_a;
    let net_in_b = in_b;
    let amount_out_a = if a_to_b { (vault_b.balance * net_in_b)/(vault_a.account.balance + net_in_a)}
                    else { 0 };
    let amount_out_b = if a_to_b { 0 }
                else {
                    (vault_a.account.balance * net_in_a)/(vault_b.account.balance + net_in_b) };                    

    // 4. Slippage check
    if a_to_b {
        assert!(amount_out_a > min_amount_out); }
    else{
        assert!(amount_out_b > min_amount_out); }

    //TODO Note to self: step 5 unnecessary (update reserves)

    // 6. Transfer tokens (Cross call)
    //TODO

    // 7. Result
    //TODO

}
    


fn add_liquidity(pre_state: &[AccountWithMetadata], max_balance_in: [u128], main_token: AccountId) {
    if pre_states.len() != 7 {
       panic!("Invalid number of input accounts");
    }

    let pool = &pre_states[0];
    let vault_a = &pre_states[1];
    let vault_b = &pre_states[2];
    let pool_lp = &pre_states[3];
    let user_a = &pre_states[4];
    let user_b = &pre_states[5];
    let user_lp = &pre_state[6];

    if balance_in.len() != 2 {
        panic!("Invalid number of input balances");
    }

    //TODO: add authorization checks if need be;
    //might be redundant

    max_amount_a = balance_in[0];
    max_amount_b = balance_in[1];

    // 2. Determine deposit amounts
    pool_data = pool.account.data.parse();
    let mut actual_amount_a = 0;
    let mut actual_amount_b = 0;

    if main_token == pool_data.definition_token_a {
        actual_amount_a = max_amount_a;
        actual_amount_b = (vault_b.account.balance/vault_a.account.balance)*actual_amount_a;
    } else if main_token == pool_data.definition_token_b {
        actual_amount_b = max_amount_b;
        actual_amount_a = (vault_a.account.balance/vault_b.account.balance)*actual_amount_b;
    } else {
        return; //main token does not match with vaults.
    }

    // 3. Validate amounts
    assert!(user_a.account.balance >= actual_amount_a && actual_amount_a > 0);
    assert!(user_b.account.balance >= actual_amount_b && actual_amount_b > 0)

    // 4. Calculate LP to mint
    //TODO
}




fn remove_liquidity(pre_state: &[AccountWithMetadata], max_balance_in: [u128], main_token: AccountId) {
    if pre_states.len() != 7 {
       panic!("Invalid number of input accounts");
    }

    let pool = &pre_states[0];
    let vault_a = &pre_states[1];
    let vault_b = &pre_states[2];
    let pool_lp = &pre_states[3];
    let user_a = &pre_states[4];
    let user_b = &pre_states[5];
    let user_lp = &pre_states[6];

    if balance_in.len() != 2 {
        panic!("Invalid number of input balances");
    }

    assert!(user_lp.account.balance)
    //TODO
}