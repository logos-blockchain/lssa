use nssa_core::{
    account::Account,
    program::{read_nssa_inputs, write_nssa_outputs, ProgramInput},
};

type Instruction = ();

fn main() {
    let ProgramInput { pre_states, .. } = read_nssa_inputs::<Instruction>();

    let [pre] = match pre_states.try_into() {
        Ok(array) => array,
        Err(_) => return,
    };

    let account_pre = pre.account.clone();

    write_nssa_outputs(vec![pre], vec![account_pre, Account::default()]);
}
