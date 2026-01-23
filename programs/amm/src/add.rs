use amm_core::{PoolDefinition, compute_liquidity_token_pda_seed};
use nssa_core::{
    account::AccountWithMetadata,
    program::{AccountPostState, ChainedCall},
};

#[expect(clippy::too_many_arguments, reason = "TODO: Fix later")]
pub fn add_liquidity(
    pool: AccountWithMetadata,
    vault_a: AccountWithMetadata,
    vault_b: AccountWithMetadata,
    pool_definition_lp: AccountWithMetadata,
    user_holding_a: AccountWithMetadata,
    user_holding_b: AccountWithMetadata,
    user_holding_lp: AccountWithMetadata,
    min_amount_liquidity: u128,
    max_amount_to_add_token_a: u128,
    max_amount_to_add_token_b: u128,
) -> (Vec<AccountPostState>, Vec<ChainedCall>) {
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

    if max_amount_to_add_token_a == 0 || max_amount_to_add_token_b == 0 {
        panic!("Both max-balances must be nonzero");
    }

    if min_amount_liquidity == 0 {
        panic!("Min-lp must be nonzero");
    }

    // 2. Determine deposit amount
    let vault_b_token_holding = token_core::TokenHolding::try_from(&vault_b.account.data)
        .expect("Add liquidity: AMM Program expects valid Token Holding Account for Vault B");
    let token_core::TokenHolding::Fungible {
        definition_id: _,
        balance: vault_b_balance,
    } = vault_b_token_holding
    else {
        panic!(
            "Add liquidity: AMM Program expects valid Fungible Token Holding Account for Vault B"
        );
    };

    let vault_a_token_holding = token_core::TokenHolding::try_from(&vault_a.account.data)
        .expect("Add liquidity: AMM Program expects valid Token Holding Account for Vault A");
    let token_core::TokenHolding::Fungible {
        definition_id: _,
        balance: vault_a_balance,
    } = vault_a_token_holding
    else {
        panic!(
            "Add liquidity: AMM Program expects valid Fungible Token Holding Account for Vault A"
        );
    };

    if pool_def_data.reserve_a == 0 || pool_def_data.reserve_b == 0 {
        panic!("Reserves must be nonzero");
    }

    if vault_a_balance < pool_def_data.reserve_a || vault_b_balance < pool_def_data.reserve_b {
        panic!("Vaults' balances must be at least the reserve amounts");
    }

    // Calculate actual_amounts
    let ideal_a: u128 =
        (pool_def_data.reserve_a * max_amount_to_add_token_b) / pool_def_data.reserve_b;
    let ideal_b: u128 =
        (pool_def_data.reserve_b * max_amount_to_add_token_a) / pool_def_data.reserve_a;

    let actual_amount_a = if ideal_a > max_amount_to_add_token_a {
        max_amount_to_add_token_a
    } else {
        ideal_a
    };
    let actual_amount_b = if ideal_b > max_amount_to_add_token_b {
        max_amount_to_add_token_b
    } else {
        ideal_b
    };

    // 3. Validate amounts
    if max_amount_to_add_token_a < actual_amount_a || max_amount_to_add_token_b < actual_amount_b {
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

    if delta_lp < min_amount_liquidity {
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
    let token_program_id = user_holding_a.account.program_owner;

    // Chain call for Token A (UserHoldingA -> Vault_A)
    let call_token_a = ChainedCall::new(
        token_program_id,
        vec![user_holding_a.clone(), vault_a.clone()],
        &token_core::Instruction::Transfer {
            amount_to_transfer: actual_amount_a,
        },
    );
    // Chain call for Token B (UserHoldingB -> Vault_B)
    let call_token_b = ChainedCall::new(
        token_program_id,
        vec![user_holding_b.clone(), vault_b.clone()],
        &token_core::Instruction::Transfer {
            amount_to_transfer: actual_amount_b,
        },
    );
    // Chain call for LP (mint new tokens for user_holding_lp)
    let mut pool_definition_lp_auth = pool_definition_lp.clone();
    pool_definition_lp_auth.is_authorized = true;
    let call_token_lp = ChainedCall::new(
        token_program_id,
        vec![pool_definition_lp_auth.clone(), user_holding_lp.clone()],
        &token_core::Instruction::Mint {
            amount_to_mint: delta_lp,
        },
    )
    .with_pda_seeds(vec![compute_liquidity_token_pda_seed(pool.account_id)]);

    let chained_calls = vec![call_token_lp, call_token_b, call_token_a];

    let post_states = vec![
        AccountPostState::new(pool_post),
        AccountPostState::new(vault_a.account.clone()),
        AccountPostState::new(vault_b.account.clone()),
        AccountPostState::new(pool_definition_lp.account.clone()),
        AccountPostState::new(user_holding_a.account.clone()),
        AccountPostState::new(user_holding_b.account.clone()),
        AccountPostState::new(user_holding_lp.account.clone()),
    ];

    (post_states, chained_calls)
}
