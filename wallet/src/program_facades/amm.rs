use common::{error::ExecutionFailureKind, rpc_primitives::requests::SendTxResponse};
use nssa::{AccountId, program::Program};
use nssa_core::{SharedSecretKey, program::InstructionData};
use serde::Serialize;

use crate::{PrivacyPreservingAccount, WalletCore};

struct OrphanHack65BytesInput([u8; 65]);

impl Serialize for OrphanHack65BytesInput {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_bytes(&self.0)
    }
}

struct OrphanHack49BytesInput([u8; 49]);

impl Serialize for OrphanHack49BytesInput {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_bytes(&self.0)
    }
}

pub struct AMM<'w>(pub &'w WalletCore);

impl AMM<'_> {
    #[allow(clippy::too_many_arguments)]
    pub async fn send_new_amm_definition(
        &self,
        _amm_pool: PrivacyPreservingAccount,
        _vault_holding_a: PrivacyPreservingAccount,
        _vault_holding_b: PrivacyPreservingAccount,
        _pool_lp: PrivacyPreservingAccount,
        _user_holding_a: PrivacyPreservingAccount,
        _user_holding_b: PrivacyPreservingAccount,
        _user_holding_lp: PrivacyPreservingAccount,
        _balance_a: u128,
        _balance_b: u128,
    ) -> Result<SendTxResponse, ExecutionFailureKind> {
        todo!()
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn send_new_amm_definition_privacy_preserving(
        &self,
        _amm_pool: PrivacyPreservingAccount,
        _vault_holding_a: PrivacyPreservingAccount,
        _vault_holding_b: PrivacyPreservingAccount,
        _pool_lp: PrivacyPreservingAccount,
        _user_holding_a: PrivacyPreservingAccount,
        _user_holding_b: PrivacyPreservingAccount,
        _user_holding_lp: PrivacyPreservingAccount,
        _balance_a: u128,
        _balance_b: u128,
    ) -> Result<(SendTxResponse, [Option<SharedSecretKey>; 7]), ExecutionFailureKind> {
        todo!()
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn send_swap(
        &self,
        amm_pool: PrivacyPreservingAccount,
        vault_holding_1: PrivacyPreservingAccount,
        vault_holding_2: PrivacyPreservingAccount,
        user_holding_a: PrivacyPreservingAccount,
        user_holding_b: PrivacyPreservingAccount,
        amount_in: u128,
        min_amount_out: u128,
        token_definition_id: AccountId,
    ) -> Result<SendTxResponse, ExecutionFailureKind> {
        let (instruction, program) =
            amm_program_preparation_swap(amount_in, min_amount_out, token_definition_id);

        match (
            amm_pool,
            vault_holding_1,
            vault_holding_2,
            user_holding_a,
            user_holding_b,
        ) {
            (
                PrivacyPreservingAccount::Public(amm_pool),
                PrivacyPreservingAccount::Public(vault_holding_1),
                PrivacyPreservingAccount::Public(vault_holding_2),
                PrivacyPreservingAccount::Public(user_holding_a),
                PrivacyPreservingAccount::Public(user_holding_b),
            ) => {
                let account_ids = vec![
                    amm_pool,
                    vault_holding_1,
                    vault_holding_2,
                    user_holding_a,
                    user_holding_b,
                ];

                // ToDo: Correct authorization
                // ToDo: Also correct instruction serialization

                let message = nssa::public_transaction::Message::try_new(
                    program.id(),
                    account_ids,
                    vec![],
                    instruction,
                )
                .unwrap();

                let witness_set = nssa::public_transaction::WitnessSet::for_message(&message, &[]);

                let tx = nssa::PublicTransaction::new(message, witness_set);

                Ok(self.0.sequencer_client.send_tx_public(tx).await?)
            }
            _ => unreachable!(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn send_swap_privacy_preserving(
        &self,
        amm_pool: PrivacyPreservingAccount,
        vault_holding_1: PrivacyPreservingAccount,
        vault_holding_2: PrivacyPreservingAccount,
        user_holding_a: PrivacyPreservingAccount,
        user_holding_b: PrivacyPreservingAccount,
        amount_in: u128,
        min_amount_out: u128,
        token_definition_id: AccountId,
    ) -> Result<(SendTxResponse, [Option<SharedSecretKey>; 5]), ExecutionFailureKind> {
        let (instruction_data, program) =
            amm_program_preparation_swap(amount_in, min_amount_out, token_definition_id);

        self.0
            .send_privacy_preserving_tx(
                vec![
                    amm_pool.clone(),
                    vault_holding_1.clone(),
                    vault_holding_2.clone(),
                    user_holding_a.clone(),
                    user_holding_b.clone(),
                ],
                &instruction_data,
                &program,
            )
            .await
            .map(|(resp, secrets)| {
                let mut secrets = secrets.into_iter();
                let mut secrets_res = [None; 5];

                for acc_id in [
                    amm_pool,
                    vault_holding_1,
                    vault_holding_2,
                    user_holding_a,
                    user_holding_b,
                ]
                .iter()
                .enumerate()
                {
                    if acc_id.1.is_private() {
                        let secret = secrets.next().expect("expected next secret");

                        secrets_res[acc_id.0] = Some(secret);
                    }
                }

                (resp, secrets_res)
            })
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn send_add_liq(
        &self,
        amm_pool: PrivacyPreservingAccount,
        vault_holding_a: PrivacyPreservingAccount,
        vault_holding_b: PrivacyPreservingAccount,
        pool_lp: PrivacyPreservingAccount,
        user_holding_a: PrivacyPreservingAccount,
        user_holding_b: PrivacyPreservingAccount,
        user_holding_lp: PrivacyPreservingAccount,
        min_amount_lp: u128,
        max_amount_a: u128,
        max_amount_b: u128,
    ) -> Result<SendTxResponse, ExecutionFailureKind> {
        let (instruction, program) =
            amm_program_preparation_add_liq(min_amount_lp, max_amount_a, max_amount_b);

        match (
            amm_pool,
            vault_holding_a,
            vault_holding_b,
            pool_lp,
            user_holding_a,
            user_holding_b,
            user_holding_lp,
        ) {
            (
                PrivacyPreservingAccount::Public(amm_pool),
                PrivacyPreservingAccount::Public(vault_holding_a),
                PrivacyPreservingAccount::Public(vault_holding_b),
                PrivacyPreservingAccount::Public(pool_lp),
                PrivacyPreservingAccount::Public(user_holding_a),
                PrivacyPreservingAccount::Public(user_holding_b),
                PrivacyPreservingAccount::Public(user_holding_lp),
            ) => {
                let account_ids = vec![
                    amm_pool,
                    vault_holding_a,
                    vault_holding_b,
                    pool_lp,
                    user_holding_a,
                    user_holding_b,
                    user_holding_lp,
                ];

                // ToDo: Correct authorization
                // ToDo: Also correct instruction serialization

                let message = nssa::public_transaction::Message::try_new(
                    program.id(),
                    account_ids,
                    vec![],
                    instruction,
                )
                .unwrap();

                let witness_set = nssa::public_transaction::WitnessSet::for_message(&message, &[]);

                let tx = nssa::PublicTransaction::new(message, witness_set);

                Ok(self.0.sequencer_client.send_tx_public(tx).await?)
            }
            _ => unreachable!(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn send_add_liq_privacy_preserving(
        &self,
        amm_pool: PrivacyPreservingAccount,
        vault_holding_a: PrivacyPreservingAccount,
        vault_holding_b: PrivacyPreservingAccount,
        pool_lp: PrivacyPreservingAccount,
        user_holding_a: PrivacyPreservingAccount,
        user_holding_b: PrivacyPreservingAccount,
        user_holding_lp: PrivacyPreservingAccount,
        min_amount_lp: u128,
        max_amount_a: u128,
        max_amount_b: u128,
    ) -> Result<(SendTxResponse, [Option<SharedSecretKey>; 7]), ExecutionFailureKind> {
        let (instruction_data, program) =
            amm_program_preparation_add_liq(min_amount_lp, max_amount_a, max_amount_b);

        self.0
            .send_privacy_preserving_tx(
                vec![
                    amm_pool.clone(),
                    vault_holding_a.clone(),
                    vault_holding_b.clone(),
                    pool_lp.clone(),
                    user_holding_a.clone(),
                    user_holding_b.clone(),
                    user_holding_lp.clone(),
                ],
                &instruction_data,
                &program,
            )
            .await
            .map(|(resp, secrets)| {
                let mut secrets = secrets.into_iter();
                let mut secrets_res = [None; 7];

                for acc_id in [
                    amm_pool,
                    vault_holding_a,
                    vault_holding_b,
                    pool_lp,
                    user_holding_a,
                    user_holding_b,
                    user_holding_lp,
                ]
                .iter()
                .enumerate()
                {
                    if acc_id.1.is_private() {
                        let secret = secrets.next().expect("expected next secret");

                        secrets_res[acc_id.0] = Some(secret);
                    }
                }

                (resp, secrets_res)
            })
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn send_remove_liq(
        &self,
        amm_pool: PrivacyPreservingAccount,
        vault_holding_a: PrivacyPreservingAccount,
        vault_holding_b: PrivacyPreservingAccount,
        pool_lp: PrivacyPreservingAccount,
        user_holding_a: PrivacyPreservingAccount,
        user_holding_b: PrivacyPreservingAccount,
        user_holding_lp: PrivacyPreservingAccount,
        balance_lp: u128,
        max_amount_a: u128,
        max_amount_b: u128,
    ) -> Result<SendTxResponse, ExecutionFailureKind> {
        let (instruction, program) =
            amm_program_preparation_remove_liq(balance_lp, max_amount_a, max_amount_b);

        match (
            amm_pool,
            vault_holding_a,
            vault_holding_b,
            pool_lp,
            user_holding_a,
            user_holding_b,
            user_holding_lp,
        ) {
            (
                PrivacyPreservingAccount::Public(amm_pool),
                PrivacyPreservingAccount::Public(vault_holding_a),
                PrivacyPreservingAccount::Public(vault_holding_b),
                PrivacyPreservingAccount::Public(pool_lp),
                PrivacyPreservingAccount::Public(user_holding_a),
                PrivacyPreservingAccount::Public(user_holding_b),
                PrivacyPreservingAccount::Public(user_holding_lp),
            ) => {
                let account_ids = vec![
                    amm_pool,
                    vault_holding_a,
                    vault_holding_b,
                    pool_lp,
                    user_holding_a,
                    user_holding_b,
                    user_holding_lp,
                ];

                // ToDo: Correct authorization
                // ToDo: Also correct instruction serialization

                let message = nssa::public_transaction::Message::try_new(
                    program.id(),
                    account_ids,
                    vec![],
                    instruction,
                )
                .unwrap();

                let witness_set = nssa::public_transaction::WitnessSet::for_message(&message, &[]);

                let tx = nssa::PublicTransaction::new(message, witness_set);

                Ok(self.0.sequencer_client.send_tx_public(tx).await?)
            }
            _ => unreachable!(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn send_remove_liq_privacy_preserving(
        &self,
        amm_pool: PrivacyPreservingAccount,
        vault_holding_a: PrivacyPreservingAccount,
        vault_holding_b: PrivacyPreservingAccount,
        pool_lp: PrivacyPreservingAccount,
        user_holding_a: PrivacyPreservingAccount,
        user_holding_b: PrivacyPreservingAccount,
        user_holding_lp: PrivacyPreservingAccount,
        balance_lp: u128,
        max_amount_a: u128,
        max_amount_b: u128,
    ) -> Result<(SendTxResponse, [Option<SharedSecretKey>; 7]), ExecutionFailureKind> {
        let (instruction_data, program) =
            amm_program_preparation_remove_liq(balance_lp, max_amount_a, max_amount_b);

        self.0
            .send_privacy_preserving_tx(
                vec![
                    amm_pool.clone(),
                    vault_holding_a.clone(),
                    vault_holding_b.clone(),
                    pool_lp.clone(),
                    user_holding_a.clone(),
                    user_holding_b.clone(),
                    user_holding_lp.clone(),
                ],
                &instruction_data,
                &program,
            )
            .await
            .map(|(resp, secrets)| {
                let mut secrets = secrets.into_iter();
                let mut secrets_res = [None; 7];

                for acc_id in [
                    amm_pool,
                    vault_holding_a,
                    vault_holding_b,
                    pool_lp,
                    user_holding_a,
                    user_holding_b,
                    user_holding_lp,
                ]
                .iter()
                .enumerate()
                {
                    if acc_id.1.is_private() {
                        let secret = secrets.next().expect("expected next secret");

                        secrets_res[acc_id.0] = Some(secret);
                    }
                }

                (resp, secrets_res)
            })
    }
}

#[allow(unused)]
fn amm_program_preparation_definition(
    balance_a: u128,
    balance_b: u128,
) -> (InstructionData, Program) {
    // An instruction data of 65-bytes, indicating the initial amm reserves' balances and
    // token_program_id with the following layout:
    // [0x00 || array of balances (little-endian 16 bytes) || AMM_PROGRAM_ID)]
    let amm_program_id = Program::token().id();

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

    let instruction_data =
        Program::serialize_instruction(OrphanHack65BytesInput(instruction)).unwrap();
    let program = Program::token();

    (instruction_data, program)
}

fn amm_program_preparation_swap(
    amount_in: u128,
    min_amount_out: u128,
    token_definition_id: AccountId,
) -> (InstructionData, Program) {
    // An instruction data byte string of length 65, indicating which token type to swap, quantity
    // of tokens put into the swap (of type TOKEN_DEFINITION_ID) and min_amount_out.
    // [0x01 || amount (little-endian 16 bytes) || TOKEN_DEFINITION_ID].
    let mut instruction = [0; 65];
    instruction[1..17].copy_from_slice(&amount_in.to_le_bytes());
    instruction[17..33].copy_from_slice(&min_amount_out.to_le_bytes());

    // This can be done less verbose, but it is better to use same way, as in amm program
    instruction[33..].copy_from_slice(&token_definition_id.to_bytes());

    let instruction_data =
        Program::serialize_instruction(OrphanHack65BytesInput(instruction)).unwrap();
    let program = Program::token();

    (instruction_data, program)
}

fn amm_program_preparation_add_liq(
    min_amount_lp: u128,
    max_amount_a: u128,
    max_amount_b: u128,
) -> (InstructionData, Program) {
    // An instruction data byte string of length 49, amounts for minimum amount of liquidity from
    // add (min_amount_lp), max amount added for each token (max_amount_a and max_amount_b);
    // indicate [0x02 || array of of balances (little-endian 16 bytes)].
    let mut instruction = [0; 49];
    instruction[0] = 0x02;

    instruction[1..17].copy_from_slice(&min_amount_lp.to_le_bytes());
    instruction[17..33].copy_from_slice(&max_amount_a.to_le_bytes());
    instruction[33..49].copy_from_slice(&max_amount_b.to_le_bytes());

    let instruction_data =
        Program::serialize_instruction(OrphanHack49BytesInput(instruction)).unwrap();
    let program = Program::token();

    (instruction_data, program)
}

fn amm_program_preparation_remove_liq(
    balance_lp: u128,
    max_amount_a: u128,
    max_amount_b: u128,
) -> (InstructionData, Program) {
    // An instruction data byte string of length 49, amounts for minimum amount of liquidity to
    // redeem (balance_lp), minimum balance of each token to remove (min_amount_a and
    // min_amount_b); indicate [0x03 || array of balances (little-endian 16 bytes)].
    let mut instruction = [0; 49];
    instruction[0] = 0x03;

    instruction[1..17].copy_from_slice(&balance_lp.to_le_bytes());
    instruction[17..33].copy_from_slice(&max_amount_a.to_le_bytes());
    instruction[33..49].copy_from_slice(&max_amount_b.to_le_bytes());

    let instruction_data =
        Program::serialize_instruction(OrphanHack49BytesInput(instruction)).unwrap();
    let program = Program::token();

    (instruction_data, program)
}
