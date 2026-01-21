use common::{error::ExecutionFailureKind, rpc_primitives::requests::SendTxResponse};
use nssa::{AccountId, ProgramId, program::Program};
use nssa_core::program::PdaSeed;

use crate::{TokenHolding, WalletCore};

fn compute_pool_pda(
    amm_program_id: ProgramId,
    definition_token_a_id: AccountId,
    definition_token_b_id: AccountId,
) -> AccountId {
    AccountId::from((
        &amm_program_id,
        &compute_pool_pda_seed(definition_token_a_id, definition_token_b_id),
    ))
}

fn compute_pool_pda_seed(
    definition_token_a_id: AccountId,
    definition_token_b_id: AccountId,
) -> PdaSeed {
    use risc0_zkvm::sha::{Impl, Sha256};

    let mut i: usize = 0;
    let (token_1, token_2) = loop {
        if definition_token_a_id.value()[i] > definition_token_b_id.value()[i] {
            let token_1 = definition_token_a_id;
            let token_2 = definition_token_b_id;
            break (token_1, token_2);
        } else if definition_token_a_id.value()[i] < definition_token_b_id.value()[i] {
            let token_1 = definition_token_b_id;
            let token_2 = definition_token_a_id;
            break (token_1, token_2);
        }

        if i == 32 {
            panic!("Definitions match");
        } else {
            i += 1;
        }
    };

    let mut bytes = [0; 64];
    bytes[0..32].copy_from_slice(&token_1.to_bytes());
    bytes[32..].copy_from_slice(&token_2.to_bytes());

    PdaSeed::new(
        Impl::hash_bytes(&bytes)
            .as_bytes()
            .try_into()
            .expect("Hash output must be exactly 32 bytes long"),
    )
}

fn compute_vault_pda(
    amm_program_id: ProgramId,
    pool_id: AccountId,
    definition_token_id: AccountId,
) -> AccountId {
    AccountId::from((
        &amm_program_id,
        &compute_vault_pda_seed(pool_id, definition_token_id),
    ))
}

fn compute_vault_pda_seed(pool_id: AccountId, definition_token_id: AccountId) -> PdaSeed {
    use risc0_zkvm::sha::{Impl, Sha256};

    let mut bytes = [0; 64];
    bytes[0..32].copy_from_slice(&pool_id.to_bytes());
    bytes[32..].copy_from_slice(&definition_token_id.to_bytes());

    PdaSeed::new(
        Impl::hash_bytes(&bytes)
            .as_bytes()
            .try_into()
            .expect("Hash output must be exactly 32 bytes long"),
    )
}

fn compute_liquidity_token_pda(amm_program_id: ProgramId, pool_id: AccountId) -> AccountId {
    AccountId::from((&amm_program_id, &compute_liquidity_token_pda_seed(pool_id)))
}

fn compute_liquidity_token_pda_seed(pool_id: AccountId) -> PdaSeed {
    use risc0_zkvm::sha::{Impl, Sha256};

    let mut bytes = [0; 64];
    bytes[0..32].copy_from_slice(&pool_id.to_bytes());
    bytes[32..].copy_from_slice(&[0; 32]);

    PdaSeed::new(
        Impl::hash_bytes(&bytes)
            .as_bytes()
            .try_into()
            .expect("Hash output must be exactly 32 bytes long"),
    )
}

pub struct Amm<'w>(pub &'w WalletCore);

impl Amm<'_> {
    pub async fn send_new_definition(
        &self,
        user_holding_a: AccountId,
        user_holding_b: AccountId,
        user_holding_lp: AccountId,
        balance_a: u128,
        balance_b: u128,
    ) -> Result<SendTxResponse, ExecutionFailureKind> {
        let (instruction, program) = amm_program_preparation_definition(balance_a, balance_b);

        let amm_program_id = Program::amm().id();

        let user_a_acc = self
            .0
            .get_account_public(user_holding_a)
            .await
            .map_err(|_| ExecutionFailureKind::SequencerError)?;
        let user_b_acc = self
            .0
            .get_account_public(user_holding_b)
            .await
            .map_err(|_| ExecutionFailureKind::SequencerError)?;

        let definition_token_a_id = TokenHolding::parse(&user_a_acc.data)
            .ok_or(ExecutionFailureKind::AccountDataError(user_holding_a))?
            .definition_id;
        let definition_token_b_id = TokenHolding::parse(&user_b_acc.data)
            .ok_or(ExecutionFailureKind::AccountDataError(user_holding_a))?
            .definition_id;

        let amm_pool =
            compute_pool_pda(amm_program_id, definition_token_a_id, definition_token_b_id);
        let vault_holding_a = compute_vault_pda(amm_program_id, amm_pool, definition_token_a_id);
        let vault_holding_b = compute_vault_pda(amm_program_id, amm_pool, definition_token_b_id);
        let pool_lp = compute_liquidity_token_pda(amm_program_id, amm_pool);

        let account_ids = vec![
            amm_pool,
            vault_holding_a,
            vault_holding_b,
            pool_lp,
            user_holding_a,
            user_holding_b,
            user_holding_lp,
        ];

        let nonces = self
            .0
            .get_accounts_nonces(vec![user_holding_a, user_holding_b])
            .await
            .map_err(|_| ExecutionFailureKind::SequencerError)?;

        let signing_key_a = self
            .0
            .storage
            .user_data
            .get_pub_account_signing_key(&user_holding_a)
            .ok_or(ExecutionFailureKind::KeyNotFoundError)?;

        let signing_key_b = self
            .0
            .storage
            .user_data
            .get_pub_account_signing_key(&user_holding_b)
            .ok_or(ExecutionFailureKind::KeyNotFoundError)?;

        let message = nssa::public_transaction::Message::try_new(
            program.id(),
            account_ids,
            nonces,
            instruction,
        )
        .unwrap();

        let witness_set = nssa::public_transaction::WitnessSet::for_message(
            &message,
            &[signing_key_a, signing_key_b],
        );

        let tx = nssa::PublicTransaction::new(message, witness_set);

        Ok(self.0.sequencer_client.send_tx_public(tx).await?)
    }

    pub async fn send_swap(
        &self,
        user_holding_a: AccountId,
        user_holding_b: AccountId,
        amount_in: u128,
        min_amount_out: u128,
        token_definition_id: AccountId,
    ) -> Result<SendTxResponse, ExecutionFailureKind> {
        let (instruction, program) =
            amm_program_preparation_swap(amount_in, min_amount_out, token_definition_id);

        let amm_program_id = Program::amm().id();

        let user_a_acc = self
            .0
            .get_account_public(user_holding_a)
            .await
            .map_err(|_| ExecutionFailureKind::SequencerError)?;
        let user_b_acc = self
            .0
            .get_account_public(user_holding_b)
            .await
            .map_err(|_| ExecutionFailureKind::SequencerError)?;

        let definition_token_a_id = TokenHolding::parse(&user_a_acc.data)
            .ok_or(ExecutionFailureKind::AccountDataError(user_holding_a))?
            .definition_id;
        let definition_token_b_id = TokenHolding::parse(&user_b_acc.data)
            .ok_or(ExecutionFailureKind::AccountDataError(user_holding_b))?
            .definition_id;

        let amm_pool =
            compute_pool_pda(amm_program_id, definition_token_a_id, definition_token_b_id);
        let vault_holding_a = compute_vault_pda(amm_program_id, amm_pool, definition_token_a_id);
        let vault_holding_b = compute_vault_pda(amm_program_id, amm_pool, definition_token_b_id);

        let account_ids = vec![
            amm_pool,
            vault_holding_a,
            vault_holding_b,
            user_holding_a,
            user_holding_b,
        ];

        let account_id_auth;

        // Checking, which account are associated with TokenDefinition
        let token_holder_acc_a = self
            .0
            .get_account_public(user_holding_a)
            .await
            .map_err(|_| ExecutionFailureKind::SequencerError)?;
        let token_holder_acc_b = self
            .0
            .get_account_public(user_holding_b)
            .await
            .map_err(|_| ExecutionFailureKind::SequencerError)?;

        let token_holder_a = TokenHolding::parse(&token_holder_acc_a.data)
            .ok_or(ExecutionFailureKind::AccountDataError(user_holding_a))?;
        let token_holder_b = TokenHolding::parse(&token_holder_acc_b.data)
            .ok_or(ExecutionFailureKind::AccountDataError(user_holding_b))?;

        if token_holder_a.definition_id == token_definition_id {
            account_id_auth = user_holding_a;
        } else if token_holder_b.definition_id == token_definition_id {
            account_id_auth = user_holding_b;
        } else {
            return Err(ExecutionFailureKind::AccountDataError(token_definition_id));
        }

        let nonces = self
            .0
            .get_accounts_nonces(vec![account_id_auth])
            .await
            .map_err(|_| ExecutionFailureKind::SequencerError)?;

        let signing_key = self
            .0
            .storage
            .user_data
            .get_pub_account_signing_key(&account_id_auth)
            .ok_or(ExecutionFailureKind::KeyNotFoundError)?;

        let message = nssa::public_transaction::Message::try_new(
            program.id(),
            account_ids,
            nonces,
            instruction,
        )
        .unwrap();

        let witness_set =
            nssa::public_transaction::WitnessSet::for_message(&message, &[signing_key]);

        let tx = nssa::PublicTransaction::new(message, witness_set);

        Ok(self.0.sequencer_client.send_tx_public(tx).await?)
    }

    pub async fn send_add_liquidity(
        &self,
        user_holding_a: AccountId,
        user_holding_b: AccountId,
        user_holding_lp: AccountId,
        min_amount_lp: u128,
        max_amount_a: u128,
        max_amount_b: u128,
    ) -> Result<SendTxResponse, ExecutionFailureKind> {
        let (instruction, program) =
            amm_program_preparation_add_liq(min_amount_lp, max_amount_a, max_amount_b);

        let amm_program_id = Program::amm().id();

        let user_a_acc = self
            .0
            .get_account_public(user_holding_a)
            .await
            .map_err(|_| ExecutionFailureKind::SequencerError)?;
        let user_b_acc = self
            .0
            .get_account_public(user_holding_b)
            .await
            .map_err(|_| ExecutionFailureKind::SequencerError)?;

        let definition_token_a_id = TokenHolding::parse(&user_a_acc.data)
            .ok_or(ExecutionFailureKind::AccountDataError(user_holding_a))?
            .definition_id;
        let definition_token_b_id = TokenHolding::parse(&user_b_acc.data)
            .ok_or(ExecutionFailureKind::AccountDataError(user_holding_a))?
            .definition_id;

        let amm_pool =
            compute_pool_pda(amm_program_id, definition_token_a_id, definition_token_b_id);
        let vault_holding_a = compute_vault_pda(amm_program_id, amm_pool, definition_token_a_id);
        let vault_holding_b = compute_vault_pda(amm_program_id, amm_pool, definition_token_b_id);
        let pool_lp = compute_liquidity_token_pda(amm_program_id, amm_pool);

        let account_ids = vec![
            amm_pool,
            vault_holding_a,
            vault_holding_b,
            pool_lp,
            user_holding_a,
            user_holding_b,
            user_holding_lp,
        ];

        let nonces = self
            .0
            .get_accounts_nonces(vec![user_holding_a, user_holding_b])
            .await
            .map_err(|_| ExecutionFailureKind::SequencerError)?;

        let signing_key_a = self
            .0
            .storage
            .user_data
            .get_pub_account_signing_key(&user_holding_a)
            .ok_or(ExecutionFailureKind::KeyNotFoundError)?;

        let signing_key_b = self
            .0
            .storage
            .user_data
            .get_pub_account_signing_key(&user_holding_b)
            .ok_or(ExecutionFailureKind::KeyNotFoundError)?;

        let message = nssa::public_transaction::Message::try_new(
            program.id(),
            account_ids,
            nonces,
            instruction,
        )
        .unwrap();

        let witness_set = nssa::public_transaction::WitnessSet::for_message(
            &message,
            &[signing_key_a, signing_key_b],
        );

        let tx = nssa::PublicTransaction::new(message, witness_set);

        Ok(self.0.sequencer_client.send_tx_public(tx).await?)
    }

    pub async fn send_remove_liquidity(
        &self,
        user_holding_a: AccountId,
        user_holding_b: AccountId,
        user_holding_lp: AccountId,
        balance_lp: u128,
        min_amount_a: u128,
        min_amount_b: u128,
    ) -> Result<SendTxResponse, ExecutionFailureKind> {
        let (instruction, program) =
            amm_program_preparation_remove_liq(balance_lp, min_amount_a, min_amount_b);

        let amm_program_id = Program::amm().id();

        let user_a_acc = self
            .0
            .get_account_public(user_holding_a)
            .await
            .map_err(|_| ExecutionFailureKind::SequencerError)?;
        let user_b_acc = self
            .0
            .get_account_public(user_holding_b)
            .await
            .map_err(|_| ExecutionFailureKind::SequencerError)?;

        let definition_token_a_id = TokenHolding::parse(&user_a_acc.data)
            .ok_or(ExecutionFailureKind::AccountDataError(user_holding_a))?
            .definition_id;
        let definition_token_b_id = TokenHolding::parse(&user_b_acc.data)
            .ok_or(ExecutionFailureKind::AccountDataError(user_holding_a))?
            .definition_id;

        let amm_pool =
            compute_pool_pda(amm_program_id, definition_token_a_id, definition_token_b_id);
        let vault_holding_a = compute_vault_pda(amm_program_id, amm_pool, definition_token_a_id);
        let vault_holding_b = compute_vault_pda(amm_program_id, amm_pool, definition_token_b_id);
        let pool_lp = compute_liquidity_token_pda(amm_program_id, amm_pool);

        let account_ids = vec![
            amm_pool,
            vault_holding_a,
            vault_holding_b,
            pool_lp,
            user_holding_a,
            user_holding_b,
            user_holding_lp,
        ];

        let nonces = self
            .0
            .get_accounts_nonces(vec![user_holding_lp])
            .await
            .map_err(|_| ExecutionFailureKind::SequencerError)?;

        let signing_key_lp = self
            .0
            .storage
            .user_data
            .get_pub_account_signing_key(&user_holding_lp)
            .ok_or(ExecutionFailureKind::KeyNotFoundError)?;

        let message = nssa::public_transaction::Message::try_new(
            program.id(),
            account_ids,
            nonces,
            instruction,
        )
        .unwrap();

        let witness_set =
            nssa::public_transaction::WitnessSet::for_message(&message, &[signing_key_lp]);

        let tx = nssa::PublicTransaction::new(message, witness_set);

        Ok(self.0.sequencer_client.send_tx_public(tx).await?)
    }
}

fn amm_program_preparation_definition(balance_a: u128, balance_b: u128) -> (Vec<u8>, Program) {
    // An instruction data of 65-bytes, indicating the initial amm reserves' balances and
    // token_program_id with the following layout:
    // [0x00 || array of balances (little-endian 16 bytes) || AMM_PROGRAM_ID)]
    let amm_program_id = Program::amm().id();

    let mut instruction = [0; 65];
    instruction[1..17].copy_from_slice(&balance_a.to_le_bytes());
    instruction[17..33].copy_from_slice(&balance_b.to_le_bytes());

    // This can be done less verbose, but it is better to use same way, as in amm program
    instruction[33..37].copy_from_slice(&amm_program_id[0].to_le_bytes());
    instruction[37..41].copy_from_slice(&amm_program_id[1].to_le_bytes());
    instruction[41..45].copy_from_slice(&amm_program_id[2].to_le_bytes());
    instruction[45..49].copy_from_slice(&amm_program_id[3].to_le_bytes());
    instruction[49..53].copy_from_slice(&amm_program_id[4].to_le_bytes());
    instruction[53..57].copy_from_slice(&amm_program_id[5].to_le_bytes());
    instruction[57..61].copy_from_slice(&amm_program_id[6].to_le_bytes());
    instruction[61..].copy_from_slice(&amm_program_id[7].to_le_bytes());

    let instruction_data = instruction.to_vec();
    let program = Program::amm();

    (instruction_data, program)
}

fn amm_program_preparation_swap(
    amount_in: u128,
    min_amount_out: u128,
    token_definition_id: AccountId,
) -> (Vec<u8>, Program) {
    // An instruction data byte string of length 65, indicating which token type to swap, quantity
    // of tokens put into the swap (of type TOKEN_DEFINITION_ID) and min_amount_out.
    // [0x01 || amount (little-endian 16 bytes) || TOKEN_DEFINITION_ID].
    let mut instruction = [0; 65];
    instruction[0] = 0x01;
    instruction[1..17].copy_from_slice(&amount_in.to_le_bytes());
    instruction[17..33].copy_from_slice(&min_amount_out.to_le_bytes());

    // This can be done less verbose, but it is better to use same way, as in amm program
    instruction[33..].copy_from_slice(&token_definition_id.to_bytes());

    let instruction_data = instruction.to_vec();
    let program = Program::amm();

    (instruction_data, program)
}

fn amm_program_preparation_add_liq(
    min_amount_lp: u128,
    max_amount_a: u128,
    max_amount_b: u128,
) -> (Vec<u8>, Program) {
    // An instruction data byte string of length 49, amounts for minimum amount of liquidity from
    // add (min_amount_lp), max amount added for each token (max_amount_a and max_amount_b);
    // indicate [0x02 || array of of balances (little-endian 16 bytes)].
    let mut instruction = [0; 49];
    instruction[0] = 0x02;

    instruction[1..17].copy_from_slice(&min_amount_lp.to_le_bytes());
    instruction[17..33].copy_from_slice(&max_amount_a.to_le_bytes());
    instruction[33..49].copy_from_slice(&max_amount_b.to_le_bytes());

    let instruction_data = instruction.to_vec();
    let program = Program::amm();

    (instruction_data, program)
}

fn amm_program_preparation_remove_liq(
    balance_lp: u128,
    min_amount_a: u128,
    min_amount_b: u128,
) -> (Vec<u8>, Program) {
    // An instruction data byte string of length 49, amounts for minimum amount of liquidity to
    // redeem (balance_lp), minimum balance of each token to remove (min_amount_a and
    // min_amount_b); indicate [0x03 || array of balances (little-endian 16 bytes)].
    let mut instruction = [0; 49];
    instruction[0] = 0x03;

    instruction[1..17].copy_from_slice(&balance_lp.to_le_bytes());
    instruction[17..33].copy_from_slice(&min_amount_a.to_le_bytes());
    instruction[33..49].copy_from_slice(&min_amount_b.to_le_bytes());

    let instruction_data = instruction.to_vec();
    let program = Program::amm();

    (instruction_data, program)
}
