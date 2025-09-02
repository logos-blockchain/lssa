use nssa_core::program::{read_nssa_inputs, write_nssa_outputs, ProgramInput};

type Instruction = ();

fn main() {
    let ProgramInput { pre_states, .. } = read_nssa_inputs::<Instruction>();

    let [pre] = match pre_states.try_into() {
        Ok(array) => array,
        Err(_) => return,
    };

    let account_pre = &pre.account;
    let mut account_post = account_pre.clone();
    account_post.program_owner = [0, 1, 2, 3, 4, 5, 6, 7];

    write_nssa_outputs(vec![pre], vec![account_post]);
}
