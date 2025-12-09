use nssa_core::{
    account::Data,
    program::{
        AccountPostState, ChainedCall, PdaSeed, ProgramInput, read_nssa_inputs,
        write_nssa_outputs_with_chained_call,
    },
};
use risc0_zkvm::{
    serde::to_vec,
    sha::{Impl, Sha256},
};

const PRIZE: u128 = 150;

type Instruction = u128;

struct Challenge {
    difficulty: u8,
    seed: [u8; 32],
}

impl Challenge {
    fn new(bytes: &[u8]) -> Self {
        assert_eq!(bytes.len(), 33);
        let difficulty = bytes[0];
        assert!(difficulty <= 32);

        let mut seed = [0; 32];
        seed.copy_from_slice(&bytes[1..]);
        Self { difficulty, seed }
    }

    // Checks if the leftmost `self.difficulty` number of bytes of SHA256(self.data || solution) are
    // zero.
    fn validate_solution(&self, solution: Instruction) -> bool {
        let mut bytes = [0; 32 + 16];
        bytes[..32].copy_from_slice(&self.seed);
        bytes[32..].copy_from_slice(&solution.to_le_bytes());
        let digest: [u8; 32] = Impl::hash_bytes(&bytes).as_bytes().try_into().unwrap();
        let difficulty = self.difficulty as usize;
        digest[..difficulty].iter().all(|&b| b == 0)
    }

    fn next_data(self) -> Data {
        let mut result = [0; 33];
        result[0] = self.difficulty;
        result[1..].copy_from_slice(Impl::hash_bytes(&self.seed).as_bytes());
        result.to_vec().try_into().expect("should fit")
    }
}

/// A pinata program
fn main() {
    // Read input accounts.
    // It is expected to receive three accounts: [pinata_definition, pinata_token_holding,
    // winner_token_holding]
    let ProgramInput {
        pre_states,
        instruction: solution,
    } = read_nssa_inputs::<Instruction>();

    let [
        pinata_definition,
        pinata_token_holding,
        winner_token_holding,
    ] = match pre_states.try_into() {
        Ok(array) => array,
        Err(_) => return,
    };

    let data = Challenge::new(&pinata_definition.account.data);

    if !data.validate_solution(solution) {
        return;
    }

    let mut pinata_definition_post = pinata_definition.account.clone();
    let pinata_token_holding_post = pinata_token_holding.account.clone();
    let winner_token_holding_post = winner_token_holding.account.clone();
    pinata_definition_post.data = data.next_data();

    let mut instruction_data: [u8; 23] = [0; 23];
    instruction_data[0] = 1;
    instruction_data[1..17].copy_from_slice(&PRIZE.to_le_bytes());

    // Flip authorization to true for chained call
    let mut pinata_token_holding_for_chain_call = pinata_token_holding.clone();
    pinata_token_holding_for_chain_call.is_authorized = true;

    let chained_calls = vec![ChainedCall {
        program_id: pinata_token_holding_post.program_owner,
        instruction_data: to_vec(&instruction_data).unwrap(),
        pre_states: vec![
            pinata_token_holding_for_chain_call,
            winner_token_holding.clone(),
        ],
        pda_seeds: vec![PdaSeed::new([0; 32])],
    }];

    write_nssa_outputs_with_chained_call(
        vec![
            pinata_definition,
            pinata_token_holding,
            winner_token_holding,
        ],
        vec![
            AccountPostState::new(pinata_definition_post),
            AccountPostState::new(pinata_token_holding_post),
            AccountPostState::new(winner_token_holding_post),
        ],
        chained_calls,
    );
}
