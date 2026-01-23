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
    let user_holding_lp_data = token_core::TokenHolding::try_from(&user_holding_lp.account.data)
        .expect("Remove liquidity: AMM Program expects a valid Token Account for liquidity token");
    let token_core::TokenHolding::Fungible {
        definition_id: _,
        balance: user_lp_balance,
    } = user_holding_lp_data
    else {
        panic!(
            "Remove liquidity: AMM Program expects a valid Fungible Token Holding Account for liquidity token"
        );
    };

    if user_lp_balance > pool_def_data.liquidity_pool_supply
        || user_holding_lp_data.definition_id() != pool_def_data.liquidity_pool_id
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

    let active: bool = pool_def_data.liquidity_pool_supply - delta_lp != 0;

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

    let token_program_id = user_holding_a.account.program_owner;

    // Chaincall for Token A withdraw
    let call_token_a = ChainedCall::new(
        token_program_id,
        vec![running_vault_a, user_holding_a.clone()],
        &token_core::Instruction::Transfer {
            amount_to_transfer: withdraw_amount_a,
        },
    )
    .with_pda_seeds(vec![compute_vault_pda_seed(
        pool.account_id,
        pool_def_data.definition_token_a_id,
    )]);
    // Chaincall for Token B withdraw
    let call_token_b = ChainedCall::new(
        token_program_id,
        vec![running_vault_b, user_holding_b.clone()],
        &token_core::Instruction::Transfer {
            amount_to_transfer: withdraw_amount_b,
        },
    )
    .with_pda_seeds(vec![compute_vault_pda_seed(
        pool.account_id,
        pool_def_data.definition_token_b_id,
    )]);
    // Chaincall for LP adjustment
    let mut pool_definition_lp_auth = pool_definition_lp.clone();
    pool_definition_lp_auth.is_authorized = true;
    let call_token_lp = ChainedCall::new(
        token_program_id,
        vec![pool_definition_lp_auth, user_holding_lp.clone()],
        &token_core::Instruction::Burn {
            amount_to_burn: delta_lp,
        },
    )
    .with_pda_seeds(vec![compute_liquidity_token_pda_seed(pool.account_id)]);

    let chained_calls = vec![call_token_lp, call_token_b, call_token_a];

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