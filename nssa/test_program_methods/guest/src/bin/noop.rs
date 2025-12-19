use nssa_core::program::{read_nssa_inputs, write_nssa_outputs, ProgramInput, AccountPostState};

type Instruction = ();

fn main() {
    let (ProgramInput { pre_states, .. }, instruction_words) = read_nssa_inputs::<Instruction>();

    let post_states = pre_states.iter().map(|account| {
        AccountPostState::new(account.account.clone())
    }).collect();
    write_nssa_outputs(instruction_words, pre_states, post_states);
}
