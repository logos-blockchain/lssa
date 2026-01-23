//! The AMM Program.
//!
//! This program implements a simple AMM that supports multiple AMM pools (a single pool per
//! token pair).
//!
//! AMM program accepts [`Instruction`] as input, refer to the corresponding documentation
//! for more details.

use amm_core::Instruction;
use amm_program;
use nssa_core::program::{
    AccountPostState, ChainedCall, ProgramInput, read_nssa_inputs,
    write_nssa_outputs_with_chained_call,
};

fn main() {
    let (
        ProgramInput {
            pre_states,
            instruction,
        },
        instruction_words,
    ) = read_nssa_inputs::<Instruction>();

    let pre_states_clone = pre_states.clone();

    let (post_states, chained_calls): (Vec<AccountPostState>, Vec<ChainedCall>) = match instruction
    {
        Instruction::NewDefinition {
            token_a_amount,
            token_b_amount,
            amm_program_id,
        } => {
            let [
                pool,
                vault_a,
                vault_b,
                pool_definition_lp,
                user_holding_a,
                user_holding_b,
                user_holding_lp,
            ] = pre_states
                .try_into()
                .expect("Transfer instruction requires exactly seven accounts");
            amm_program::new_definition::new_definition(
                pool,
                vault_a,
                vault_b,
                pool_definition_lp,
                user_holding_a,
                user_holding_b,
                user_holding_lp,
                token_a_amount,
                token_b_amount,
                amm_program_id,
            )
        }
        Instruction::AddLiquidity {
            min_amount_liquidity,
            max_amount_to_add_token_a,
            max_amount_to_add_token_b,
        } => {
            let [
                pool,
                vault_a,
                vault_b,
                pool_definition_lp,
                user_holding_a,
                user_holding_b,
                user_holding_lp,
            ] = pre_states
                .try_into()
                .expect("Transfer instruction requires exactly seven accounts");
            amm_program::add::add_liquidity(
                pool,
                vault_a,
                vault_b,
                pool_definition_lp,
                user_holding_a,
                user_holding_b,
                user_holding_lp,
                min_amount_liquidity,
                max_amount_to_add_token_a,
                max_amount_to_add_token_b,
            )
        }
        Instruction::RemoveLiquidity {
            remove_liquidity_amount,
            min_amount_to_remove_token_a,
            min_amount_to_remove_token_b,
        } => {
            let [
                pool,
                vault_a,
                vault_b,
                pool_definition_lp,
                user_holding_a,
                user_holding_b,
                user_holding_lp,
            ] = pre_states
                .try_into()
                .expect("Transfer instruction requires exactly seven accounts");
            amm_program::remove::remove_liquidity(
                pool,
                vault_a,
                vault_b,
                pool_definition_lp,
                user_holding_a,
                user_holding_b,
                user_holding_lp,
                remove_liquidity_amount,
                min_amount_to_remove_token_a,
                min_amount_to_remove_token_b,
            )
        }
        Instruction::Swap {
            swap_amount_in,
            min_amount_out,
            token_definition_id_in,
        } => {
            let [pool, vault_a, vault_b, user_holding_a, user_holding_b] = pre_states
                .try_into()
                .expect("Transfer instruction requires exactly five accounts");
            amm_program::swap::swap(
                pool,
                vault_a,
                vault_b,
                user_holding_a,
                user_holding_b,
                swap_amount_in,
                min_amount_out,
                token_definition_id_in,
            )
        }
    };

    write_nssa_outputs_with_chained_call(
        instruction_words,
        pre_states_clone,
        post_states,
        chained_calls,
    );
}
