#![cfg(all(test, feature = "with-nssa"))]

use amm_core::PoolDefinition;
use nssa::{
    Account, AccountId, Data, PrivateKey, PublicKey, PublicTransaction, V02State, program::Program,
    public_transaction,
};
use token_core::{TokenDefinition, TokenHolding};

struct PrivateKeysForTests;

impl PrivateKeysForTests {
    fn user_token_a_key() -> PrivateKey {
        PrivateKey::try_new([31; 32]).expect("Keys constructor expects valid private key")
    }

    fn user_token_b_key() -> PrivateKey {
        PrivateKey::try_new([32; 32]).expect("Keys constructor expects valid private key")
    }

    fn user_token_lp_key() -> PrivateKey {
        PrivateKey::try_new([33; 32]).expect("Keys constructor expects valid private key")
    }
}

struct BalanceForTests;

impl BalanceForTests {
    fn user_token_a_holding_init() -> u128 {
        10_000
    }

    fn user_token_b_holding_init() -> u128 {
        10_000
    }

    fn user_token_lp_holding_init() -> u128 {
        2_000
    }

    fn vault_a_balance_init() -> u128 {
        5_000
    }

    fn vault_b_balance_init() -> u128 {
        2_500
    }

    fn pool_lp_supply_init() -> u128 {
        5_000
    }

    fn token_a_supply() -> u128 {
        100_000
    }

    fn token_b_supply() -> u128 {
        100_000
    }

    fn token_lp_supply() -> u128 {
        5_000
    }

    fn remove_lp() -> u128 {
        1_000
    }

    fn remove_min_amount_a() -> u128 {
        500
    }

    fn remove_min_amount_b() -> u128 {
        500
    }

    fn add_min_amount_lp() -> u128 {
        1_000
    }

    fn add_max_amount_a() -> u128 {
        2_000
    }

    fn add_max_amount_b() -> u128 {
        1_000
    }

    fn swap_amount_in() -> u128 {
        1_000
    }

    fn swap_min_amount_out() -> u128 {
        200
    }

    fn vault_a_balance_swap_1() -> u128 {
        3_572
    }

    fn vault_b_balance_swap_1() -> u128 {
        3_500
    }

    fn user_token_a_holding_swap_1() -> u128 {
        11_428
    }

    fn user_token_b_holding_swap_1() -> u128 {
        9_000
    }

    fn vault_a_balance_swap_2() -> u128 {
        6_000
    }

    fn vault_b_balance_swap_2() -> u128 {
        2_084
    }

    fn user_token_a_holding_swap_2() -> u128 {
        9_000
    }

    fn user_token_b_holding_swap_2() -> u128 {
        10_416
    }

    fn vault_a_balance_add() -> u128 {
        7_000
    }

    fn vault_b_balance_add() -> u128 {
        3_500
    }

    fn user_token_a_holding_add() -> u128 {
        8_000
    }

    fn user_token_b_holding_add() -> u128 {
        9_000
    }

    fn user_token_lp_holding_add() -> u128 {
        4_000
    }

    fn token_lp_supply_add() -> u128 {
        7_000
    }

    fn vault_a_balance_remove() -> u128 {
        4_000
    }

    fn vault_b_balance_remove() -> u128 {
        2_000
    }

    fn user_token_a_holding_remove() -> u128 {
        11_000
    }

    fn user_token_b_holding_remove() -> u128 {
        10_500
    }

    fn user_token_lp_holding_remove() -> u128 {
        1_000
    }

    fn token_lp_supply_remove() -> u128 {
        4_000
    }

    fn user_token_a_holding_new_definition() -> u128 {
        5_000
    }

    fn user_token_b_holding_new_definition() -> u128 {
        7_500
    }
}

struct IdForTests;

impl IdForTests {
    fn pool_definition_id() -> AccountId {
        amm_core::compute_pool_pda(
            Program::amm().id(),
            IdForTests::token_a_definition_id(),
            IdForTests::token_b_definition_id(),
        )
    }

    fn token_lp_definition_id() -> AccountId {
        amm_core::compute_liquidity_token_pda(Program::amm().id(), IdForTests::pool_definition_id())
    }

    fn token_a_definition_id() -> AccountId {
        AccountId::new([3; 32])
    }

    fn token_b_definition_id() -> AccountId {
        AccountId::new([4; 32])
    }

    fn user_token_a_id() -> AccountId {
        AccountId::from(&PublicKey::new_from_private_key(
            &PrivateKeysForTests::user_token_a_key(),
        ))
    }

    fn user_token_b_id() -> AccountId {
        AccountId::from(&PublicKey::new_from_private_key(
            &PrivateKeysForTests::user_token_b_key(),
        ))
    }

    fn user_token_lp_id() -> AccountId {
        AccountId::from(&PublicKey::new_from_private_key(
            &PrivateKeysForTests::user_token_lp_key(),
        ))
    }

    fn vault_a_id() -> AccountId {
        amm_core::compute_vault_pda(
            Program::amm().id(),
            IdForTests::pool_definition_id(),
            IdForTests::token_a_definition_id(),
        )
    }

    fn vault_b_id() -> AccountId {
        amm_core::compute_vault_pda(
            Program::amm().id(),
            IdForTests::pool_definition_id(),
            IdForTests::token_b_definition_id(),
        )
    }
}

struct AccountForTests;

impl AccountForTests {
    fn user_token_a_holding() -> Account {
        Account {
            program_owner: Program::token().id(),
            balance: 0u128,
            data: Data::from(&TokenHolding::Fungible {
                definition_id: IdForTests::token_a_definition_id(),
                balance: BalanceForTests::user_token_a_holding_init(),
            }),
            nonce: 0,
        }
    }

    fn user_token_b_holding() -> Account {
        Account {
            program_owner: Program::token().id(),
            balance: 0u128,
            data: Data::from(&TokenHolding::Fungible {
                definition_id: IdForTests::token_b_definition_id(),
                balance: BalanceForTests::user_token_b_holding_init(),
            }),
            nonce: 0,
        }
    }

    fn pool_definition_init() -> Account {
        Account {
            program_owner: Program::amm().id(),
            balance: 0u128,
            data: Data::from(&PoolDefinition {
                definition_token_a_id: IdForTests::token_a_definition_id(),
                definition_token_b_id: IdForTests::token_b_definition_id(),
                vault_a_id: IdForTests::vault_a_id(),
                vault_b_id: IdForTests::vault_b_id(),
                liquidity_pool_id: IdForTests::token_lp_definition_id(),
                liquidity_pool_supply: BalanceForTests::pool_lp_supply_init(),
                reserve_a: BalanceForTests::vault_a_balance_init(),
                reserve_b: BalanceForTests::vault_b_balance_init(),
                fees: 0u128,
                active: true,
            }),
            nonce: 0,
        }
    }

    fn token_a_definition_account() -> Account {
        Account {
            program_owner: Program::token().id(),
            balance: 0u128,
            data: Data::from(&TokenDefinition::Fungible {
                name: String::from("test"),
                total_supply: BalanceForTests::token_a_supply(),
                metadata_id: None,
            }),
            nonce: 0,
        }
    }

    fn token_b_definition_acc() -> Account {
        Account {
            program_owner: Program::token().id(),
            balance: 0u128,
            data: Data::from(&TokenDefinition::Fungible {
                name: String::from("test"),
                total_supply: BalanceForTests::token_b_supply(),
                metadata_id: None,
            }),
            nonce: 0,
        }
    }

    fn token_lp_definition_acc() -> Account {
        Account {
            program_owner: Program::token().id(),
            balance: 0u128,
            data: Data::from(&TokenDefinition::Fungible {
                name: String::from("LP Token"),
                total_supply: BalanceForTests::token_lp_supply(),
                metadata_id: None,
            }),
            nonce: 0,
        }
    }

    fn vault_a_init() -> Account {
        Account {
            program_owner: Program::token().id(),
            balance: 0u128,
            data: Data::from(&TokenHolding::Fungible {
                definition_id: IdForTests::token_a_definition_id(),
                balance: BalanceForTests::vault_a_balance_init(),
            }),
            nonce: 0,
        }
    }

    fn vault_b_init() -> Account {
        Account {
            program_owner: Program::token().id(),
            balance: 0u128,
            data: Data::from(&TokenHolding::Fungible {
                definition_id: IdForTests::token_b_definition_id(),
                balance: BalanceForTests::vault_b_balance_init(),
            }),
            nonce: 0,
        }
    }

    fn user_token_lp_holding() -> Account {
        Account {
            program_owner: Program::token().id(),
            balance: 0u128,
            data: Data::from(&TokenHolding::Fungible {
                definition_id: IdForTests::token_lp_definition_id(),
                balance: BalanceForTests::user_token_lp_holding_init(),
            }),
            nonce: 0,
        }
    }

    fn vault_a_swap_1() -> Account {
        Account {
            program_owner: Program::token().id(),
            balance: 0u128,
            data: Data::from(&TokenHolding::Fungible {
                definition_id: IdForTests::token_a_definition_id(),
                balance: BalanceForTests::vault_a_balance_swap_1(),
            }),
            nonce: 0,
        }
    }

    fn vault_b_swap_1() -> Account {
        Account {
            program_owner: Program::token().id(),
            balance: 0u128,
            data: Data::from(&TokenHolding::Fungible {
                definition_id: IdForTests::token_b_definition_id(),
                balance: BalanceForTests::vault_b_balance_swap_1(),
            }),
            nonce: 0,
        }
    }

    fn pool_definition_swap_1() -> Account {
        Account {
            program_owner: Program::amm().id(),
            balance: 0u128,
            data: Data::from(&PoolDefinition {
                definition_token_a_id: IdForTests::token_a_definition_id(),
                definition_token_b_id: IdForTests::token_b_definition_id(),
                vault_a_id: IdForTests::vault_a_id(),
                vault_b_id: IdForTests::vault_b_id(),
                liquidity_pool_id: IdForTests::token_lp_definition_id(),
                liquidity_pool_supply: BalanceForTests::pool_lp_supply_init(),
                reserve_a: BalanceForTests::vault_a_balance_swap_1(),
                reserve_b: BalanceForTests::vault_b_balance_swap_1(),
                fees: 0u128,
                active: true,
            }),
            nonce: 0,
        }
    }

    fn user_token_a_holding_swap_1() -> Account {
        Account {
            program_owner: Program::token().id(),
            balance: 0u128,
            data: Data::from(&TokenHolding::Fungible {
                definition_id: IdForTests::token_a_definition_id(),
                balance: BalanceForTests::user_token_a_holding_swap_1(),
            }),
            nonce: 0,
        }
    }

    fn user_token_b_holding_swap_1() -> Account {
        Account {
            program_owner: Program::token().id(),
            balance: 0u128,
            data: Data::from(&TokenHolding::Fungible {
                definition_id: IdForTests::token_b_definition_id(),
                balance: BalanceForTests::user_token_b_holding_swap_1(),
            }),
            nonce: 1,
        }
    }

    fn vault_a_swap_2() -> Account {
        Account {
            program_owner: Program::token().id(),
            balance: 0u128,
            data: Data::from(&TokenHolding::Fungible {
                definition_id: IdForTests::token_a_definition_id(),
                balance: BalanceForTests::vault_a_balance_swap_2(),
            }),
            nonce: 0,
        }
    }

    fn vault_b_swap_2() -> Account {
        Account {
            program_owner: Program::token().id(),
            balance: 0u128,
            data: Data::from(&TokenHolding::Fungible {
                definition_id: IdForTests::token_b_definition_id(),
                balance: BalanceForTests::vault_b_balance_swap_2(),
            }),
            nonce: 0,
        }
    }

    fn pool_definition_swap_2() -> Account {
        Account {
            program_owner: Program::amm().id(),
            balance: 0u128,
            data: Data::from(&PoolDefinition {
                definition_token_a_id: IdForTests::token_a_definition_id(),
                definition_token_b_id: IdForTests::token_b_definition_id(),
                vault_a_id: IdForTests::vault_a_id(),
                vault_b_id: IdForTests::vault_b_id(),
                liquidity_pool_id: IdForTests::token_lp_definition_id(),
                liquidity_pool_supply: BalanceForTests::pool_lp_supply_init(),
                reserve_a: BalanceForTests::vault_a_balance_swap_2(),
                reserve_b: BalanceForTests::vault_b_balance_swap_2(),
                fees: 0u128,
                active: true,
            }),
            nonce: 0,
        }
    }

    fn user_token_a_holding_swap_2() -> Account {
        Account {
            program_owner: Program::token().id(),
            balance: 0u128,
            data: Data::from(&TokenHolding::Fungible {
                definition_id: IdForTests::token_a_definition_id(),
                balance: BalanceForTests::user_token_a_holding_swap_2(),
            }),
            nonce: 1,
        }
    }

    fn user_token_b_holding_swap_2() -> Account {
        Account {
            program_owner: Program::token().id(),
            balance: 0u128,
            data: Data::from(&TokenHolding::Fungible {
                definition_id: IdForTests::token_b_definition_id(),
                balance: BalanceForTests::user_token_b_holding_swap_2(),
            }),
            nonce: 0,
        }
    }

    fn vault_a_add() -> Account {
        Account {
            program_owner: Program::token().id(),
            balance: 0u128,
            data: Data::from(&TokenHolding::Fungible {
                definition_id: IdForTests::token_a_definition_id(),
                balance: BalanceForTests::vault_a_balance_add(),
            }),
            nonce: 0,
        }
    }

    fn vault_b_add() -> Account {
        Account {
            program_owner: Program::token().id(),
            balance: 0u128,
            data: Data::from(&TokenHolding::Fungible {
                definition_id: IdForTests::token_b_definition_id(),
                balance: BalanceForTests::vault_b_balance_add(),
            }),
            nonce: 0,
        }
    }

    fn pool_definition_add() -> Account {
        Account {
            program_owner: Program::amm().id(),
            balance: 0u128,
            data: Data::from(&PoolDefinition {
                definition_token_a_id: IdForTests::token_a_definition_id(),
                definition_token_b_id: IdForTests::token_b_definition_id(),
                vault_a_id: IdForTests::vault_a_id(),
                vault_b_id: IdForTests::vault_b_id(),
                liquidity_pool_id: IdForTests::token_lp_definition_id(),
                liquidity_pool_supply: BalanceForTests::token_lp_supply_add(),
                reserve_a: BalanceForTests::vault_a_balance_add(),
                reserve_b: BalanceForTests::vault_b_balance_add(),
                fees: 0u128,
                active: true,
            }),
            nonce: 0,
        }
    }

    fn user_token_a_holding_add() -> Account {
        Account {
            program_owner: Program::token().id(),
            balance: 0u128,
            data: Data::from(&TokenHolding::Fungible {
                definition_id: IdForTests::token_a_definition_id(),
                balance: BalanceForTests::user_token_a_holding_add(),
            }),
            nonce: 1,
        }
    }

    fn user_token_b_holding_add() -> Account {
        Account {
            program_owner: Program::token().id(),
            balance: 0u128,
            data: Data::from(&TokenHolding::Fungible {
                definition_id: IdForTests::token_b_definition_id(),
                balance: BalanceForTests::user_token_b_holding_add(),
            }),
            nonce: 1,
        }
    }

    fn user_token_lp_holding_add() -> Account {
        Account {
            program_owner: Program::token().id(),
            balance: 0u128,
            data: Data::from(&TokenHolding::Fungible {
                definition_id: IdForTests::token_lp_definition_id(),
                balance: BalanceForTests::user_token_lp_holding_add(),
            }),
            nonce: 0,
        }
    }

    fn token_lp_definition_add() -> Account {
        Account {
            program_owner: Program::token().id(),
            balance: 0u128,
            data: Data::from(&TokenDefinition::Fungible {
                name: String::from("LP Token"),
                total_supply: BalanceForTests::token_lp_supply_add(),
                metadata_id: None,
            }),
            nonce: 0,
        }
    }

    fn vault_a_remove() -> Account {
        Account {
            program_owner: Program::token().id(),
            balance: 0u128,
            data: Data::from(&TokenHolding::Fungible {
                definition_id: IdForTests::token_a_definition_id(),
                balance: BalanceForTests::vault_a_balance_remove(),
            }),
            nonce: 0,
        }
    }

    fn vault_b_remove() -> Account {
        Account {
            program_owner: Program::token().id(),
            balance: 0u128,
            data: Data::from(&TokenHolding::Fungible {
                definition_id: IdForTests::token_b_definition_id(),
                balance: BalanceForTests::vault_b_balance_remove(),
            }),
            nonce: 0,
        }
    }

    fn pool_definition_remove() -> Account {
        Account {
            program_owner: Program::amm().id(),
            balance: 0u128,
            data: Data::from(&PoolDefinition {
                definition_token_a_id: IdForTests::token_a_definition_id(),
                definition_token_b_id: IdForTests::token_b_definition_id(),
                vault_a_id: IdForTests::vault_a_id(),
                vault_b_id: IdForTests::vault_b_id(),
                liquidity_pool_id: IdForTests::token_lp_definition_id(),
                liquidity_pool_supply: BalanceForTests::token_lp_supply_remove(),
                reserve_a: BalanceForTests::vault_a_balance_remove(),
                reserve_b: BalanceForTests::vault_b_balance_remove(),
                fees: 0u128,
                active: true,
            }),
            nonce: 0,
        }
    }

    fn user_token_a_holding_remove() -> Account {
        Account {
            program_owner: Program::token().id(),
            balance: 0u128,
            data: Data::from(&TokenHolding::Fungible {
                definition_id: IdForTests::token_a_definition_id(),
                balance: BalanceForTests::user_token_a_holding_remove(),
            }),
            nonce: 0,
        }
    }

    fn user_token_b_holding_remove() -> Account {
        Account {
            program_owner: Program::token().id(),
            balance: 0u128,
            data: Data::from(&TokenHolding::Fungible {
                definition_id: IdForTests::token_b_definition_id(),
                balance: BalanceForTests::user_token_b_holding_remove(),
            }),
            nonce: 0,
        }
    }

    fn user_token_lp_holding_remove() -> Account {
        Account {
            program_owner: Program::token().id(),
            balance: 0u128,
            data: Data::from(&TokenHolding::Fungible {
                definition_id: IdForTests::token_lp_definition_id(),
                balance: BalanceForTests::user_token_lp_holding_remove(),
            }),
            nonce: 1,
        }
    }

    fn token_lp_definition_remove() -> Account {
        Account {
            program_owner: Program::token().id(),
            balance: 0u128,
            data: Data::from(&TokenDefinition::Fungible {
                name: String::from("LP Token"),
                total_supply: BalanceForTests::token_lp_supply_remove(),
                metadata_id: None,
            }),
            nonce: 0,
        }
    }

    fn token_lp_definition_init_inactive() -> Account {
        Account {
            program_owner: Program::token().id(),
            balance: 0u128,
            data: Data::from(&TokenDefinition::Fungible {
                name: String::from("LP Token"),
                total_supply: 0,
                metadata_id: None,
            }),
            nonce: 0,
        }
    }

    fn vault_a_init_inactive() -> Account {
        Account {
            program_owner: Program::token().id(),
            balance: 0u128,
            data: Data::from(&TokenHolding::Fungible {
                definition_id: IdForTests::token_a_definition_id(),
                balance: 0,
            }),
            nonce: 0,
        }
    }

    fn vault_b_init_inactive() -> Account {
        Account {
            program_owner: Program::token().id(),
            balance: 0u128,
            data: Data::from(&TokenHolding::Fungible {
                definition_id: IdForTests::token_b_definition_id(),
                balance: 0,
            }),
            nonce: 0,
        }
    }

    fn pool_definition_inactive() -> Account {
        Account {
            program_owner: Program::amm().id(),
            balance: 0u128,
            data: Data::from(&PoolDefinition {
                definition_token_a_id: IdForTests::token_a_definition_id(),
                definition_token_b_id: IdForTests::token_b_definition_id(),
                vault_a_id: IdForTests::vault_a_id(),
                vault_b_id: IdForTests::vault_b_id(),
                liquidity_pool_id: IdForTests::token_lp_definition_id(),
                liquidity_pool_supply: 0,
                reserve_a: 0,
                reserve_b: 0,
                fees: 0u128,
                active: false,
            }),
            nonce: 0,
        }
    }

    fn user_token_a_holding_new_init() -> Account {
        Account {
            program_owner: Program::token().id(),
            balance: 0u128,
            data: Data::from(&TokenHolding::Fungible {
                definition_id: IdForTests::token_a_definition_id(),
                balance: BalanceForTests::user_token_a_holding_new_definition(),
            }),
            nonce: 1,
        }
    }

    fn user_token_b_holding_new_init() -> Account {
        Account {
            program_owner: Program::token().id(),
            balance: 0u128,
            data: Data::from(&TokenHolding::Fungible {
                definition_id: IdForTests::token_b_definition_id(),
                balance: BalanceForTests::user_token_b_holding_new_definition(),
            }),
            nonce: 1,
        }
    }

    fn user_token_lp_holding_new_init() -> Account {
        Account {
            program_owner: Program::token().id(),
            balance: 0u128,
            data: Data::from(&TokenHolding::Fungible {
                definition_id: IdForTests::token_lp_definition_id(),
                balance: BalanceForTests::user_token_a_holding_new_definition(),
            }),
            nonce: 0,
        }
    }

    fn token_lp_definition_new_init() -> Account {
        Account {
            program_owner: Program::token().id(),
            balance: 0u128,
            data: Data::from(&TokenDefinition::Fungible {
                name: String::from("LP Token"),
                total_supply: BalanceForTests::vault_a_balance_init(),
                metadata_id: None,
            }),
            nonce: 0,
        }
    }

    fn pool_definition_new_init() -> Account {
        Account {
            program_owner: Program::amm().id(),
            balance: 0u128,
            data: Data::from(&PoolDefinition {
                definition_token_a_id: IdForTests::token_a_definition_id(),
                definition_token_b_id: IdForTests::token_b_definition_id(),
                vault_a_id: IdForTests::vault_a_id(),
                vault_b_id: IdForTests::vault_b_id(),
                liquidity_pool_id: IdForTests::token_lp_definition_id(),
                liquidity_pool_supply: BalanceForTests::user_token_a_holding_new_definition(),
                reserve_a: BalanceForTests::vault_a_balance_init(),
                reserve_b: BalanceForTests::vault_b_balance_init(),
                fees: 0u128,
                active: true,
            }),
            nonce: 0,
        }
    }

    fn user_token_lp_holding_init_zero() -> Account {
        Account {
            program_owner: Program::token().id(),
            balance: 0u128,
            data: Data::from(&TokenHolding::Fungible {
                definition_id: IdForTests::token_lp_definition_id(),
                balance: 0,
            }),
            nonce: 0,
        }
    }
}

fn state_for_amm_tests() -> V02State {
    let initial_data = [];
    let mut state = V02State::new_with_genesis_accounts(&initial_data, &[]);
    state.force_insert_account(
        IdForTests::pool_definition_id(),
        AccountForTests::pool_definition_init(),
    );
    state.force_insert_account(
        IdForTests::token_a_definition_id(),
        AccountForTests::token_a_definition_account(),
    );
    state.force_insert_account(
        IdForTests::token_b_definition_id(),
        AccountForTests::token_b_definition_acc(),
    );
    state.force_insert_account(
        IdForTests::token_lp_definition_id(),
        AccountForTests::token_lp_definition_acc(),
    );
    state.force_insert_account(
        IdForTests::user_token_a_id(),
        AccountForTests::user_token_a_holding(),
    );
    state.force_insert_account(
        IdForTests::user_token_b_id(),
        AccountForTests::user_token_b_holding(),
    );
    state.force_insert_account(
        IdForTests::user_token_lp_id(),
        AccountForTests::user_token_lp_holding(),
    );
    state.force_insert_account(IdForTests::vault_a_id(), AccountForTests::vault_a_init());
    state.force_insert_account(IdForTests::vault_b_id(), AccountForTests::vault_b_init());

    state
}

fn state_for_amm_tests_with_new_def() -> V02State {
    let initial_data = [];
    let mut state = V02State::new_with_genesis_accounts(&initial_data, &[]);
    state.force_insert_account(
        IdForTests::token_a_definition_id(),
        AccountForTests::token_a_definition_account(),
    );
    state.force_insert_account(
        IdForTests::token_b_definition_id(),
        AccountForTests::token_b_definition_acc(),
    );
    state.force_insert_account(
        IdForTests::user_token_a_id(),
        AccountForTests::user_token_a_holding(),
    );
    state.force_insert_account(
        IdForTests::user_token_b_id(),
        AccountForTests::user_token_b_holding(),
    );
    state
}

#[test]
fn test_simple_amm_remove() {
    let mut state = state_for_amm_tests();

    let instruction = amm_core::Instruction::RemoveLiquidity {
        remove_liquidity_amount: BalanceForTests::remove_lp(),
        min_amount_to_remove_token_a: BalanceForTests::remove_min_amount_a(),
        min_amount_to_remove_token_b: BalanceForTests::remove_min_amount_b(),
    };

    let message = public_transaction::Message::try_new(
        Program::amm().id(),
        vec![
            IdForTests::pool_definition_id(),
            IdForTests::vault_a_id(),
            IdForTests::vault_b_id(),
            IdForTests::token_lp_definition_id(),
            IdForTests::user_token_a_id(),
            IdForTests::user_token_b_id(),
            IdForTests::user_token_lp_id(),
        ],
        vec![0],
        instruction,
    )
    .unwrap();

    let witness_set = public_transaction::WitnessSet::for_message(
        &message,
        &[&PrivateKeysForTests::user_token_lp_key()],
    );

    let tx = PublicTransaction::new(message, witness_set);
    state.transition_from_public_transaction(&tx).unwrap();

    let pool_post = state.get_account_by_id(IdForTests::pool_definition_id());
    let vault_a_post = state.get_account_by_id(IdForTests::vault_a_id());
    let vault_b_post = state.get_account_by_id(IdForTests::vault_b_id());
    let token_lp_post = state.get_account_by_id(IdForTests::token_lp_definition_id());
    let user_token_a_post = state.get_account_by_id(IdForTests::user_token_a_id());
    let user_token_b_post = state.get_account_by_id(IdForTests::user_token_b_id());
    let user_token_lp_post = state.get_account_by_id(IdForTests::user_token_lp_id());

    let expected_pool = AccountForTests::pool_definition_remove();
    let expected_vault_a = AccountForTests::vault_a_remove();
    let expected_vault_b = AccountForTests::vault_b_remove();
    let expected_token_lp = AccountForTests::token_lp_definition_remove();
    let expected_user_token_a = AccountForTests::user_token_a_holding_remove();
    let expected_user_token_b = AccountForTests::user_token_b_holding_remove();
    let expected_user_token_lp = AccountForTests::user_token_lp_holding_remove();

    assert_eq!(pool_post, expected_pool);
    assert_eq!(vault_a_post, expected_vault_a);
    assert_eq!(vault_b_post, expected_vault_b);
    assert_eq!(token_lp_post, expected_token_lp);
    assert_eq!(user_token_a_post, expected_user_token_a);
    assert_eq!(user_token_b_post, expected_user_token_b);
    assert_eq!(user_token_lp_post, expected_user_token_lp);
}

#[test]
fn test_simple_amm_new_definition_inactive_initialized_pool_and_uninit_user_lp() {
    let mut state = state_for_amm_tests_with_new_def();

    // Uninitialized in constructor
    state.force_insert_account(
        IdForTests::vault_a_id(),
        AccountForTests::vault_a_init_inactive(),
    );
    state.force_insert_account(
        IdForTests::vault_b_id(),
        AccountForTests::vault_b_init_inactive(),
    );
    state.force_insert_account(
        IdForTests::pool_definition_id(),
        AccountForTests::pool_definition_inactive(),
    );
    state.force_insert_account(
        IdForTests::token_lp_definition_id(),
        AccountForTests::token_lp_definition_init_inactive(),
    );

    let instruction = amm_core::Instruction::NewDefinition {
        token_a_amount: BalanceForTests::vault_a_balance_init(),
        token_b_amount: BalanceForTests::vault_b_balance_init(),
        amm_program_id: Program::amm().id(),
    };

    let message = public_transaction::Message::try_new(
        Program::amm().id(),
        vec![
            IdForTests::pool_definition_id(),
            IdForTests::vault_a_id(),
            IdForTests::vault_b_id(),
            IdForTests::token_lp_definition_id(),
            IdForTests::user_token_a_id(),
            IdForTests::user_token_b_id(),
            IdForTests::user_token_lp_id(),
        ],
        vec![0, 0],
        instruction,
    )
    .unwrap();

    let witness_set = public_transaction::WitnessSet::for_message(
        &message,
        &[
            &PrivateKeysForTests::user_token_a_key(),
            &PrivateKeysForTests::user_token_b_key(),
        ],
    );

    let tx = PublicTransaction::new(message, witness_set);
    state.transition_from_public_transaction(&tx).unwrap();

    let pool_post = state.get_account_by_id(IdForTests::pool_definition_id());
    let vault_a_post = state.get_account_by_id(IdForTests::vault_a_id());
    let vault_b_post = state.get_account_by_id(IdForTests::vault_b_id());
    let token_lp_post = state.get_account_by_id(IdForTests::token_lp_definition_id());
    let user_token_a_post = state.get_account_by_id(IdForTests::user_token_a_id());
    let user_token_b_post = state.get_account_by_id(IdForTests::user_token_b_id());
    let user_token_lp_post = state.get_account_by_id(IdForTests::user_token_lp_id());

    let expected_pool = AccountForTests::pool_definition_new_init();
    let expected_vault_a = AccountForTests::vault_a_init();
    let expected_vault_b = AccountForTests::vault_b_init();
    let expected_token_lp = AccountForTests::token_lp_definition_new_init();
    let expected_user_token_a = AccountForTests::user_token_a_holding_new_init();
    let expected_user_token_b = AccountForTests::user_token_b_holding_new_init();
    let expected_user_token_lp = AccountForTests::user_token_lp_holding_new_init();

    assert_eq!(pool_post, expected_pool);
    assert_eq!(vault_a_post, expected_vault_a);
    assert_eq!(vault_b_post, expected_vault_b);
    assert_eq!(token_lp_post, expected_token_lp);
    assert_eq!(user_token_a_post, expected_user_token_a);
    assert_eq!(user_token_b_post, expected_user_token_b);
    assert_eq!(user_token_lp_post, expected_user_token_lp);
}

#[test]
fn test_simple_amm_new_definition_inactive_initialized_pool_init_user_lp() {
    let mut state = state_for_amm_tests_with_new_def();

    // Uninitialized in constructor
    state.force_insert_account(
        IdForTests::vault_a_id(),
        AccountForTests::vault_a_init_inactive(),
    );
    state.force_insert_account(
        IdForTests::vault_b_id(),
        AccountForTests::vault_b_init_inactive(),
    );
    state.force_insert_account(
        IdForTests::pool_definition_id(),
        AccountForTests::pool_definition_inactive(),
    );
    state.force_insert_account(
        IdForTests::token_lp_definition_id(),
        AccountForTests::token_lp_definition_init_inactive(),
    );
    state.force_insert_account(
        IdForTests::user_token_lp_id(),
        AccountForTests::user_token_lp_holding_init_zero(),
    );

    let instruction = amm_core::Instruction::NewDefinition {
        token_a_amount: BalanceForTests::vault_a_balance_init(),
        token_b_amount: BalanceForTests::vault_b_balance_init(),
        amm_program_id: Program::amm().id(),
    };

    let message = public_transaction::Message::try_new(
        Program::amm().id(),
        vec![
            IdForTests::pool_definition_id(),
            IdForTests::vault_a_id(),
            IdForTests::vault_b_id(),
            IdForTests::token_lp_definition_id(),
            IdForTests::user_token_a_id(),
            IdForTests::user_token_b_id(),
            IdForTests::user_token_lp_id(),
        ],
        vec![0, 0],
        instruction,
    )
    .unwrap();

    let witness_set = public_transaction::WitnessSet::for_message(
        &message,
        &[
            &PrivateKeysForTests::user_token_a_key(),
            &PrivateKeysForTests::user_token_b_key(),
        ],
    );

    let tx = PublicTransaction::new(message, witness_set);
    state.transition_from_public_transaction(&tx).unwrap();

    let pool_post = state.get_account_by_id(IdForTests::pool_definition_id());
    let vault_a_post = state.get_account_by_id(IdForTests::vault_a_id());
    let vault_b_post = state.get_account_by_id(IdForTests::vault_b_id());
    let token_lp_post = state.get_account_by_id(IdForTests::token_lp_definition_id());
    let user_token_a_post = state.get_account_by_id(IdForTests::user_token_a_id());
    let user_token_b_post = state.get_account_by_id(IdForTests::user_token_b_id());
    let user_token_lp_post = state.get_account_by_id(IdForTests::user_token_lp_id());

    let expected_pool = AccountForTests::pool_definition_init();
    let expected_vault_a = AccountForTests::vault_a_init();
    let expected_vault_b = AccountForTests::vault_b_init();
    let expected_token_lp = AccountForTests::token_lp_definition_new_init();
    let expected_user_token_a = AccountForTests::user_token_a_holding_new_init();
    let expected_user_token_b = AccountForTests::user_token_b_holding_new_init();
    let expected_user_token_lp = AccountForTests::user_token_lp_holding_new_init();

    assert_eq!(pool_post, expected_pool);
    assert_eq!(vault_a_post, expected_vault_a);
    assert_eq!(vault_b_post, expected_vault_b);
    assert_eq!(token_lp_post, expected_token_lp);
    assert_eq!(user_token_a_post, expected_user_token_a);
    assert_eq!(user_token_b_post, expected_user_token_b);
    assert_eq!(user_token_lp_post, expected_user_token_lp);
}

#[test]
fn test_simple_amm_new_definition_uninitialized_pool() {
    let mut state = state_for_amm_tests_with_new_def();

    // Uninitialized in constructor
    state.force_insert_account(
        IdForTests::vault_a_id(),
        AccountForTests::vault_a_init_inactive(),
    );
    state.force_insert_account(
        IdForTests::vault_b_id(),
        AccountForTests::vault_b_init_inactive(),
    );

    let instruction = amm_core::Instruction::NewDefinition {
        token_a_amount: BalanceForTests::vault_a_balance_init(),
        token_b_amount: BalanceForTests::vault_b_balance_init(),
        amm_program_id: Program::amm().id(),
    };

    let message = public_transaction::Message::try_new(
        Program::amm().id(),
        vec![
            IdForTests::pool_definition_id(),
            IdForTests::vault_a_id(),
            IdForTests::vault_b_id(),
            IdForTests::token_lp_definition_id(),
            IdForTests::user_token_a_id(),
            IdForTests::user_token_b_id(),
            IdForTests::user_token_lp_id(),
        ],
        vec![0, 0],
        instruction,
    )
    .unwrap();

    let witness_set = public_transaction::WitnessSet::for_message(
        &message,
        &[
            &PrivateKeysForTests::user_token_a_key(),
            &PrivateKeysForTests::user_token_b_key(),
        ],
    );

    let tx = PublicTransaction::new(message, witness_set);
    state.transition_from_public_transaction(&tx).unwrap();

    let pool_post = state.get_account_by_id(IdForTests::pool_definition_id());
    let vault_a_post = state.get_account_by_id(IdForTests::vault_a_id());
    let vault_b_post = state.get_account_by_id(IdForTests::vault_b_id());
    let token_lp_post = state.get_account_by_id(IdForTests::token_lp_definition_id());
    let user_token_a_post = state.get_account_by_id(IdForTests::user_token_a_id());
    let user_token_b_post = state.get_account_by_id(IdForTests::user_token_b_id());
    let user_token_lp_post = state.get_account_by_id(IdForTests::user_token_lp_id());

    let expected_pool = AccountForTests::pool_definition_new_init();
    let expected_vault_a = AccountForTests::vault_a_init();
    let expected_vault_b = AccountForTests::vault_b_init();
    let expected_token_lp = AccountForTests::token_lp_definition_new_init();
    let expected_user_token_a = AccountForTests::user_token_a_holding_new_init();
    let expected_user_token_b = AccountForTests::user_token_b_holding_new_init();
    let expected_user_token_lp = AccountForTests::user_token_lp_holding_new_init();

    assert_eq!(pool_post, expected_pool);
    assert_eq!(vault_a_post, expected_vault_a);
    assert_eq!(vault_b_post, expected_vault_b);
    assert_eq!(token_lp_post, expected_token_lp);
    assert_eq!(user_token_a_post, expected_user_token_a);
    assert_eq!(user_token_b_post, expected_user_token_b);
    assert_eq!(user_token_lp_post, expected_user_token_lp);
}

#[test]
fn test_simple_amm_add() {
    let mut state = state_for_amm_tests();

    let instruction = amm_core::Instruction::AddLiquidity {
        min_amount_liquidity: BalanceForTests::add_min_amount_lp(),
        max_amount_to_add_token_a: BalanceForTests::add_max_amount_a(),
        max_amount_to_add_token_b: BalanceForTests::add_max_amount_b(),
    };

    let message = public_transaction::Message::try_new(
        Program::amm().id(),
        vec![
            IdForTests::pool_definition_id(),
            IdForTests::vault_a_id(),
            IdForTests::vault_b_id(),
            IdForTests::token_lp_definition_id(),
            IdForTests::user_token_a_id(),
            IdForTests::user_token_b_id(),
            IdForTests::user_token_lp_id(),
        ],
        vec![0, 0],
        instruction,
    )
    .unwrap();

    let witness_set = public_transaction::WitnessSet::for_message(
        &message,
        &[
            &PrivateKeysForTests::user_token_a_key(),
            &PrivateKeysForTests::user_token_b_key(),
        ],
    );

    let tx = PublicTransaction::new(message, witness_set);
    state.transition_from_public_transaction(&tx).unwrap();

    let pool_post = state.get_account_by_id(IdForTests::pool_definition_id());
    let vault_a_post = state.get_account_by_id(IdForTests::vault_a_id());
    let vault_b_post = state.get_account_by_id(IdForTests::vault_b_id());
    let token_lp_post = state.get_account_by_id(IdForTests::token_lp_definition_id());
    let user_token_a_post = state.get_account_by_id(IdForTests::user_token_a_id());
    let user_token_b_post = state.get_account_by_id(IdForTests::user_token_b_id());
    let user_token_lp_post = state.get_account_by_id(IdForTests::user_token_lp_id());

    let expected_pool = AccountForTests::pool_definition_add();
    let expected_vault_a = AccountForTests::vault_a_add();
    let expected_vault_b = AccountForTests::vault_b_add();
    let expected_token_lp = AccountForTests::token_lp_definition_add();
    let expected_user_token_a = AccountForTests::user_token_a_holding_add();
    let expected_user_token_b = AccountForTests::user_token_b_holding_add();
    let expected_user_token_lp = AccountForTests::user_token_lp_holding_add();

    assert_eq!(pool_post, expected_pool);
    assert_eq!(vault_a_post, expected_vault_a);
    assert_eq!(vault_b_post, expected_vault_b);
    assert_eq!(token_lp_post, expected_token_lp);
    assert_eq!(user_token_a_post, expected_user_token_a);
    assert_eq!(user_token_b_post, expected_user_token_b);
    assert_eq!(user_token_lp_post, expected_user_token_lp);
}

#[test]
fn test_simple_amm_swap_1() {
    let mut state = state_for_amm_tests();

    let instruction = amm_core::Instruction::Swap {
        swap_amount_in: BalanceForTests::swap_amount_in(),
        min_amount_out: BalanceForTests::swap_min_amount_out(),
        token_definition_id_in: IdForTests::token_b_definition_id(),
    };

    let message = public_transaction::Message::try_new(
        Program::amm().id(),
        vec![
            IdForTests::pool_definition_id(),
            IdForTests::vault_a_id(),
            IdForTests::vault_b_id(),
            IdForTests::user_token_a_id(),
            IdForTests::user_token_b_id(),
        ],
        vec![0],
        instruction,
    )
    .unwrap();

    let witness_set = public_transaction::WitnessSet::for_message(
        &message,
        &[&PrivateKeysForTests::user_token_b_key()],
    );

    let tx = PublicTransaction::new(message, witness_set);
    state.transition_from_public_transaction(&tx).unwrap();

    let pool_post = state.get_account_by_id(IdForTests::pool_definition_id());
    let vault_a_post = state.get_account_by_id(IdForTests::vault_a_id());
    let vault_b_post = state.get_account_by_id(IdForTests::vault_b_id());
    let user_token_a_post = state.get_account_by_id(IdForTests::user_token_a_id());
    let user_token_b_post = state.get_account_by_id(IdForTests::user_token_b_id());

    let expected_pool = AccountForTests::pool_definition_swap_1();
    let expected_vault_a = AccountForTests::vault_a_swap_1();
    let expected_vault_b = AccountForTests::vault_b_swap_1();
    let expected_user_token_a = AccountForTests::user_token_a_holding_swap_1();
    let expected_user_token_b = AccountForTests::user_token_b_holding_swap_1();

    assert_eq!(pool_post, expected_pool);
    assert_eq!(vault_a_post, expected_vault_a);
    assert_eq!(vault_b_post, expected_vault_b);
    assert_eq!(user_token_a_post, expected_user_token_a);
    assert_eq!(user_token_b_post, expected_user_token_b);
}

#[test]
fn test_simple_amm_swap_2() {
    let mut state = state_for_amm_tests();

    let instruction = amm_core::Instruction::Swap {
        swap_amount_in: BalanceForTests::swap_amount_in(),
        min_amount_out: BalanceForTests::swap_min_amount_out(),
        token_definition_id_in: IdForTests::token_a_definition_id(),
    };
    let message = public_transaction::Message::try_new(
        Program::amm().id(),
        vec![
            IdForTests::pool_definition_id(),
            IdForTests::vault_a_id(),
            IdForTests::vault_b_id(),
            IdForTests::user_token_a_id(),
            IdForTests::user_token_b_id(),
        ],
        vec![0],
        instruction,
    )
    .unwrap();

    let witness_set = public_transaction::WitnessSet::for_message(
        &message,
        &[&PrivateKeysForTests::user_token_a_key()],
    );

    let tx = PublicTransaction::new(message, witness_set);
    state.transition_from_public_transaction(&tx).unwrap();

    let pool_post = state.get_account_by_id(IdForTests::pool_definition_id());
    let vault_a_post = state.get_account_by_id(IdForTests::vault_a_id());
    let vault_b_post = state.get_account_by_id(IdForTests::vault_b_id());
    let user_token_a_post = state.get_account_by_id(IdForTests::user_token_a_id());
    let user_token_b_post = state.get_account_by_id(IdForTests::user_token_b_id());

    let expected_pool = AccountForTests::pool_definition_swap_2();
    let expected_vault_a = AccountForTests::vault_a_swap_2();
    let expected_vault_b = AccountForTests::vault_b_swap_2();
    let expected_user_token_a = AccountForTests::user_token_a_holding_swap_2();
    let expected_user_token_b = AccountForTests::user_token_b_holding_swap_2();

    assert_eq!(pool_post, expected_pool);
    assert_eq!(vault_a_post, expected_vault_a);
    assert_eq!(vault_b_post, expected_vault_b);
    assert_eq!(user_token_a_post, expected_user_token_a);
    assert_eq!(user_token_b_post, expected_user_token_b);
}
