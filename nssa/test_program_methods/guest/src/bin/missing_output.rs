use nssa_core::program::{read_nssa_inputs, write_nssa_outputs, ProgramInput};

type Instruction = ();

fn main() {
    let ProgramInput { pre_states, .. } = read_nssa_inputs::<Instruction>();

    let [pre1, pre2] = match pre_states.try_into() {
        Ok(array) => array,
        Err(_) => return,
    };

    let account_pre1 = pre1.account.clone();

    write_nssa_outputs(vec![pre1, pre2], vec![account_pre1]);
}
