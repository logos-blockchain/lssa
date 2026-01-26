use amm_core::{
    PoolDefinition, compute_liquidity_token_pda, compute_liquidity_token_pda_seed,
    compute_pool_pda, compute_vault_pda,
};
use nssa_core::{
    account::{Account, AccountWithMetadata},
    program::{AccountPostState, ChainedCall, ProgramId},
};

#[expect(clippy::too_many_arguments, reason = "TODO: Fix later")]
pub fn new_definition(
    pool: AccountWithMetadata,
    vault_a: AccountWithMetadata,
    vault_b: AccountWithMetadata,
    pool_definition_lp: AccountWithMetadata,
    user_holding_a: AccountWithMetadata,
    user_holding_b: AccountWithMetadata,
    user_holding_lp: AccountWithMetadata,
    token_a_amount: NonZeroU128,
    token_b_amount: NonZeroU128,
    amm_program_id: ProgramId,
) -> (Vec<AccountPostState>, Vec<ChainedCall>) {
    // Prevents pool constant coefficient (k) from being 0.
    if token_a_amount == 0 || token_b_amount == 0 {
        panic!("Balances must be nonzero")
    }

    // Verify token_a and token_b are different
    let definition_token_a_id = token_core::TokenHolding::try_from(&user_holding_a.account.data)
        .expect("New definition: AMM Program expects valid Token Holding account for Token A")
        .definition_id();
    let definition_token_b_id = token_core::TokenHolding::try_from(&user_holding_b.account.data)
        .expect("New definition: AMM Program expects valid Token Holding account for Token B")
        .definition_id();

    // both instances of the same token program
    let token_program = user_holding_a.account.program_owner;

    if user_holding_b.account.program_owner != token_program {
        panic!("User Token holdings must use the same Token Program");
    }

    if definition_token_a_id == definition_token_b_id {
        panic!("Cannot set up a swap for a token with itself")
    }

    if pool.account_id
        != compute_pool_pda(amm_program_id, definition_token_a_id, definition_token_b_id)
    {
        panic!("Pool Definition Account ID does not match PDA");
    }

    if vault_a.account_id
        != compute_vault_pda(amm_program_id, pool.account_id, definition_token_a_id)
        || vault_b.account_id
            != compute_vault_pda(amm_program_id, pool.account_id, definition_token_b_id)
    {
        panic!("Vault ID does not match PDA");
    }

    if pool_definition_lp.account_id != compute_liquidity_token_pda(amm_program_id, pool.account_id)
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
        vault_a_id: vault_a.account_id,
        vault_b_id: vault_b.account_id,
        liquidity_pool_id: pool_definition_lp.account_id,
        liquidity_pool_supply: token_a_amount,
        reserve_a: token_a_amount,
        reserve_b: token_b_amount,
        fees: 0u128, // TODO: we assume all fees are 0 for now.
        active: true,
    };

    pool_post.data = pool_post_definition.into_data();
    let pool_post: AccountPostState = if pool.account == Account::default() {
        AccountPostState::new_claimed(pool_post.clone())
    } else {
        AccountPostState::new(pool_post.clone())
    };

    let token_program_id = user_holding_a.account.program_owner;

    // Chain call for Token A (user_holding_a -> Vault_A)
    let call_token_a = ChainedCall::new(
        token_program_id,
        vec![user_holding_a.clone(), vault_a.clone()],
        &token_core::Instruction::Transfer {
            amount_to_transfer: token_a_amount,
        },
    );
    // Chain call for Token B (user_holding_b -> Vault_B)
    let call_token_b = ChainedCall::new(
        token_program_id,
        vec![user_holding_b.clone(), vault_b.clone()],
        &token_core::Instruction::Transfer {
            amount_to_transfer: token_b_amount,
        },
    );

    // Chain call for liquidity token (TokenLP definition -> User LP Holding)
    let instruction = if pool.account == Account::default() {
        token_core::Instruction::NewFungibleDefinition {
            name: String::from("LP Token"),
            total_supply: token_a_amount,
        }
    } else {
        token_core::Instruction::Mint {
            amount_to_mint: token_a_amount,
        }
    };

    let mut pool_lp_auth = pool_definition_lp.clone();
    pool_lp_auth.is_authorized = true;

    let call_token_lp = ChainedCall::new(
        token_program_id,
        vec![pool_lp_auth.clone(), user_holding_lp.clone()],
        &instruction,
    )
    .with_pda_seeds(vec![compute_liquidity_token_pda_seed(pool.account_id)]);

    let chained_calls = vec![call_token_lp, call_token_b, call_token_a];

    let post_states = vec![
        pool_post.clone(),
        AccountPostState::new(vault_a.account.clone()),
        AccountPostState::new(vault_b.account.clone()),
        AccountPostState::new(pool_definition_lp.account.clone()),
        AccountPostState::new(user_holding_a.account.clone()),
        AccountPostState::new(user_holding_b.account.clone()),
        AccountPostState::new(user_holding_lp.account.clone()),
    ];

    (post_states.clone(), chained_calls)
}
