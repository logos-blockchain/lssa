use nssa_core::{
    account::{Account, AccountId, AccountWithMetadata, Data},
    program::{
        AccountPostState, ChainedCall, PdaSeed, ProgramId, ProgramInput, read_nssa_inputs,
        write_nssa_outputs_with_chained_call,
    },
};

// The AMM program has five functions (four directly accessible via instructions):
// 1. New AMM definition.
//    Arguments to this function are:
//      * Seven accounts: [amm_pool, vault_holding_a, vault_holding_b, pool_lp, user_holding_a, user_holding_b, user_holding_lp].
//        For new AMM Pool: amm_pool, vault_holding_a, vault_holding_b, pool_lp and user_holding_lp are default accounts.
//        amm_pool is a default account that will initiate the amm definition account values
//        vault_holding_a is a token holding account for token a
//        vault_holding_b is a token holding account for token b
//        pool_lp is a token holding account for the pool's lp token
//        user_holding_a is a token holding account for token a
//        user_holding_b is a token holding account for token b
//        user_holding_lp is a token holding account for lp token
//      * PDA remark: Accounts amm_pool, vault_holding_a, vault_holding_b and pool_lp are PDA.
//        The AccountId for these accounts must be computed using:
//              amm_pool AccountId <- compute_pool_pda
//              vault_holding_a, vault_holding_b <- compute_vault_pda
//              pool_lp <-compute_liquidity_token_pda
//      * Requires authorization: user_holding_a, user_holding_b
//      * An instruction data of 65-bytes, indicating the initial amm reserves' balances and token_program_id with
//        the following layout:
//        [0x00 || array of balances (little-endian 16 bytes) || AMM_PROGRAM_ID)]
//      * Internally, calls compute_liquidity_token_pda_seed, compute_vault_pda_seed to authorize transfers.
//      * Internally, calls compute_pool_da, compute_vault_pda and compute_vault_pda to check various AccountIds are correct.
// 2. Swap assets
//    Arguments to this function are:
//      * Five accounts: [amm_pool, vault_holding_a, vault_holding_b, user_holding_a, user_holding_b].
//      * Requires authorization: user holding account associated to TOKEN_DEFINITION_ID (either user_holding_a or user_holding_b)
//      * An instruction data byte string of length 65, indicating which token type to swap, quantity of tokens put into the swap
//        (of type TOKEN_DEFINITION_ID) and min_amount_out.
//        [0x01 || amount (little-endian 16 bytes) || TOKEN_DEFINITION_ID].
//      * Internally, calls swap logic.
//              * Four accounts: [user_deposit, vault_deposit, vault_withdraw, user_withdraw].
//                user_deposit and vault_deposit define deposit transaction.
//                vault_withdraw and user_withdraw define withdraw transaction.
//              * deposit_amount is the amount for user_deposit -> vault_deposit transfer.
//              * reserve_amounts is the pool's reserves; used to compute the withdraw amount.
//              * Outputs the token transfers as a Vec<ChainedCall> and the withdraw amount.
// 3. Add liquidity
//    Arguments to this function are:
//      * Seven accounts: [amm_pool, vault_holding_a, vault_holding_b, pool_lp, user_holding_a, user_holding_a, user_holding_lp].
//      * Requires authorization: user_holding_a, user_holding_b
//      * An instruction data byte string of length 49, amounts for minimum amount of liquidity from add (min_amount_lp),
//      * max amount added for each token (max_amount_a and max_amount_b); indicate
//        [0x02 || array of of balances (little-endian 16 bytes)].
//      * Internally, calls compute_liquidity_token_pda_seed to compute liquidity pool PDA seed.
// 4. Remove liquidity
//      * Seven accounts: [amm_pool, vault_holding_a, vault_holding_b, pool_lp, user_holding_a, user_holding_a, user_holding_lp].
//      * Requires authorization: user_holding_lp
//      * An instruction data byte string of length 49, amounts for minimum amount of liquidity to redeem (balance_lp),
//      * minimum balance of each token to remove (min_amount_a and min_amount_b); indicate
//        [0x03 || array of balances (little-endian 16 bytes)].
//      * Internally, calls compute_vault_pda_seed to compute vault_a and vault_b's PDA seed.

const POOL_DEFINITION_DATA_SIZE: usize = 225;

#[derive(Clone, Default)]
struct PoolDefinition {
    definition_token_a_id: AccountId,
    definition_token_b_id: AccountId,
    vault_a_id: AccountId,
    vault_b_id: AccountId,
    liquidity_pool_id: AccountId,
    liquidity_pool_supply: u128,
    reserve_a: u128,
    reserve_b: u128,
    /// Fees are currently not used
    fees: u128,
    /// A pool becomes inactive (active = false)
    /// once all of its liquidity has been removed (e.g., reserves are emptied and liquidity_pool_supply = 0)
    active: bool,
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
    let (
        ProgramInput {
            pre_states,
            instruction,
        },
        instruction_words,
    ) = read_nssa_inputs::<Instruction>();

    let (post_states, chained_calls) =
        match instruction[0] {
            0 => {
                let balance_a: u128 = u128::from_le_bytes(
                    instruction[1..17]
                        .try_into()
                        .expect("New definition: AMM Program expects u128 for balance a"),
                );
                let balance_b: u128 = u128::from_le_bytes(
                    instruction[17..33]
                        .try_into()
                        .expect("New definition: AMM Program expects u128 for balance b"),
                );

                // Convert Vec<u8> to ProgramId ([u32;8])
                let mut amm_program_id: [u32; 8] = [0; 8];
                amm_program_id[0] = u32::from_le_bytes(
                    instruction[33..37]
                        .try_into()
                        .expect("New definition: AMM Program expects valid u32"),
                );
                amm_program_id[1] = u32::from_le_bytes(
                    instruction[37..41]
                        .try_into()
                        .expect("New definition: AMM Program expects valid u32"),
                );
                amm_program_id[2] = u32::from_le_bytes(
                    instruction[41..45]
                        .try_into()
                        .expect("New definition: AMM Program expects valid u32"),
                );
                amm_program_id[3] = u32::from_le_bytes(
                    instruction[45..49]
                        .try_into()
                        .expect("New definition: AMM Program expects valid u32"),
                );
                amm_program_id[4] = u32::from_le_bytes(
                    instruction[49..53]
                        .try_into()
                        .expect("New definition: AMM Program expects valid u32"),
                );
                amm_program_id[5] = u32::from_le_bytes(
                    instruction[53..57]
                        .try_into()
                        .expect("New definition: AMM Program expects valid u32"),
                );
                amm_program_id[6] = u32::from_le_bytes(
                    instruction[57..61]
                        .try_into()
                        .expect("New definition: AMM Program expects valid u32"),
                );
                amm_program_id[7] = u32::from_le_bytes(
                    instruction[61..65]
                        .try_into()
                        .expect("New definition: AMM Program expects valid u32"),
                );

                new_definition(&pre_states, &[balance_a, balance_b], amm_program_id)
            }
            1 => {
                let mut token_in_id: [u8; 32] = [0; 32];
                token_in_id[0..].copy_from_slice(&instruction[33..65]);
                let token_in_id = AccountId::new(token_in_id);

                let amount_in = u128::from_le_bytes(
                    instruction[1..17]
                        .try_into()
                        .expect("Swap: AMM Program expects valid u128 for balance to move"),
                );
                let min_amount_out = u128::from_le_bytes(
                    instruction[17..33]
                        .try_into()
                        .expect("Swap: AMM Program expects valid u128 for balance to move"),
                );

                swap(&pre_states, &[amount_in, min_amount_out], token_in_id)
            }
            2 => {
                let min_amount_lp = u128::from_le_bytes(instruction[1..17].try_into().expect(
                    "Add liquidity: AMM Program expects valid u128 for min amount liquidity",
                ));
                let max_amount_a = u128::from_le_bytes(
                    instruction[17..33]
                        .try_into()
                        .expect("Add liquidity: AMM Program expects valid u128 for max amount a"),
                );
                let max_amount_b = u128::from_le_bytes(
                    instruction[33..49]
                        .try_into()
                        .expect("Add liquidity: AMM Program expects valid u128 for max amount b"),
                );

                add_liquidity(&pre_states, &[min_amount_lp, max_amount_a, max_amount_b])
            }
            3 => {
                let balance_lp = u128::from_le_bytes(instruction[1..17].try_into().expect(
                    "Remove liquidity: AMM Program expects valid u128 for balance liquidity",
                ));
                let min_amount_a = u128::from_le_bytes(
                    instruction[17..33]
                        .try_into()
                        .expect("Remove liquidity: AMM Program expects valid u128 for balance a"),
                );
                let min_amount_b = u128::from_le_bytes(
                    instruction[33..49]
                        .try_into()
                        .expect("Remove liquidity: AMM Program expects valid u128 for balance b"),
                );

                remove_liquidity(&pre_states, &[balance_lp, min_amount_a, min_amount_b])
            }
            _ => panic!("Invalid instruction"),
        };

    write_nssa_outputs_with_chained_call(instruction_words, pre_states, post_states, chained_calls);
}

fn compute_pool_pda(
    amm_program_id: ProgramId,
    definition_token_a_id: AccountId,
    definition_token_b_id: AccountId,
) -> AccountId {
    AccountId::from((
        &amm_program_id,
        &compute_pool_pda_seed(definition_token_a_id, definition_token_b_id),
    ))
}

fn compute_pool_pda_seed(
    definition_token_a_id: AccountId,
    definition_token_b_id: AccountId,
) -> PdaSeed {
    use risc0_zkvm::sha::{Impl, Sha256};

    let (token_1, token_2) = match definition_token_a_id
        .value()
        .cmp(definition_token_b_id.value())
    {
        std::cmp::Ordering::Less => (definition_token_b_id.clone(), definition_token_a_id.clone()),
        std::cmp::Ordering::Greater => {
            (definition_token_a_id.clone(), definition_token_b_id.clone())
        }
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

fn compute_vault_pda(
    amm_program_id: ProgramId,
    pool_id: AccountId,
    definition_token_id: AccountId,
) -> AccountId {
    AccountId::from((
        &amm_program_id,
        &compute_vault_pda_seed(pool_id, definition_token_id),
    ))
}

fn compute_vault_pda_seed(pool_id: AccountId, definition_token_id: AccountId) -> PdaSeed {
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

fn compute_liquidity_token_pda(amm_program_id: ProgramId, pool_id: AccountId) -> AccountId {
    AccountId::from((&amm_program_id, &compute_liquidity_token_pda_seed(pool_id)))
}

fn compute_liquidity_token_pda_seed(pool_id: AccountId) -> PdaSeed {
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

const TOKEN_PROGRAM_NEW: u8 = 0;
const TOKEN_PROGRAM_TRANSFER: u8 = 1;
const TOKEN_PROGRAM_MINT: u8 = 4;
const TOKEN_PROGRAM_BURN: u8 = 3;

fn initialize_token_transfer_chained_call(
    token_program_command: u8,
    sender: AccountWithMetadata,
    recipient: AccountWithMetadata,
    amount_to_move: u128,
    pda_seed: Vec<PdaSeed>,
) -> ChainedCall {
    let mut instruction_data = [0; 23];
    instruction_data[0] = token_program_command;
    instruction_data[1..17].copy_from_slice(&amount_to_move.to_le_bytes());
    let instruction_data = risc0_zkvm::serde::to_vec(&instruction_data)
        .expect("AMM Program expects valid token transfer instruction data");

    ChainedCall {
        program_id: sender.account.program_owner,
        instruction_data,
        pre_states: vec![sender, recipient],
        pda_seeds: pda_seed,
    }
}

fn new_definition(
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
        .expect("New definition: AMM Program expects valid Token Holding account for Token A")
        .definition_id;
    let definition_token_b_id = TokenHolding::parse(&user_holding_b.account.data)
        .expect("New definition: AMM Program expects valid Token Holding account for Token B")
        .definition_id;

    // both instances of the same token program
    let token_program = user_holding_a.account.program_owner;

    if user_holding_b.account.program_owner != token_program {
        panic!("User Token holdings must use the same Token Program");
    }

    if definition_token_a_id == definition_token_b_id {
        panic!("Cannot set up a swap for a token with itself")
    }

    if pool.account_id
        != compute_pool_pda(
            amm_program_id.clone(),
            definition_token_a_id.clone(),
            definition_token_b_id.clone(),
        )
    {
        panic!("Pool Definition Account ID does not match PDA");
    }

    if vault_a.account_id
        != compute_vault_pda(
            amm_program_id.clone(),
            pool.account_id.clone(),
            definition_token_a_id.clone(),
        )
        || vault_b.account_id
            != compute_vault_pda(
                amm_program_id.clone(),
                pool.account_id.clone(),
                definition_token_b_id.clone(),
            )
    {
        panic!("Vault ID does not match PDA");
    }

    if pool_lp.account_id
        != compute_liquidity_token_pda(amm_program_id.clone(), pool.account_id.clone())
    {
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

    // LP Token minting calculation
    // We assume LP is based on the initial deposit amount for Token_A.

    // Update pool account
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

    pool_post.data = pool_post_definition.into_data();
    let pool_post: AccountPostState = if pool.account == Account::default() {
        AccountPostState::new_claimed(pool_post.clone())
    } else {
        AccountPostState::new(pool_post.clone())
    };

    let mut chained_calls = Vec::<ChainedCall>::new();

    //Chain call for Token A (user_holding_a -> Vault_A)
    let call_token_a = initialize_token_transfer_chained_call(
        TOKEN_PROGRAM_TRANSFER,
        user_holding_a.clone(),
        vault_a.clone(),
        amount_a,
        Vec::<PdaSeed>::new(),
    );
    //Chain call for Token B (user_holding_b -> Vault_B)
    let call_token_b = initialize_token_transfer_chained_call(
        TOKEN_PROGRAM_TRANSFER,
        user_holding_b.clone(),
        vault_b.clone(),
        amount_b,
        Vec::<PdaSeed>::new(),
    );

    //Chain call for liquidity token (TokenLP definition -> User LP Holding)
    let mut instruction_data = [0; 23];
    instruction_data[0] = if pool.account == Account::default() {
        TOKEN_PROGRAM_NEW
    } else {
        TOKEN_PROGRAM_MINT
    }; //new or mint
    let nme = if pool.account == Account::default() {
        [1u8; 6]
    } else {
        [0u8; 6]
    };

    instruction_data[1..17].copy_from_slice(&amount_a.to_le_bytes());
    instruction_data[17..].copy_from_slice(&nme);
    let instruction_data = risc0_zkvm::serde::to_vec(&instruction_data)
        .expect("New definition: AMM Program expects valid instruction_data");

    let mut pool_lp_auth = pool_lp.clone();
    pool_lp_auth.is_authorized = true;

    let token_program_id = user_holding_a.account.program_owner;
    let call_token_lp = ChainedCall {
        program_id: token_program_id,
        instruction_data,
        pre_states: vec![pool_lp_auth.clone(), user_holding_lp.clone()],
        pda_seeds: vec![compute_liquidity_token_pda_seed(pool.account_id.clone())],
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
        AccountPostState::new(pre_states[6].account.clone()),
    ];

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

    let pool = &pre_states[0];
    let vault_a = &pre_states[1];
    let vault_b = &pre_states[2];
    let user_holding_a = &pre_states[3];
    let user_holding_b = &pre_states[4];

    // Verify vaults are in fact vaults
    let pool_def_data = PoolDefinition::parse(&pool.account.data)
        .expect("Swap: AMM Program expects a valid Pool Definition Account");

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
    if TokenHolding::parse(&vault_a.account.data)
        .expect("Swap: AMM Program expects a valid Token Holding Account for Vault A")
        .balance
        < pool_def_data.reserve_a
    {
        panic!("Reserve for Token A exceeds vault balance");
    }
    if TokenHolding::parse(&vault_b.account.data)
        .expect("Swap: AMM Program expects a valid Token Holding Account for Vault B")
        .balance
        < pool_def_data.reserve_b
    {
        panic!("Reserve for Token B exceeds vault balance");
    }

    let (chained_calls, [deposit_a, withdraw_a], [deposit_b, withdraw_b]) =
        if token_in_id == pool_def_data.definition_token_a_id {
            let (chained_calls, deposit_a, withdraw_b) = swap_logic(
                user_holding_a.clone(),
                vault_a.clone(),
                vault_b.clone(),
                user_holding_b.clone(),
                amounts[0],
                amounts[1],
                &[pool_def_data.reserve_a, pool_def_data.reserve_b],
                pool.account_id.clone(),
            );

            (chained_calls, [deposit_a, 0], [0, withdraw_b])
        } else if token_in_id == pool_def_data.definition_token_b_id {
            let (chained_calls, deposit_b, withdraw_a) = swap_logic(
                user_holding_b.clone(),
                vault_b.clone(),
                vault_a.clone(),
                user_holding_a.clone(),
                amounts[0],
                amounts[1],
                &[pool_def_data.reserve_b, pool_def_data.reserve_a],
                pool.account_id.clone(),
            );

            (chained_calls, [0, withdraw_a], [deposit_b, 0])
        } else {
            panic!("AccountId is not a token type for the pool");
        };

    // Update pool account
    let mut pool_post = pool.account.clone();
    let pool_post_definition = PoolDefinition {
        reserve_a: pool_def_data.reserve_a + deposit_a - withdraw_a,
        reserve_b: pool_def_data.reserve_b + deposit_b - withdraw_b,
        ..pool_def_data
    };

    pool_post.data = pool_post_definition.into_data();

    let post_states = vec![
        AccountPostState::new(pool_post.clone()),
        AccountPostState::new(pre_states[1].account.clone()),
        AccountPostState::new(pre_states[2].account.clone()),
        AccountPostState::new(pre_states[3].account.clone()),
        AccountPostState::new(pre_states[4].account.clone()),
    ];

    (post_states, chained_calls)
}

fn swap_logic(
    user_deposit: AccountWithMetadata,
    vault_deposit: AccountWithMetadata,
    vault_withdraw: AccountWithMetadata,
    user_withdraw: AccountWithMetadata,
    deposit_amount: u128,
    min_amount_out: u128,
    reserve_amounts: &[u128],
    pool_id: AccountId,
) -> (Vec<ChainedCall>, u128, u128) {
    let reserve_deposit_vault_amount = reserve_amounts[0];
    let reserve_withdraw_vault_amount = reserve_amounts[1];

    // Compute withdraw amount
    // Maintains pool constant product
    // k = pool_def_data.reserve_a * pool_def_data.reserve_b;
    let withdraw_amount = (reserve_withdraw_vault_amount * deposit_amount)
        / (reserve_deposit_vault_amount + deposit_amount);

    //Slippage check
    if min_amount_out > withdraw_amount {
        panic!("Withdraw amount is less than minimal amount out");
    }

    if withdraw_amount == 0 {
        panic!("Withdraw amount should be nonzero");
    }

    let mut chained_calls = Vec::new();
    chained_calls.push(initialize_token_transfer_chained_call(
        TOKEN_PROGRAM_TRANSFER,
        user_deposit.clone(),
        vault_deposit.clone(),
        deposit_amount,
        Vec::<PdaSeed>::new(),
    ));

    let mut vault_withdraw = vault_withdraw.clone();
    vault_withdraw.is_authorized = true;

    chained_calls.push(initialize_token_transfer_chained_call(
        TOKEN_PROGRAM_TRANSFER,
        vault_withdraw.clone(),
        user_withdraw.clone(),
        withdraw_amount,
        vec![compute_vault_pda_seed(
            pool_id,
            TokenHolding::parse(&vault_withdraw.account.data)
                .expect("Swap Logic: AMM Program expects valid token data")
                .definition_id,
        )],
    ));

    (chained_calls, deposit_amount, withdraw_amount)
}

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

    // 1. Fetch Pool state
    let pool_def_data = PoolDefinition::parse(&pool.account.data)
        .expect("Add liquidity: AMM Program expects valid Pool Definition Account");
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
    let vault_b_balance = TokenHolding::parse(&vault_b.account.data)
        .expect("Add liquidity: AMM Program expects valid Token Holding Account for Vault B")
        .balance;
    let vault_a_balance = TokenHolding::parse(&vault_a.account.data)
        .expect("Add liquidity: AMM Program expects valid Token Holding Account for Vault A")
        .balance;

    if pool_def_data.reserve_a == 0 || pool_def_data.reserve_b == 0 {
        panic!("Reserves must be nonzero");
    }

    if vault_a_balance < pool_def_data.reserve_a || vault_b_balance < pool_def_data.reserve_b {
        panic!("Vaults' balances must be at least the reserve amounts");
    }

    // Calculate actual_amounts
    let ideal_a: u128 = (pool_def_data.reserve_a * max_amount_b) / pool_def_data.reserve_b;
    let ideal_b: u128 = (pool_def_data.reserve_b * max_amount_a) / pool_def_data.reserve_a;

    let actual_amount_a = if ideal_a > max_amount_a {
        max_amount_a
    } else {
        ideal_a
    };
    let actual_amount_b = if ideal_b > max_amount_b {
        max_amount_b
    } else {
        ideal_b
    };

    // 3. Validate amounts
    if max_amount_a < actual_amount_a || max_amount_b < actual_amount_b {
        panic!("Actual trade amounts cannot exceed max_amounts");
    }

    if actual_amount_a == 0 || actual_amount_b == 0 {
        panic!("A trade amount is 0");
    }

    // 4. Calculate LP to mint
    let delta_lp = std::cmp::min(
        pool_def_data.liquidity_pool_supply * actual_amount_a / pool_def_data.reserve_a,
        pool_def_data.liquidity_pool_supply * actual_amount_b / pool_def_data.reserve_b,
    );

    if delta_lp == 0 {
        panic!("Payable LP must be nonzero");
    }

    if delta_lp < min_amount_lp {
        panic!("Payable LP is less than provided minimum LP amount");
    }

    // 5. Update pool account
    let mut pool_post = pool.account.clone();
    let pool_post_definition = PoolDefinition {
        liquidity_pool_supply: pool_def_data.liquidity_pool_supply + delta_lp,
        reserve_a: pool_def_data.reserve_a + actual_amount_a,
        reserve_b: pool_def_data.reserve_b + actual_amount_b,
        ..pool_def_data
    };

    pool_post.data = pool_post_definition.into_data();
    let mut chained_call = Vec::new();

    // Chain call for Token A (UserHoldingA -> Vault_A)
    let call_token_a = initialize_token_transfer_chained_call(
        TOKEN_PROGRAM_TRANSFER,
        user_holding_a.clone(),
        vault_a.clone(),
        actual_amount_a,
        Vec::<PdaSeed>::new(),
    );
    // Chain call for Token B (UserHoldingB -> Vault_B)
    let call_token_b = initialize_token_transfer_chained_call(
        TOKEN_PROGRAM_TRANSFER,
        user_holding_b.clone(),
        vault_b.clone(),
        actual_amount_b,
        Vec::<PdaSeed>::new(),
    );
    // Chain call for LP (mint new tokens for user_holding_lp)
    let mut pool_definition_lp_auth = pool_definition_lp.clone();
    pool_definition_lp_auth.is_authorized = true;
    let call_token_lp = initialize_token_transfer_chained_call(
        TOKEN_PROGRAM_MINT,
        pool_definition_lp_auth.clone(),
        user_holding_lp.clone(),
        delta_lp,
        vec![compute_liquidity_token_pda_seed(pool.account_id.clone())],
    );

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
        AccountPostState::new(pre_states[6].account.clone()),
    ];

    (post_states, chained_call)
}

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

    // 1. Fetch Pool state
    let pool_def_data = PoolDefinition::parse(&pool.account.data)
        .expect("Remove liquidity: AMM Program expects a valid Pool Definition Account");

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
    let user_holding_lp_data = TokenHolding::parse(&user_holding_lp.account.data)
        .expect("Remove liquidity: AMM Program expects a valid Token Account for liquidity token");

    if user_holding_lp_data.balance > pool_def_data.liquidity_pool_supply
        || user_holding_lp_data.definition_id != pool_def_data.liquidity_pool_id
    {
        panic!("Invalid liquidity account provided");
    }

    let withdraw_amount_a =
        (pool_def_data.reserve_a * amount_lp) / pool_def_data.liquidity_pool_supply;
    let withdraw_amount_b =
        (pool_def_data.reserve_b * amount_lp) / pool_def_data.liquidity_pool_supply;

    // 3. Validate and slippage check
    if withdraw_amount_a < amount_min_a {
        panic!("Insufficient minimal withdraw amount (Token A) provided for liquidity amount");
    }
    if withdraw_amount_b < amount_min_b {
        panic!("Insufficient minimal withdraw amount (Token B) provided for liquidity amount");
    }

    // 4. Calculate LP to reduce cap by
    let delta_lp: u128 =
        (pool_def_data.liquidity_pool_supply * amount_lp) / pool_def_data.liquidity_pool_supply;

    let active: bool = if pool_def_data.liquidity_pool_supply - delta_lp == 0 {
        false
    } else {
        true
    };

    // 5. Update pool account
    let mut pool_post = pool.account.clone();
    let pool_post_definition = PoolDefinition {
        liquidity_pool_supply: pool_def_data.liquidity_pool_supply - delta_lp,
        reserve_a: pool_def_data.reserve_a - withdraw_amount_a,
        reserve_b: pool_def_data.reserve_b - withdraw_amount_b,
        active,
        ..pool_def_data.clone()
    };

    pool_post.data = pool_post_definition.into_data();

    let mut chained_calls = Vec::new();

    //Chaincall for Token A withdraw
    let call_token_a = initialize_token_transfer_chained_call(
        TOKEN_PROGRAM_TRANSFER,
        running_vault_a,
        user_holding_a.clone(),
        withdraw_amount_a,
        vec![compute_vault_pda_seed(
            pool.account_id.clone(),
            pool_def_data.definition_token_a_id.clone(),
        )],
    );
    //Chaincall for Token B withdraw
    let call_token_b = initialize_token_transfer_chained_call(
        TOKEN_PROGRAM_TRANSFER,
        running_vault_b,
        user_holding_b.clone(),
        withdraw_amount_b,
        vec![compute_vault_pda_seed(
            pool.account_id.clone(),
            pool_def_data.definition_token_b_id.clone(),
        )],
    );
    //Chaincall for LP adjustment
    let mut pool_definition_lp_auth = pool_definition_lp.clone();
    pool_definition_lp_auth.is_authorized = true;
    let call_token_lp = initialize_token_transfer_chained_call(
        TOKEN_PROGRAM_BURN,
        pool_definition_lp_auth.clone(),
        user_holding_lp.clone(),
        delta_lp,
        vec![compute_liquidity_token_pda_seed(pool.account_id.clone())],
    );

    chained_calls.push(call_token_lp);
    chained_calls.push(call_token_b);
    chained_calls.push(call_token_a);

    let post_states = vec![
        AccountPostState::new(pool_post.clone()),
        AccountPostState::new(pre_states[1].account.clone()),
        AccountPostState::new(pre_states[2].account.clone()),
        AccountPostState::new(pre_states[3].account.clone()),
        AccountPostState::new(pre_states[4].account.clone()),
        AccountPostState::new(pre_states[5].account.clone()),
        AccountPostState::new(pre_states[6].account.clone()),
    ];

    (post_states, chained_calls)
}

#[cfg(test)]
mod tests {
    use nssa_core::{
        program::ProgramId,
        {
            account::{Account, AccountId, AccountWithMetadata},
            program::ChainedCall,
            program::PdaSeed,
        },
    };

    use crate::{
        PoolDefinition, TokenDefinition, TokenHolding, add_liquidity, compute_liquidity_token_pda,
        compute_liquidity_token_pda_seed, compute_pool_pda, compute_pool_pda_seed,
        compute_vault_pda, compute_vault_pda_seed, new_definition, remove_liquidity, swap,
    };

    const TOKEN_PROGRAM_ID: ProgramId = [15; 8];
    const AMM_PROGRAM_ID: ProgramId = [42; 8];

    struct BalanceForTests;

    impl BalanceForTests {
        fn vault_a_reserve_init() -> u128 {
            1_000
        }

        fn vault_b_reserve_init() -> u128 {
            500
        }

        fn vault_a_reserve_low() -> u128 {
            10
        }

        fn vault_b_reserve_low() -> u128 {
            10
        }

        fn vault_a_reserve_high() -> u128 {
            500_000
        }

        fn vault_b_reserve_high() -> u128 {
            500_000
        }

        fn user_token_a_balance() -> u128 {
            1_000
        }

        fn user_token_b_balance() -> u128 {
            500
        }

        fn user_token_lp_balance() -> u128 {
            100
        }

        fn remove_min_amount_a() -> u128 {
            50
        }

        fn remove_min_amount_b() -> u128 {
            100
        }

        fn remove_actual_a_successful() -> u128 {
            100
        }

        fn remove_min_amount_b_low() -> u128 {
            50
        }

        fn remove_amount_lp() -> u128 {
            100
        }

        fn remove_amount_lp_1() -> u128 {
            30
        }

        fn add_max_amount_a() -> u128 {
            500
        }

        fn add_max_amount_b() -> u128 {
            200
        }

        fn add_max_amount_b_high() -> u128 {
            20_000
        }

        fn add_max_amount_a_low() -> u128 {
            10
        }

        fn add_max_amount_b_low() -> u128 {
            10
        }

        fn add_min_amount_lp() -> u128 {
            20
        }

        fn vault_a_swap_test_1() -> u128 {
            1_500
        }

        fn vault_a_swap_test_2() -> u128 {
            715
        }

        fn vault_b_swap_test_1() -> u128 {
            334
        }

        fn vault_b_swap_test_2() -> u128 {
            700
        }

        fn min_amount_out() -> u128 {
            200
        }

        fn vault_a_add_successful() -> u128 {
            1_400
        }

        fn vault_b_add_successful() -> u128 {
            700
        }

        fn add_successful_amount_a() -> u128 {
            400
        }

        fn add_successful_amount_b() -> u128 {
            200
        }

        fn vault_a_remove_successful() -> u128 {
            900
        }

        fn vault_b_remove_successful() -> u128 {
            450
        }
    }

    struct ChainedCallForTests;

    impl ChainedCallForTests {
        fn cc_token_a_initialization() -> ChainedCall {
            let mut instruction: [u8; 23] = [0; 23];
            instruction[0] = 1;
            instruction[1..17]
                .copy_from_slice(&BalanceForTests::user_token_a_balance().to_le_bytes());
            let instruction_data = risc0_zkvm::serde::to_vec(&instruction)
                .expect("AMM Program expects valid transaction instruction data");
            ChainedCall {
                program_id: TOKEN_PROGRAM_ID,
                instruction_data,
                pre_states: vec![
                    AccountForTests::user_holding_a(),
                    AccountForTests::vault_a_uninit(),
                ],
                pda_seeds: Vec::<PdaSeed>::new(),
            }
        }

        fn cc_token_b_initialization() -> ChainedCall {
            let mut instruction: [u8; 23] = [0; 23];
            instruction[0] = 1;
            instruction[1..17]
                .copy_from_slice(&BalanceForTests::user_token_b_balance().to_le_bytes());
            let instruction_data = risc0_zkvm::serde::to_vec(&instruction)
                .expect("AMM Program expects valid transaction instruction data");
            ChainedCall {
                program_id: TOKEN_PROGRAM_ID,
                instruction_data,
                pre_states: vec![
                    AccountForTests::user_holding_b(),
                    AccountForTests::vault_b_uninit(),
                ],
                pda_seeds: Vec::<PdaSeed>::new(),
            }
        }

        fn cc_pool_lp_initialization() -> ChainedCall {
            let mut instruction: [u8; 23] = [0; 23];
            instruction[0] = 1;
            instruction[1..17]
                .copy_from_slice(&BalanceForTests::user_token_a_balance().to_le_bytes());
            let instruction_data = risc0_zkvm::serde::to_vec(&instruction)
                .expect("AMM Program expects valid transaction instruction data");
            ChainedCall {
                program_id: TOKEN_PROGRAM_ID,
                instruction_data,
                pre_states: vec![
                    AccountForTests::pool_lp_uninit(),
                    AccountForTests::user_holding_lp_uninit(),
                ],
                pda_seeds: Vec::<PdaSeed>::new(),
            }
        }

        fn cc_swap_token_a_test_1() -> ChainedCall {
            let mut instruction_data: [u8; 23] = [0; 23];
            instruction_data[0] = 1;
            instruction_data[1..17]
                .copy_from_slice(&BalanceForTests::add_max_amount_a().to_le_bytes());
            let instruction_data = risc0_zkvm::serde::to_vec(&instruction_data)
                .expect("AMM Program expects valid transaction instruction data");
            ChainedCall {
                program_id: TOKEN_PROGRAM_ID,
                instruction_data,
                pre_states: vec![
                    AccountForTests::user_holding_a(),
                    AccountForTests::vault_a_init(),
                ],
                pda_seeds: Vec::<PdaSeed>::new(),
            }
        }

        fn cc_swap_token_b_test_1() -> ChainedCall {
            let swap_amount: u128 = 166;

            let mut vault_b_auth = AccountForTests::vault_b_init();
            vault_b_auth.is_authorized = true;

            let mut instruction: [u8; 23] = [0; 23];
            instruction[0] = 1;
            instruction[1..17].copy_from_slice(&swap_amount.to_le_bytes());
            let instruction_data = risc0_zkvm::serde::to_vec(&instruction)
                .expect("AMM Program expects valid transaction instruction data");
            ChainedCall {
                program_id: TOKEN_PROGRAM_ID,
                instruction_data,
                pre_states: vec![vault_b_auth, AccountForTests::user_holding_b()],
                pda_seeds: vec![compute_vault_pda_seed(
                    IdForTests::pool_definition_id(),
                    IdForTests::token_b_definition_id(),
                )],
            }
        }

        fn cc_swap_token_a_test_2() -> ChainedCall {
            let swap_amount: u128 = 285;

            let mut vault_a_auth = AccountForTests::vault_a_init();
            vault_a_auth.is_authorized = true;

            let mut instruction_data: [u8; 23] = [0; 23];
            instruction_data[0] = 1;
            instruction_data[1..17].copy_from_slice(&swap_amount.to_le_bytes());
            let instruction_data = risc0_zkvm::serde::to_vec(&instruction_data)
                .expect("AMM Program expects valid transaction instruction data");
            ChainedCall {
                program_id: TOKEN_PROGRAM_ID,
                instruction_data,
                pre_states: vec![vault_a_auth, AccountForTests::user_holding_a()],
                pda_seeds: vec![compute_vault_pda_seed(
                    IdForTests::pool_definition_id(),
                    IdForTests::token_a_definition_id(),
                )],
            }
        }

        fn cc_swap_token_b_test_2() -> ChainedCall {
            let mut instruction: [u8; 23] = [0; 23];
            instruction[0] = 1;
            instruction[1..17].copy_from_slice(&BalanceForTests::add_max_amount_b().to_le_bytes());
            let instruction_data = risc0_zkvm::serde::to_vec(&instruction)
                .expect("AMM Program expects valid transaction instruction data");
            ChainedCall {
                program_id: TOKEN_PROGRAM_ID,
                instruction_data,
                pre_states: vec![
                    AccountForTests::user_holding_b(),
                    AccountForTests::vault_b_init(),
                ],
                pda_seeds: Vec::<PdaSeed>::new(),
            }
        }

        fn cc_add_token_a() -> ChainedCall {
            let mut instruction: [u8; 23] = [0; 23];
            instruction[0] = 1;
            instruction[1..17]
                .copy_from_slice(&BalanceForTests::add_successful_amount_a().to_le_bytes());
            let instruction_data = risc0_zkvm::serde::to_vec(&instruction)
                .expect("AMM Program expects valid transaction instruction data");
            ChainedCall {
                program_id: TOKEN_PROGRAM_ID,
                instruction_data,
                pre_states: vec![
                    AccountForTests::user_holding_a(),
                    AccountForTests::vault_a_init(),
                ],
                pda_seeds: Vec::<PdaSeed>::new(),
            }
        }

        fn cc_add_token_b() -> ChainedCall {
            let mut instruction: [u8; 23] = [0; 23];
            instruction[0] = 1;
            instruction[1..17]
                .copy_from_slice(&BalanceForTests::add_successful_amount_b().to_le_bytes());
            let instruction_data = risc0_zkvm::serde::to_vec(&instruction)
                .expect("Swap Logic: AMM Program expects valid transaction instruction data");
            ChainedCall {
                program_id: TOKEN_PROGRAM_ID,
                instruction_data,
                pre_states: vec![
                    AccountForTests::user_holding_b(),
                    AccountForTests::vault_b_init(),
                ],
                pda_seeds: Vec::<PdaSeed>::new(),
            }
        }

        fn cc_add_pool_lp() -> ChainedCall {
            let mut pool_lp_auth = AccountForTests::pool_lp_init();
            pool_lp_auth.is_authorized = true;

            let mut instruction: [u8; 23] = [0; 23];
            instruction[0] = 4;
            instruction[1..17]
                .copy_from_slice(&BalanceForTests::add_successful_amount_a().to_le_bytes());
            let instruction_data = risc0_zkvm::serde::to_vec(&instruction)
                .expect("Swap Logic: AMM Program expects valid transaction instruction data");
            ChainedCall {
                program_id: TOKEN_PROGRAM_ID,
                instruction_data,
                pre_states: vec![pool_lp_auth, AccountForTests::user_holding_lp_init()],
                pda_seeds: vec![compute_liquidity_token_pda_seed(
                    IdForTests::pool_definition_id(),
                )],
            }
        }

        fn cc_remove_token_a() -> ChainedCall {
            let mut vault_a_auth = AccountForTests::vault_a_init();
            vault_a_auth.is_authorized = true;

            let mut instruction: [u8; 23] = [0; 23];
            instruction[0] = 1;
            instruction[1..17]
                .copy_from_slice(&BalanceForTests::remove_actual_a_successful().to_le_bytes());
            let instruction_data = risc0_zkvm::serde::to_vec(&instruction)
                .expect("AMM Program expects valid transaction instruction data");
            ChainedCall {
                program_id: TOKEN_PROGRAM_ID,
                instruction_data,
                pre_states: vec![vault_a_auth, AccountForTests::user_holding_a()],
                pda_seeds: vec![compute_vault_pda_seed(
                    IdForTests::pool_definition_id(),
                    IdForTests::token_a_definition_id(),
                )],
            }
        }

        fn cc_remove_token_b() -> ChainedCall {
            let mut vault_b_auth = AccountForTests::vault_b_init();
            vault_b_auth.is_authorized = true;

            let mut instruction: [u8; 23] = [0; 23];
            instruction[0] = 1;
            instruction[1..17]
                .copy_from_slice(&BalanceForTests::remove_min_amount_b_low().to_le_bytes());
            let instruction_data = risc0_zkvm::serde::to_vec(&instruction)
                .expect("AMM Program expects valid transaction instruction data");
            ChainedCall {
                program_id: TOKEN_PROGRAM_ID,
                instruction_data,
                pre_states: vec![vault_b_auth, AccountForTests::user_holding_b()],
                pda_seeds: vec![compute_vault_pda_seed(
                    IdForTests::pool_definition_id(),
                    IdForTests::token_b_definition_id(),
                )],
            }
        }

        fn cc_remove_pool_lp() -> ChainedCall {
            let mut pool_lp_auth = AccountForTests::pool_lp_init();
            pool_lp_auth.is_authorized = true;

            let mut instruction: [u8; 23] = [0; 23];
            instruction[0] = 3;
            instruction[1..17]
                .copy_from_slice(&BalanceForTests::remove_actual_a_successful().to_le_bytes());
            let instruction_data = risc0_zkvm::serde::to_vec(&instruction)
                .expect("AMM Program expects valid transaction instruction data");
            ChainedCall {
                program_id: TOKEN_PROGRAM_ID,
                instruction_data,
                pre_states: vec![
                    AccountForTests::pool_lp_init(),
                    AccountForTests::user_holding_lp_init(),
                ],
                pda_seeds: vec![compute_liquidity_token_pda_seed(
                    IdForTests::pool_definition_id(),
                )],
            }
        }

        fn cc_new_definition_token_a() -> ChainedCall {
            let mut instruction: [u8; 23] = [0; 23];
            instruction[0] = 1;
            instruction[1..17]
                .copy_from_slice(&BalanceForTests::add_successful_amount_a().to_le_bytes());
            let instruction_data = risc0_zkvm::serde::to_vec(&instruction)
                .expect("AMM Program expects valid transaction instruction data");
            ChainedCall {
                program_id: TOKEN_PROGRAM_ID,
                instruction_data,
                pre_states: vec![
                    AccountForTests::user_holding_a(),
                    AccountForTests::vault_a_init(),
                ],
                pda_seeds: Vec::<PdaSeed>::new(),
            }
        }

        fn cc_new_definition_token_b() -> ChainedCall {
            let mut instruction: [u8; 23] = [0; 23];
            instruction[0] = 1;
            instruction[1..17]
                .copy_from_slice(&BalanceForTests::add_successful_amount_b().to_le_bytes());
            let instruction_data = risc0_zkvm::serde::to_vec(&instruction)
                .expect("Swap Logic: AMM Program expects valid transaction instruction data");
            ChainedCall {
                program_id: TOKEN_PROGRAM_ID,
                instruction_data,
                pre_states: vec![
                    AccountForTests::user_holding_b(),
                    AccountForTests::vault_b_init(),
                ],
                pda_seeds: Vec::<PdaSeed>::new(),
            }
        }

        fn cc_new_definition_token_lp() -> ChainedCall {
            let mut instruction: [u8; 23] = [0; 23];
            instruction[0] = 1;
            instruction[1..17]
                .copy_from_slice(&BalanceForTests::add_successful_amount_a().to_le_bytes());
            let instruction_data = risc0_zkvm::serde::to_vec(&instruction)
                .expect("AMM Program expects valid transaction instruction data");
            ChainedCall {
                program_id: TOKEN_PROGRAM_ID,
                instruction_data,
                pre_states: vec![
                    AccountForTests::pool_lp_init(),
                    AccountForTests::user_holding_lp_uninit(),
                ],
                pda_seeds: vec![compute_liquidity_token_pda_seed(
                    IdForTests::pool_definition_id(),
                )],
            }
        }
    }

    struct IdForTests;

    impl IdForTests {
        fn token_a_definition_id() -> AccountId {
            AccountId::new([42; 32])
        }

        fn token_b_definition_id() -> AccountId {
            AccountId::new([43; 32])
        }

        fn token_lp_definition_id() -> AccountId {
            compute_liquidity_token_pda(AMM_PROGRAM_ID, IdForTests::pool_definition_id())
        }

        fn user_token_a_id() -> AccountId {
            AccountId::new([45; 32])
        }

        fn user_token_b_id() -> AccountId {
            AccountId::new([46; 32])
        }

        fn user_token_lp_id() -> AccountId {
            AccountId::new([47; 32])
        }

        fn pool_definition_id() -> AccountId {
            compute_pool_pda(
                AMM_PROGRAM_ID,
                IdForTests::token_a_definition_id(),
                IdForTests::token_b_definition_id(),
            )
        }

        fn vault_a_id() -> AccountId {
            compute_vault_pda(
                AMM_PROGRAM_ID,
                IdForTests::pool_definition_id(),
                IdForTests::token_a_definition_id(),
            )
        }

        fn vault_b_id() -> AccountId {
            compute_vault_pda(
                AMM_PROGRAM_ID,
                IdForTests::pool_definition_id(),
                IdForTests::token_b_definition_id(),
            )
        }
    }

    struct AccountForTests;

    impl AccountForTests {
        fn user_holding_a() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: 1u8,
                        definition_id: IdForTests::token_a_definition_id(),
                        balance: BalanceForTests::user_token_a_balance(),
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::user_token_a_id(),
            }
        }

        fn user_holding_b() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: 1u8,
                        definition_id: IdForTests::token_b_definition_id(),
                        balance: BalanceForTests::user_token_b_balance(),
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::user_token_b_id(),
            }
        }

        fn vault_a_uninit() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: 1u8,
                        definition_id: IdForTests::token_a_definition_id(),
                        balance: 0,
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::vault_a_id(),
            }
        }

        fn vault_b_uninit() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: 1u8,
                        definition_id: IdForTests::token_b_definition_id(),
                        balance: 0,
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::vault_b_id(),
            }
        }

        fn vault_a_init() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: 1u8,
                        definition_id: IdForTests::token_a_definition_id(),
                        balance: BalanceForTests::vault_a_reserve_init(),
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::vault_a_id(),
            }
        }

        fn vault_b_init() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: 1u8,
                        definition_id: IdForTests::token_b_definition_id(),
                        balance: BalanceForTests::vault_b_reserve_init(),
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::vault_b_id(),
            }
        }

        fn vault_a_init_high() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: 1u8,
                        definition_id: IdForTests::token_a_definition_id(),
                        balance: BalanceForTests::vault_a_reserve_high(),
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::vault_a_id(),
            }
        }

        fn vault_b_init_high() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: 1u8,
                        definition_id: IdForTests::token_b_definition_id(),
                        balance: BalanceForTests::vault_b_reserve_high(),
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::vault_b_id(),
            }
        }

        fn vault_a_init_low() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: 1u8,
                        definition_id: IdForTests::token_a_definition_id(),
                        balance: BalanceForTests::vault_a_reserve_low(),
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::vault_a_id(),
            }
        }

        fn vault_b_init_low() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: 1u8,
                        definition_id: IdForTests::token_b_definition_id(),
                        balance: BalanceForTests::vault_b_reserve_low(),
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::vault_b_id(),
            }
        }

        fn vault_a_init_zero() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: 1u8,
                        definition_id: IdForTests::token_a_definition_id(),
                        balance: 0,
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::vault_a_id(),
            }
        }

        fn vault_b_init_zero() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: 1u8,
                        definition_id: IdForTests::token_b_definition_id(),
                        balance: 0,
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::vault_b_id(),
            }
        }

        fn vault_a_wrong_id() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: 1u8,
                        definition_id: IdForTests::token_a_definition_id(),
                        balance: BalanceForTests::vault_a_reserve_init(),
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::vault_b_id(),
            }
        }

        fn vault_b_wrong_id() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: 1u8,
                        definition_id: IdForTests::token_b_definition_id(),
                        balance: BalanceForTests::vault_b_reserve_init(),
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::vault_a_id(),
            }
        }

        fn pool_lp_uninit() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenDefinition::into_data(TokenDefinition {
                        account_type: 0u8,
                        name: [1; 6],
                        total_supply: 0u128,
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::token_lp_definition_id(),
            }
        }

        fn pool_lp_init() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenDefinition::into_data(TokenDefinition {
                        account_type: 0u8,
                        name: [1; 6],
                        total_supply: BalanceForTests::vault_a_reserve_init(),
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::token_lp_definition_id(),
            }
        }

        fn pool_lp_with_wrong_id() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenDefinition::into_data(TokenDefinition {
                        account_type: 0u8,
                        name: [1; 6],
                        total_supply: BalanceForTests::vault_a_reserve_init(),
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::vault_a_id(),
            }
        }

        fn user_holding_lp_uninit() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: 1u8,
                        definition_id: IdForTests::token_lp_definition_id(),
                        balance: 0,
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::user_token_lp_id(),
            }
        }

        fn user_holding_lp_init() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: 1u8,
                        definition_id: IdForTests::token_lp_definition_id(),
                        balance: BalanceForTests::user_token_lp_balance(),
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::user_token_lp_id(),
            }
        }

        fn pool_definition_uninit() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account::default(),
                is_authorized: true,
                account_id: IdForTests::pool_definition_id(),
            }
        }

        fn pool_definition_init() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: ProgramId::default(),
                    balance: 0u128,
                    data: PoolDefinition::into_data(PoolDefinition {
                        definition_token_a_id: IdForTests::token_a_definition_id(),
                        definition_token_b_id: IdForTests::token_b_definition_id(),
                        vault_a_id: IdForTests::vault_a_id(),
                        vault_b_id: IdForTests::vault_b_id(),
                        liquidity_pool_id: IdForTests::token_lp_definition_id(),
                        liquidity_pool_supply: BalanceForTests::vault_a_reserve_init(),
                        reserve_a: BalanceForTests::vault_a_reserve_init(),
                        reserve_b: BalanceForTests::vault_b_reserve_init(),
                        fees: 0u128,
                        active: true,
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::pool_definition_id(),
            }
        }

        fn pool_definition_init_reserve_a_zero() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: ProgramId::default(),
                    balance: 0u128,
                    data: PoolDefinition::into_data(PoolDefinition {
                        definition_token_a_id: IdForTests::token_a_definition_id(),
                        definition_token_b_id: IdForTests::token_b_definition_id(),
                        vault_a_id: IdForTests::vault_a_id(),
                        vault_b_id: IdForTests::vault_b_id(),
                        liquidity_pool_id: IdForTests::token_lp_definition_id(),
                        liquidity_pool_supply: BalanceForTests::vault_a_reserve_init(),
                        reserve_a: 0,
                        reserve_b: BalanceForTests::vault_b_reserve_init(),
                        fees: 0u128,
                        active: true,
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::pool_definition_id(),
            }
        }

        fn pool_definition_init_reserve_b_zero() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: ProgramId::default(),
                    balance: 0u128,
                    data: PoolDefinition::into_data(PoolDefinition {
                        definition_token_a_id: IdForTests::token_a_definition_id(),
                        definition_token_b_id: IdForTests::token_b_definition_id(),
                        vault_a_id: IdForTests::vault_a_id(),
                        vault_b_id: IdForTests::vault_b_id(),
                        liquidity_pool_id: IdForTests::token_lp_definition_id(),
                        liquidity_pool_supply: BalanceForTests::vault_a_reserve_init(),
                        reserve_a: BalanceForTests::vault_a_reserve_init(),
                        reserve_b: 0,
                        fees: 0u128,
                        active: true,
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::pool_definition_id(),
            }
        }

        fn pool_definition_init_reserve_a_low() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: ProgramId::default(),
                    balance: 0u128,
                    data: PoolDefinition::into_data(PoolDefinition {
                        definition_token_a_id: IdForTests::token_a_definition_id(),
                        definition_token_b_id: IdForTests::token_b_definition_id(),
                        vault_a_id: IdForTests::vault_a_id(),
                        vault_b_id: IdForTests::vault_b_id(),
                        liquidity_pool_id: IdForTests::token_lp_definition_id(),
                        liquidity_pool_supply: BalanceForTests::vault_a_reserve_low(),
                        reserve_a: BalanceForTests::vault_a_reserve_low(),
                        reserve_b: BalanceForTests::vault_b_reserve_high(),
                        fees: 0u128,
                        active: true,
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::pool_definition_id(),
            }
        }

        fn pool_definition_init_reserve_b_low() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: ProgramId::default(),
                    balance: 0u128,
                    data: PoolDefinition::into_data(PoolDefinition {
                        definition_token_a_id: IdForTests::token_a_definition_id(),
                        definition_token_b_id: IdForTests::token_b_definition_id(),
                        vault_a_id: IdForTests::vault_a_id(),
                        vault_b_id: IdForTests::vault_b_id(),
                        liquidity_pool_id: IdForTests::token_lp_definition_id(),
                        liquidity_pool_supply: BalanceForTests::vault_a_reserve_high(),
                        reserve_a: BalanceForTests::vault_a_reserve_high(),
                        reserve_b: BalanceForTests::vault_b_reserve_low(),
                        fees: 0u128,
                        active: true,
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::pool_definition_id(),
            }
        }

        fn pool_definition_unauth() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: ProgramId::default(),
                    balance: 0u128,
                    data: PoolDefinition::into_data(PoolDefinition {
                        definition_token_a_id: IdForTests::token_a_definition_id(),
                        definition_token_b_id: IdForTests::token_b_definition_id(),
                        vault_a_id: IdForTests::vault_a_id(),
                        vault_b_id: IdForTests::vault_b_id(),
                        liquidity_pool_id: IdForTests::token_lp_definition_id(),
                        liquidity_pool_supply: BalanceForTests::vault_a_reserve_init(),
                        reserve_a: BalanceForTests::vault_a_reserve_init(),
                        reserve_b: BalanceForTests::vault_b_reserve_init(),
                        fees: 0u128,
                        active: true,
                    }),
                    nonce: 0,
                },
                is_authorized: false,
                account_id: IdForTests::pool_definition_id(),
            }
        }

        fn pool_definition_swap_test_1() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: ProgramId::default(),
                    balance: 0u128,
                    data: PoolDefinition::into_data(PoolDefinition {
                        definition_token_a_id: IdForTests::token_a_definition_id(),
                        definition_token_b_id: IdForTests::token_b_definition_id(),
                        vault_a_id: IdForTests::vault_a_id(),
                        vault_b_id: IdForTests::vault_b_id(),
                        liquidity_pool_id: IdForTests::token_lp_definition_id(),
                        liquidity_pool_supply: BalanceForTests::vault_a_reserve_init(),
                        reserve_a: BalanceForTests::vault_a_swap_test_1(),
                        reserve_b: BalanceForTests::vault_b_swap_test_1(),
                        fees: 0u128,
                        active: true,
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::pool_definition_id(),
            }
        }

        fn pool_definition_swap_test_2() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: ProgramId::default(),
                    balance: 0u128,
                    data: PoolDefinition::into_data(PoolDefinition {
                        definition_token_a_id: IdForTests::token_a_definition_id(),
                        definition_token_b_id: IdForTests::token_b_definition_id(),
                        vault_a_id: IdForTests::vault_a_id(),
                        vault_b_id: IdForTests::vault_b_id(),
                        liquidity_pool_id: IdForTests::token_lp_definition_id(),
                        liquidity_pool_supply: BalanceForTests::vault_a_reserve_init(),
                        reserve_a: BalanceForTests::vault_a_swap_test_2(),
                        reserve_b: BalanceForTests::vault_b_swap_test_2(),
                        fees: 0u128,
                        active: true,
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::pool_definition_id(),
            }
        }

        fn pool_definition_add_zero_lp() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: ProgramId::default(),
                    balance: 0u128,
                    data: PoolDefinition::into_data(PoolDefinition {
                        definition_token_a_id: IdForTests::token_a_definition_id(),
                        definition_token_b_id: IdForTests::token_b_definition_id(),
                        vault_a_id: IdForTests::vault_a_id(),
                        vault_b_id: IdForTests::vault_b_id(),
                        liquidity_pool_id: IdForTests::token_lp_definition_id(),
                        liquidity_pool_supply: BalanceForTests::vault_a_reserve_low(),
                        reserve_a: BalanceForTests::vault_a_reserve_init(),
                        reserve_b: BalanceForTests::vault_b_reserve_init(),
                        fees: 0u128,
                        active: true,
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::pool_definition_id(),
            }
        }

        fn pool_definition_add_successful() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: ProgramId::default(),
                    balance: 0u128,
                    data: PoolDefinition::into_data(PoolDefinition {
                        definition_token_a_id: IdForTests::token_a_definition_id(),
                        definition_token_b_id: IdForTests::token_b_definition_id(),
                        vault_a_id: IdForTests::vault_a_id(),
                        vault_b_id: IdForTests::vault_b_id(),
                        liquidity_pool_id: IdForTests::token_lp_definition_id(),
                        liquidity_pool_supply: BalanceForTests::vault_a_add_successful(),
                        reserve_a: BalanceForTests::vault_a_add_successful(),
                        reserve_b: BalanceForTests::vault_b_add_successful(),
                        fees: 0u128,
                        active: true,
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::pool_definition_id(),
            }
        }

        fn pool_definition_remove_successful() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: ProgramId::default(),
                    balance: 0u128,
                    data: PoolDefinition::into_data(PoolDefinition {
                        definition_token_a_id: IdForTests::token_a_definition_id(),
                        definition_token_b_id: IdForTests::token_b_definition_id(),
                        vault_a_id: IdForTests::vault_a_id(),
                        vault_b_id: IdForTests::vault_b_id(),
                        liquidity_pool_id: IdForTests::token_lp_definition_id(),
                        liquidity_pool_supply: BalanceForTests::vault_a_remove_successful(),
                        reserve_a: BalanceForTests::vault_a_remove_successful(),
                        reserve_b: BalanceForTests::vault_b_remove_successful(),
                        fees: 0u128,
                        active: true,
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::pool_definition_id(),
            }
        }

        fn pool_definition_inactive() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: ProgramId::default(),
                    balance: 0u128,
                    data: PoolDefinition::into_data(PoolDefinition {
                        definition_token_a_id: IdForTests::token_a_definition_id(),
                        definition_token_b_id: IdForTests::token_b_definition_id(),
                        vault_a_id: IdForTests::vault_a_id(),
                        vault_b_id: IdForTests::vault_b_id(),
                        liquidity_pool_id: IdForTests::token_lp_definition_id(),
                        liquidity_pool_supply: BalanceForTests::vault_a_reserve_init(),
                        reserve_a: BalanceForTests::vault_a_reserve_init(),
                        reserve_b: BalanceForTests::vault_b_reserve_init(),
                        fees: 0u128,
                        active: false,
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::pool_definition_id(),
            }
        }

        fn pool_definition_with_wrong_id() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: ProgramId::default(),
                    balance: 0u128,
                    data: PoolDefinition::into_data(PoolDefinition {
                        definition_token_a_id: IdForTests::token_a_definition_id(),
                        definition_token_b_id: IdForTests::token_b_definition_id(),
                        vault_a_id: IdForTests::vault_a_id(),
                        vault_b_id: IdForTests::vault_b_id(),
                        liquidity_pool_id: IdForTests::token_lp_definition_id(),
                        liquidity_pool_supply: BalanceForTests::vault_a_reserve_init(),
                        reserve_a: BalanceForTests::vault_a_reserve_init(),
                        reserve_b: BalanceForTests::vault_b_reserve_init(),
                        fees: 0u128,
                        active: false,
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: AccountId::new([4; 32]),
            }
        }

        fn vault_a_with_wrong_id() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: 1u8,
                        definition_id: IdForTests::token_a_definition_id(),
                        balance: BalanceForTests::vault_a_reserve_init(),
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: AccountId::new([4; 32]),
            }
        }

        fn vault_b_with_wrong_id() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: TOKEN_PROGRAM_ID,
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: 1u8,
                        definition_id: IdForTests::token_b_definition_id(),
                        balance: BalanceForTests::vault_b_reserve_init(),
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: AccountId::new([4; 32]),
            }
        }

        fn pool_definition_active() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: ProgramId::default(),
                    balance: 0u128,
                    data: PoolDefinition::into_data(PoolDefinition {
                        definition_token_a_id: IdForTests::token_a_definition_id(),
                        definition_token_b_id: IdForTests::token_b_definition_id(),
                        vault_a_id: IdForTests::vault_a_id(),
                        vault_b_id: IdForTests::vault_b_id(),
                        liquidity_pool_id: IdForTests::token_lp_definition_id(),
                        liquidity_pool_supply: BalanceForTests::vault_a_reserve_init(),
                        reserve_a: BalanceForTests::vault_a_reserve_init(),
                        reserve_b: BalanceForTests::vault_b_reserve_init(),
                        fees: 0u128,
                        active: true,
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::pool_definition_id(),
            }
        }
    }

    #[test]
    fn test_pool_pda_produces_unique_id_for_token_pair() {
        //compute_pool_pda(amm_program_id: ProgramId, definition_token_a_id: AccountId, definition_token_b_id: AccountId)
        assert!(
            compute_pool_pda(
                AMM_PROGRAM_ID,
                IdForTests::token_a_definition_id(),
                IdForTests::token_b_definition_id()
            ) == compute_pool_pda(
                AMM_PROGRAM_ID,
                IdForTests::token_b_definition_id(),
                IdForTests::token_a_definition_id()
            )
        );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_new_definition_with_invalid_number_of_accounts_1() {
        let pre_states = vec![AccountForTests::pool_definition_uninit()];
        let _post_states = new_definition(
            &pre_states,
            &[
                BalanceForTests::vault_a_reserve_init(),
                BalanceForTests::vault_b_reserve_init(),
            ],
            AMM_PROGRAM_ID,
        );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_new_definition_with_invalid_number_of_accounts_2() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
        ];
        let _post_states = new_definition(
            &pre_states,
            &[
                BalanceForTests::vault_a_reserve_init(),
                BalanceForTests::vault_b_reserve_init(),
            ],
            AMM_PROGRAM_ID,
        );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_new_definition__with_invalid_number_of_accounts_3() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
        ];
        let _post_states = new_definition(
            &pre_states,
            &[
                BalanceForTests::vault_a_reserve_init(),
                BalanceForTests::vault_b_reserve_init(),
            ],
            AMM_PROGRAM_ID,
        );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_new_definition__with_invalid_number_of_accounts_4() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::pool_lp_init(),
        ];
        let _post_states = new_definition(
            &pre_states,
            &[
                BalanceForTests::vault_a_reserve_init(),
                BalanceForTests::vault_b_reserve_init(),
            ],
            AMM_PROGRAM_ID,
        );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_new_definition__with_invalid_number_of_accounts_5() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::pool_lp_init(),
            AccountForTests::user_holding_a(),
        ];
        let _post_states = new_definition(
            &pre_states,
            &[
                BalanceForTests::vault_a_reserve_init(),
                BalanceForTests::vault_b_reserve_init(),
            ],
            AMM_PROGRAM_ID,
        );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_new_definition_with_invalid_number_of_accounts_6() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::pool_lp_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
        ];
        let _post_states = new_definition(
            &pre_states,
            &[
                BalanceForTests::vault_a_reserve_init(),
                BalanceForTests::vault_b_reserve_init(),
            ],
            AMM_PROGRAM_ID,
        );
    }

    #[should_panic(expected = "Invalid number of input balances")]
    #[test]
    fn test_call_new_definition_with_invalid_number_of_balances() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::pool_lp_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
            AccountForTests::user_holding_lp_uninit(),
        ];
        let _post_states = new_definition(
            &pre_states,
            &[BalanceForTests::vault_a_reserve_init()],
            AMM_PROGRAM_ID,
        );
    }

    #[should_panic(expected = "Balances must be nonzero")]
    #[test]
    fn test_call_new_definition_with_zero_balance_1() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::pool_lp_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
            AccountForTests::user_holding_lp_uninit(),
        ];
        let _post_states = new_definition(
            &pre_states,
            &[0, BalanceForTests::vault_b_reserve_init()],
            AMM_PROGRAM_ID,
        );
    }

    #[should_panic(expected = "Balances must be nonzero")]
    #[test]
    fn test_call_new_definition_with_zero_balance_2() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::pool_lp_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
            AccountForTests::user_holding_lp_uninit(),
        ];
        let _post_states = new_definition(
            &pre_states,
            &[BalanceForTests::vault_a_reserve_init(), 0],
            AMM_PROGRAM_ID,
        );
    }

    #[should_panic(expected = "Cannot set up a swap for a token with itself")]
    #[test]
    fn test_call_new_definition_same_token_definition() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::pool_lp_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_lp_uninit(),
        ];
        let _post_states = new_definition(
            &pre_states,
            &[
                BalanceForTests::vault_a_reserve_init(),
                BalanceForTests::vault_b_reserve_init(),
            ],
            AMM_PROGRAM_ID,
        );
    }

    #[should_panic(expected = "Liquidity pool Token Definition Account ID does not match PDA")]
    #[test]
    fn test_call_new_definition_wrong_liquidity_id() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::pool_lp_with_wrong_id(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
            AccountForTests::user_holding_lp_uninit(),
        ];
        let _post_states = new_definition(
            &pre_states,
            &[
                BalanceForTests::vault_a_reserve_init(),
                BalanceForTests::vault_b_reserve_init(),
            ],
            AMM_PROGRAM_ID,
        );
    }

    #[should_panic(expected = "Pool Definition Account ID does not match PDA")]
    #[test]
    fn test_call_new_definition_wrong_pool_id() {
        let pre_states = vec![
            AccountForTests::pool_definition_with_wrong_id(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::pool_lp_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
            AccountForTests::user_holding_lp_uninit(),
        ];
        let _post_states = new_definition(
            &pre_states,
            &[
                BalanceForTests::vault_a_reserve_init(),
                BalanceForTests::vault_b_reserve_init(),
            ],
            AMM_PROGRAM_ID,
        );
    }

    #[should_panic(expected = "Vault ID does not match PDA")]
    #[test]
    fn test_call_new_definition_wrong_vault_id_1() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_with_wrong_id(),
            AccountForTests::vault_b_init(),
            AccountForTests::pool_lp_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
            AccountForTests::user_holding_lp_uninit(),
        ];
        let _post_states = new_definition(
            &pre_states,
            &[
                BalanceForTests::vault_a_reserve_init(),
                BalanceForTests::vault_b_reserve_init(),
            ],
            AMM_PROGRAM_ID,
        );
    }

    #[should_panic(expected = "Vault ID does not match PDA")]
    #[test]
    fn test_call_new_definition_wrong_vault_id_2() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_with_wrong_id(),
            AccountForTests::pool_lp_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
            AccountForTests::user_holding_lp_uninit(),
        ];
        let _post_states = new_definition(
            &pre_states,
            &[
                BalanceForTests::vault_a_reserve_init(),
                BalanceForTests::vault_b_reserve_init(),
            ],
            AMM_PROGRAM_ID,
        );
    }

    #[should_panic(expected = "Cannot initialize an active Pool Definition")]
    #[test]
    fn test_call_new_definition_cannot_initialize_active_pool() {
        let pre_states = vec![
            AccountForTests::pool_definition_active(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::pool_lp_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
            AccountForTests::user_holding_lp_uninit(),
        ];
        let _post_states = new_definition(
            &pre_states,
            &[
                BalanceForTests::vault_a_reserve_init(),
                BalanceForTests::vault_b_reserve_init(),
            ],
            AMM_PROGRAM_ID,
        );
    }

    #[should_panic(expected = "Cannot initialize an active Pool Definition")]
    #[test]
    fn test_call_new_definition_chained_call_successful() {
        let pre_states = vec![
            AccountForTests::pool_definition_active(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::pool_lp_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
            AccountForTests::user_holding_lp_uninit(),
        ];
        let (post_states, chained_calls) = new_definition(
            &pre_states,
            &[
                BalanceForTests::vault_a_reserve_init(),
                BalanceForTests::vault_b_reserve_init(),
            ],
            AMM_PROGRAM_ID,
        );

        let pool_post = post_states[0].clone();

        assert!(AccountForTests::pool_definition_add_successful().account == *pool_post.account());

        let chained_call_lp = chained_calls[0].clone();
        let chained_call_b = chained_calls[1].clone();
        let chained_call_a = chained_calls[2].clone();

        assert!(chained_call_a == ChainedCallForTests::cc_new_definition_token_a());
        assert!(chained_call_b == ChainedCallForTests::cc_new_definition_token_b());
        assert!(chained_call_lp == ChainedCallForTests::cc_new_definition_token_lp());
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_remove_liquidity_with_invalid_number_of_accounts_2() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
        ];
        let _post_states = remove_liquidity(
            &pre_states,
            &[
                BalanceForTests::remove_amount_lp(),
                BalanceForTests::remove_min_amount_a(),
                BalanceForTests::remove_min_amount_b(),
            ],
        );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_remove_liquidity_with_invalid_number_of_accounts_3() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
        ];
        let _post_states = remove_liquidity(
            &pre_states,
            &[
                BalanceForTests::remove_amount_lp(),
                BalanceForTests::remove_min_amount_a(),
                BalanceForTests::remove_min_amount_b(),
            ],
        );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_remove_liquidity_with_invalid_number_of_accounts_4() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::pool_lp_init(),
        ];
        let _post_states = remove_liquidity(
            &pre_states,
            &[
                BalanceForTests::remove_amount_lp(),
                BalanceForTests::remove_min_amount_a(),
                BalanceForTests::remove_min_amount_b(),
            ],
        );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_remove_liquidity_with_invalid_number_of_accounts_5() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::pool_lp_init(),
            AccountForTests::user_holding_a(),
        ];
        let _post_states = remove_liquidity(
            &pre_states,
            &[
                BalanceForTests::remove_amount_lp(),
                BalanceForTests::remove_min_amount_a(),
                BalanceForTests::remove_min_amount_b(),
            ],
        );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_remove_liquidity_with_invalid_number_of_accounts_6() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::pool_lp_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
        ];
        let _post_states = remove_liquidity(
            &pre_states,
            &[
                BalanceForTests::remove_amount_lp(),
                BalanceForTests::remove_min_amount_a(),
                BalanceForTests::remove_min_amount_b(),
            ],
        );
    }

    #[should_panic(expected = "Vault A was not provided")]
    #[test]
    fn test_call_remove_liquidity_vault_a_omitted() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_with_wrong_id(),
            AccountForTests::vault_b_init(),
            AccountForTests::pool_lp_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
            AccountForTests::user_holding_lp_init(),
        ];
        let _post_states = remove_liquidity(
            &pre_states,
            &[
                BalanceForTests::remove_amount_lp(),
                BalanceForTests::remove_min_amount_a(),
                BalanceForTests::remove_min_amount_b(),
            ],
        );
    }

    #[should_panic(expected = "Vault B was not provided")]
    #[test]
    fn test_call_remove_liquidity_vault_b_omitted() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_with_wrong_id(),
            AccountForTests::pool_lp_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
            AccountForTests::user_holding_lp_init(),
        ];
        let _post_states = remove_liquidity(
            &pre_states,
            &[
                BalanceForTests::remove_amount_lp(),
                BalanceForTests::remove_min_amount_a(),
                BalanceForTests::remove_min_amount_b(),
            ],
        );
    }

    #[should_panic(expected = "LP definition mismatch")]
    #[test]
    fn test_call_remove_liquidity_lp_def_mismatch() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::pool_lp_with_wrong_id(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
            AccountForTests::user_holding_lp_init(),
        ];
        let _post_states = remove_liquidity(
            &pre_states,
            &[
                BalanceForTests::remove_amount_lp(),
                BalanceForTests::remove_min_amount_a(),
                BalanceForTests::remove_min_amount_b(),
            ],
        );
    }

    #[should_panic(expected = "Invalid liquidity account provided")]
    #[test]
    fn test_call_remove_liquidity_insufficient_liquidity_amount() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::pool_lp_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
            AccountForTests::user_holding_a(), //different token account than lp to create desired error
        ];
        let _post_states = remove_liquidity(
            &pre_states,
            &[
                BalanceForTests::remove_amount_lp(),
                BalanceForTests::remove_min_amount_a(),
                BalanceForTests::remove_min_amount_b(),
            ],
        );
    }

    #[should_panic(
        expected = "Insufficient minimal withdraw amount (Token A) provided for liquidity amount"
    )]
    #[test]
    fn test_call_remove_liquidity_insufficient_balance_1() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::pool_lp_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
            AccountForTests::user_holding_lp_init(),
        ];
        let _post_states = remove_liquidity(
            &pre_states,
            &[
                BalanceForTests::remove_amount_lp_1(),
                BalanceForTests::remove_min_amount_a(),
                BalanceForTests::remove_min_amount_b(),
            ],
        );
    }

    #[should_panic(
        expected = "Insufficient minimal withdraw amount (Token B) provided for liquidity amount"
    )]
    #[test]
    fn test_call_remove_liquidity_insufficient_balance_2() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::pool_lp_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
            AccountForTests::user_holding_lp_init(),
        ];
        let _post_states = remove_liquidity(
            &pre_states,
            &[
                BalanceForTests::remove_amount_lp(),
                BalanceForTests::remove_min_amount_a(),
                BalanceForTests::remove_min_amount_b(),
            ],
        );
    }

    #[should_panic(expected = "Minimum withdraw amount must be nonzero")]
    #[test]
    fn test_call_remove_liquidity_min_bal_zero_1() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::pool_lp_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
            AccountForTests::user_holding_lp_init(),
        ];
        let _post_states = remove_liquidity(
            &pre_states,
            &[
                BalanceForTests::remove_amount_lp(),
                0,
                BalanceForTests::remove_min_amount_b(),
            ],
        );
    }

    #[should_panic(expected = "Minimum withdraw amount must be nonzero")]
    #[test]
    fn test_call_remove_liquidity_min_bal_zero_2() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::pool_lp_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
            AccountForTests::user_holding_lp_init(),
        ];
        let _post_states = remove_liquidity(
            &pre_states,
            &[
                BalanceForTests::remove_amount_lp(),
                BalanceForTests::remove_min_amount_a(),
                0,
            ],
        );
    }

    #[should_panic(expected = "Liquidity amount must be nonzero")]
    #[test]
    fn test_call_remove_liquidity_lp_bal_zero() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::pool_lp_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
            AccountForTests::user_holding_lp_init(),
        ];
        let _post_states = remove_liquidity(
            &pre_states,
            &[
                0,
                BalanceForTests::remove_min_amount_a(),
                BalanceForTests::remove_min_amount_b(),
            ],
        );
    }

    #[test]
    fn test_call_remove_liquidity_chained_call_successful() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::pool_lp_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
            AccountForTests::user_holding_lp_init(),
        ];
        let (post_states, chained_calls) = remove_liquidity(
            &pre_states,
            &[
                BalanceForTests::remove_amount_lp(),
                BalanceForTests::remove_min_amount_a(),
                BalanceForTests::remove_min_amount_b_low(),
            ],
        );

        let pool_post = post_states[0].clone();

        assert!(
            AccountForTests::pool_definition_remove_successful().account == *pool_post.account()
        );

        let chained_call_lp = chained_calls[0].clone();
        let chained_call_b = chained_calls[1].clone();
        let chained_call_a = chained_calls[2].clone();

        assert!(chained_call_a == ChainedCallForTests::cc_remove_token_a());
        assert!(chained_call_b == ChainedCallForTests::cc_remove_token_b());
        assert!(chained_call_lp == ChainedCallForTests::cc_remove_pool_lp());
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_add_liquidity_with_invalid_number_of_accounts_1() {
        let pre_states = vec![AccountForTests::pool_definition_init()];
        let _post_states = add_liquidity(
            &pre_states,
            &[
                BalanceForTests::add_min_amount_lp(),
                BalanceForTests::add_max_amount_a(),
                BalanceForTests::add_max_amount_b(),
            ],
        );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_add_liquidity_with_invalid_number_of_accounts_2() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
        ];
        let _post_states = add_liquidity(
            &pre_states,
            &[
                BalanceForTests::add_min_amount_lp(),
                BalanceForTests::add_max_amount_a(),
                BalanceForTests::add_max_amount_b(),
            ],
        );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_add_liquidity_with_invalid_number_of_accounts_3() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
        ];
        let _post_states = add_liquidity(
            &pre_states,
            &[
                BalanceForTests::add_min_amount_lp(),
                BalanceForTests::add_max_amount_a(),
                BalanceForTests::add_max_amount_b(),
            ],
        );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_add_liquidity_with_invalid_number_of_accounts_4() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::pool_lp_init(),
        ];
        let _post_states = add_liquidity(
            &pre_states,
            &[
                BalanceForTests::add_min_amount_lp(),
                BalanceForTests::add_max_amount_a(),
                BalanceForTests::add_max_amount_b(),
            ],
        );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_add_liquidity_with_invalid_number_of_accounts_5() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::pool_lp_init(),
            AccountForTests::user_holding_a(),
        ];
        let _post_states = add_liquidity(
            &pre_states,
            &[
                BalanceForTests::add_min_amount_lp(),
                BalanceForTests::add_max_amount_a(),
                BalanceForTests::add_max_amount_b(),
            ],
        );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_add_liquidity_with_invalid_number_of_accounts_6() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::pool_lp_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
        ];
        let _post_states = add_liquidity(
            &pre_states,
            &[
                BalanceForTests::add_min_amount_lp(),
                BalanceForTests::add_max_amount_a(),
                BalanceForTests::add_max_amount_b(),
            ],
        );
    }

    #[should_panic(expected = "Invalid number of input balances")]
    #[test]
    fn test_call_add_liquidity_invalid_number_of_balances_1() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::pool_lp_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
            AccountForTests::user_holding_lp_init(),
        ];
        let _post_states = add_liquidity(&pre_states, &[BalanceForTests::add_min_amount_lp()]);
    }

    #[should_panic(expected = "Invalid number of input balances")]
    #[test]
    fn test_call_add_liquidity_invalid_number_of_balances_2() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::pool_lp_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
            AccountForTests::user_holding_lp_init(),
        ];
        let _post_states = add_liquidity(
            &pre_states,
            &[
                BalanceForTests::add_min_amount_lp(),
                BalanceForTests::add_max_amount_a(),
            ],
        );
    }

    #[should_panic(expected = "Vault A was not provided")]
    #[test]
    fn test_call_add_liquidity_vault_a_omitted() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_with_wrong_id(),
            AccountForTests::vault_b_init(),
            AccountForTests::pool_lp_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
            AccountForTests::user_holding_lp_init(),
        ];
        let _post_states = add_liquidity(
            &pre_states,
            &[
                BalanceForTests::add_min_amount_lp(),
                BalanceForTests::add_max_amount_a(),
                BalanceForTests::add_max_amount_b(),
            ],
        );
    }

    #[should_panic(expected = "Vault B was not provided")]
    #[test]
    fn test_call_add_liquidity_vault_b_omitted() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_with_wrong_id(),
            AccountForTests::pool_lp_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
            AccountForTests::user_holding_lp_init(),
        ];
        let _post_states = add_liquidity(
            &pre_states,
            &[
                BalanceForTests::add_min_amount_lp(),
                BalanceForTests::add_max_amount_a(),
                BalanceForTests::add_max_amount_b(),
            ],
        );
    }

    #[should_panic(expected = "LP definition mismatch")]
    #[test]
    fn test_call_add_liquidity_lp_definition_mismatch() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::pool_lp_with_wrong_id(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
            AccountForTests::user_holding_lp_init(),
        ];
        let _post_states = add_liquidity(
            &pre_states,
            &[
                BalanceForTests::add_min_amount_lp(),
                BalanceForTests::add_max_amount_a(),
                BalanceForTests::add_max_amount_b(),
            ],
        );
    }

    #[should_panic(expected = "Both max-balances must be nonzero")]
    #[test]
    fn test_call_add_liquidity_zero_balance_1() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::pool_lp_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
            AccountForTests::user_holding_lp_init(),
        ];
        let _post_states = add_liquidity(
            &pre_states,
            &[
                BalanceForTests::add_min_amount_lp(),
                0,
                BalanceForTests::add_max_amount_b(),
            ],
        );
    }

    #[should_panic(expected = "Both max-balances must be nonzero")]
    #[test]
    fn test_call_add_liquidity_zero_balance_2() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::pool_lp_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
            AccountForTests::user_holding_lp_init(),
        ];
        let _post_states = add_liquidity(
            &pre_states,
            &[
                BalanceForTests::add_min_amount_lp(),
                0,
                BalanceForTests::add_max_amount_a(),
            ],
        );
    }

    #[should_panic(expected = "Min-lp must be nonzero")]
    #[test]
    fn test_call_add_liquidity_zero_min_lp() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::pool_lp_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
            AccountForTests::user_holding_lp_init(),
        ];
        let _post_states = add_liquidity(
            &pre_states,
            &[
                0,
                BalanceForTests::add_max_amount_a(),
                BalanceForTests::add_max_amount_b(),
            ],
        );
    }

    #[should_panic(expected = "Vaults' balances must be at least the reserve amounts")]
    #[test]
    fn test_call_add_liquidity_vault_insufficient_balance_1() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init_zero(),
            AccountForTests::vault_b_init(),
            AccountForTests::pool_lp_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
            AccountForTests::user_holding_lp_init(),
        ];
        let _post_states = add_liquidity(
            &pre_states,
            &[
                BalanceForTests::add_max_amount_a(),
                BalanceForTests::add_max_amount_b(),
                BalanceForTests::add_min_amount_lp(),
            ],
        );
    }

    #[should_panic(expected = "Vaults' balances must be at least the reserve amounts")]
    #[test]
    fn test_call_add_liquidity_vault_insufficient_balance_2() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init_zero(),
            AccountForTests::pool_lp_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
            AccountForTests::user_holding_lp_init(),
        ];
        let _post_states = add_liquidity(
            &pre_states,
            &[
                BalanceForTests::add_max_amount_a(),
                BalanceForTests::add_max_amount_b(),
                BalanceForTests::add_min_amount_lp(),
            ],
        );
    }

    #[should_panic(expected = "A trade amount is 0")]
    #[test]
    fn test_call_add_liquidity_actual_amount_zero_1() {
        let pre_states = vec![
            AccountForTests::pool_definition_init_reserve_a_low(),
            AccountForTests::vault_a_init_low(),
            AccountForTests::vault_b_init_high(),
            AccountForTests::pool_lp_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
            AccountForTests::user_holding_lp_init(),
        ];
        let _post_states = add_liquidity(
            &pre_states,
            &[
                BalanceForTests::add_min_amount_lp(),
                BalanceForTests::add_max_amount_a(),
                BalanceForTests::add_max_amount_b(),
            ],
        );
    }

    #[should_panic(expected = "A trade amount is 0")]
    #[test]
    fn test_call_add_liquidity_actual_amount_zero_2() {
        let pre_states = vec![
            AccountForTests::pool_definition_init_reserve_b_low(),
            AccountForTests::vault_a_init_high(),
            AccountForTests::vault_b_init_low(),
            AccountForTests::pool_lp_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
            AccountForTests::user_holding_lp_init(),
        ];
        let _post_states = add_liquidity(
            &pre_states,
            &[
                BalanceForTests::add_min_amount_lp(),
                BalanceForTests::add_max_amount_a_low(),
                BalanceForTests::add_max_amount_b_low(),
            ],
        );
    }

    #[should_panic(expected = "Reserves must be nonzero")]
    #[test]
    fn test_call_add_liquidity_reserves_zero_1() {
        let pre_states = vec![
            AccountForTests::pool_definition_init_reserve_a_zero(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::pool_lp_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
            AccountForTests::user_holding_lp_init(),
        ];
        let _post_states = add_liquidity(
            &pre_states,
            &[
                BalanceForTests::add_min_amount_lp(),
                BalanceForTests::add_max_amount_a(),
                BalanceForTests::add_max_amount_b(),
            ],
        );
    }

    #[should_panic(expected = "Reserves must be nonzero")]
    #[test]
    fn test_call_add_liquidity_reserves_zero_2() {
        let pre_states = vec![
            AccountForTests::pool_definition_init_reserve_b_zero(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::pool_lp_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
            AccountForTests::user_holding_lp_init(),
        ];
        let _post_states = add_liquidity(
            &pre_states,
            &[
                BalanceForTests::add_min_amount_lp(),
                BalanceForTests::add_max_amount_a(),
                BalanceForTests::add_max_amount_b(),
            ],
        );
    }

    #[should_panic(expected = "Payable LP must be nonzero")]
    #[test]
    fn test_call_add_liquidity_payable_lp_zero() {
        let pre_states = vec![
            AccountForTests::pool_definition_add_zero_lp(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::pool_lp_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
            AccountForTests::user_holding_lp_init(),
        ];
        let _post_states = add_liquidity(
            &pre_states,
            &[
                BalanceForTests::add_min_amount_lp(),
                BalanceForTests::add_max_amount_a_low(),
                BalanceForTests::add_max_amount_b_low(),
            ],
        );
    }

    #[test]
    fn test_call_add_liquidity_chained_call_successsful() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::pool_lp_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
            AccountForTests::user_holding_lp_init(),
        ];
        let (post_states, chained_calls) = add_liquidity(
            &pre_states,
            &[
                BalanceForTests::add_min_amount_lp(),
                BalanceForTests::add_max_amount_a(),
                BalanceForTests::add_max_amount_b(),
            ],
        );

        let pool_post = post_states[0].clone();

        assert!(AccountForTests::pool_definition_add_successful().account == *pool_post.account());

        let chained_call_lp = chained_calls[0].clone();
        let chained_call_b = chained_calls[1].clone();
        let chained_call_a = chained_calls[2].clone();

        assert!(chained_call_a == ChainedCallForTests::cc_add_token_a());
        assert!(chained_call_b == ChainedCallForTests::cc_add_token_b());
        assert!(chained_call_lp == ChainedCallForTests::cc_add_pool_lp());
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_swap_with_invalid_number_of_accounts_1() {
        let pre_states = vec![AccountForTests::pool_definition_init()];
        let _post_states = swap(
            &pre_states,
            &[
                BalanceForTests::add_max_amount_a(),
                BalanceForTests::min_amount_out(),
            ],
            IdForTests::token_a_definition_id(),
        );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_swap_with_invalid_number_of_accounts_2() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
        ];
        let _post_states = swap(
            &pre_states,
            &[
                BalanceForTests::add_max_amount_a(),
                BalanceForTests::min_amount_out(),
            ],
            IdForTests::token_a_definition_id(),
        );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_swap_with_invalid_number_of_accounts_3() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
        ];
        let _post_states = swap(
            &pre_states,
            &[
                BalanceForTests::add_max_amount_a(),
                BalanceForTests::min_amount_out(),
            ],
            IdForTests::token_a_definition_id(),
        );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_swap_with_invalid_number_of_accounts_4() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::user_holding_a(),
        ];
        let _post_states = swap(
            &pre_states,
            &[
                BalanceForTests::add_max_amount_a(),
                BalanceForTests::min_amount_out(),
            ],
            IdForTests::token_a_definition_id(),
        );
    }

    #[should_panic(expected = "Invalid number of amounts provided")]
    #[test]
    fn test_call_swap_with_invalid_number_of_amounts() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
        ];
        let _post_states = swap(
            &pre_states,
            &[BalanceForTests::add_max_amount_a()],
            IdForTests::token_a_definition_id(),
        );
    }

    #[should_panic(expected = "AccountId is not a token type for the pool")]
    #[test]
    fn test_call_swap_incorrect_token_type() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
        ];
        let _post_states = swap(
            &pre_states,
            &[
                BalanceForTests::add_max_amount_a(),
                BalanceForTests::min_amount_out(),
            ],
            IdForTests::token_lp_definition_id(),
        );
    }

    #[should_panic(expected = "Vault A was not provided")]
    #[test]
    fn test_call_swap_vault_a_omitted() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_with_wrong_id(),
            AccountForTests::vault_b_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
        ];
        let _post_states = swap(
            &pre_states,
            &[
                BalanceForTests::add_max_amount_a(),
                BalanceForTests::min_amount_out(),
            ],
            IdForTests::token_a_definition_id(),
        );
    }

    #[should_panic(expected = "Vault B was not provided")]
    #[test]
    fn test_call_swap_vault_b_omitted() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_with_wrong_id(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
        ];
        let _post_states = swap(
            &pre_states,
            &[
                BalanceForTests::add_max_amount_a(),
                BalanceForTests::min_amount_out(),
            ],
            IdForTests::token_a_definition_id(),
        );
    }

    #[should_panic(expected = "Reserve for Token A exceeds vault balance")]
    #[test]
    fn test_call_swap_reserves_vault_mismatch_1() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init_low(),
            AccountForTests::vault_b_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
        ];
        let _post_states = swap(
            &pre_states,
            &[
                BalanceForTests::add_max_amount_a(),
                BalanceForTests::min_amount_out(),
            ],
            IdForTests::token_a_definition_id(),
        );
    }

    #[should_panic(expected = "Reserve for Token B exceeds vault balance")]
    #[test]
    fn test_call_swap_reserves_vault_mismatch_2() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init_low(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
        ];
        let _post_states = swap(
            &pre_states,
            &[
                BalanceForTests::add_max_amount_a(),
                BalanceForTests::min_amount_out(),
            ],
            IdForTests::token_a_definition_id(),
        );
    }

    #[should_panic(expected = "Pool is inactive")]
    #[test]
    fn test_call_swap_ianctive() {
        let pre_states = vec![
            AccountForTests::pool_definition_inactive(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
        ];
        let _post_states = swap(
            &pre_states,
            &[
                BalanceForTests::add_max_amount_a(),
                BalanceForTests::min_amount_out(),
            ],
            IdForTests::token_a_definition_id(),
        );
    }

    #[should_panic(expected = "Withdraw amount is less than minimal amount out")]
    #[test]
    fn test_call_swap_below_min_out() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
        ];
        let _post_states = swap(
            &pre_states,
            &[
                BalanceForTests::add_max_amount_a(),
                BalanceForTests::min_amount_out(),
            ],
            IdForTests::token_a_definition_id(),
        );
    }

    #[test]
    fn test_call_swap_chained_call_successful_1() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
        ];
        let (post_states, chained_calls) = swap(
            &pre_states,
            &[
                BalanceForTests::add_max_amount_a(),
                BalanceForTests::add_max_amount_a_low(),
            ],
            IdForTests::token_a_definition_id(),
        );

        let pool_post = post_states[0].clone();

        assert!(AccountForTests::pool_definition_swap_test_1().account == *pool_post.account());

        let chained_call_a = chained_calls[0].clone();
        let chained_call_b = chained_calls[1].clone();

        assert!(chained_call_a == ChainedCallForTests::cc_swap_token_a_test_1());
        assert!(chained_call_b == ChainedCallForTests::cc_swap_token_b_test_1());
    }

    #[test]
    fn test_call_swap_chained_call_successful_2() {
        let pre_states = vec![
            AccountForTests::pool_definition_init(),
            AccountForTests::vault_a_init(),
            AccountForTests::vault_b_init(),
            AccountForTests::user_holding_a(),
            AccountForTests::user_holding_b(),
        ];
        let (post_states, chained_calls) = swap(
            &pre_states,
            &[
                BalanceForTests::add_max_amount_b(),
                BalanceForTests::min_amount_out(),
            ],
            IdForTests::token_b_definition_id(),
        );

        let pool_post = post_states[0].clone();

        assert!(AccountForTests::pool_definition_swap_test_2().account == *pool_post.account());

        let chained_call_a = chained_calls[1].clone();
        let chained_call_b = chained_calls[0].clone();

        assert!(chained_call_a == ChainedCallForTests::cc_swap_token_a_test_2());
        assert!(chained_call_b == ChainedCallForTests::cc_swap_token_b_test_2());
    }
}
