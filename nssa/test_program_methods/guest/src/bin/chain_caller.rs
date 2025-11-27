use nssa_core::program::{
    ChainedCall, ProgramId, ProgramInput, read_nssa_inputs, write_nssa_outputs_with_chained_call,
};
use risc0_zkvm::serde::to_vec;

type Instruction = (u128, ProgramId, u32);

/// A program that calls another program `num_chain_calls` times.
/// It permutes the order of the input accounts on the subsequent call
fn main() {
    let ProgramInput {
        pre_states,
        instruction: (balance, program_id, num_chain_calls),
    } = read_nssa_inputs::<Instruction>();

    let [sender_pre, receiver_pre] = match pre_states.try_into() {
        Ok(array) => array,
        Err(_) => return,
    };

    let instruction_data = to_vec(&balance).unwrap();

    let mut chained_call = vec![
        ChainedCall {
            program_id,
            instruction_data: instruction_data.clone(),
            pre_states: vec![receiver_pre.clone(), sender_pre.clone()], // <- Account order permutation here
        };
        num_chain_calls as usize - 1
    ];

    chained_call.push(ChainedCall {
        program_id,
        instruction_data,
        pre_states: vec![receiver_pre.clone(), sender_pre.clone()], // <- Account order permutation here
    });

    write_nssa_outputs_with_chained_call(
        vec![sender_pre.clone(), receiver_pre.clone()],
        vec![sender_pre.account, receiver_pre.account],
        chained_call,
    );
}
