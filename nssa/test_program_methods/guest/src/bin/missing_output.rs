use nssa_core::program::{ProgramInput, read_nssa_inputs, write_nssa_outputs};

type Instruction = ();

fn main() {
    let (ProgramInput { pre_states, .. }, instruction_words) = read_nssa_inputs::<Instruction>();

    let [pre1, pre2] = match pre_states.try_into() {
        Ok(array) => array,
        Err(_) => return,
    };

    let account_pre1 = pre1.account.clone();

    write_nssa_outputs(instruction_words, vec![pre1, pre2], vec![account_pre1]);
}
