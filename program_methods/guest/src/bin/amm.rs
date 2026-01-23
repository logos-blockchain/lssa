use nssa_core::{
    account::{Account, AccountId, AccountWithMetadata, Data},
    program::{
        AccountPostState, ChainedCall, PdaSeed, ProgramId, ProgramInput, read_nssa_inputs,
        write_nssa_outputs_with_chained_call,
    },
};
use amm_program::core::Instruction;

fn main() {
    let (
        ProgramInput {
            pre_states,
            instruction,
        },
        instruction_words,
    ) = read_nssa_inputs::<Instruction>();

    let pre_states_clone = pre_states.clone();

    let (post_states, chained_calls) = match instruction {
        Intruction::NewAMM => {

        }
        Instruction::RemoveLiquidity{ remove_liquidity_amount, min_amount_to_remove_token_a, min_amount_to_remove_token_b } =>{
            let [pool, vault_a, vault_b, pool_definition_lp, user_holding_a, user_holding_b, user_holding_lp] = pre_states
                .try_into()
                .expect("RemoveLiquidity instruction requires exactly seven accounts");
            amm_program::remove::remove_liquidity(pool, vault_a, vault_b, pool_definition_lp, user_holding_a, user_holding_b, user_holding_lp, remove_liquidity_amount, min_amount_to_remove_token_a, min_amount_to_remove_token_b)
        }
        Instruction::AddLiquidity { min_amount_liquidity, max_amount_a, max_amount_b } => {
            let [pool, vault_a, vault_b, pool_definition_lp, user_holding_a, user_holding_b, user_holding_lp] = pre_states
                .try_into()
                .expect("AddLiquidity instruction requires exactly seven accounts");
            amm_program::add::add_liquidity(pool, vault_a, vault_b, pool_definition_lp, user_holding_a, user_holding_b, user_holding_lp, min_amount_liquidity, max_amount_a, max_amount_b)
        }

    }

    write_nssa_outputs_with_chained_call(instruction_words, pre_states_clone, post_states, chained_calls);
}