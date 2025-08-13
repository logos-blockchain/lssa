use common::ExecutionFailureKind;
use nssa::Address;
use rand::{rngs::OsRng, RngCore};
use risc0_zkvm::{default_executor, default_prover, sha::Digest, ExecutorEnv, Receipt};
use serde::Serialize;
use utxo::utxo_core::{Randomness, UTXOPayload, UTXO};

pub mod gas_calculator;

pub use test_methods;

#[allow(clippy::result_large_err)]
pub fn gas_limits_check<INP: Serialize>(
    input_buffer: INP,
    elf: &[u8],
    gas_calculator: &gas_calculator::GasCalculator,
    attached_funds: u64,
) -> Result<(), ExecutionFailureKind> {
    let mut input_buffer_len: usize = 0;
    input_buffer_len += serde_json::to_vec(&input_buffer).unwrap().len();

    let gas_limit = gas_calculator
        .gas_runtime_full(elf, input_buffer_len)
        .ok_or(ExecutionFailureKind::InsufficientGasError)?;

    let cost = gas_calculator.runtime_cost(gas_limit);

    if cost > attached_funds {
        return Err(ExecutionFailureKind::InsufficientFundsError);
    }

    Ok(())
}

#[allow(clippy::result_large_err)]
pub fn prove_mint_utxo(
    amount_to_mint: u128,
    owner: Address,
) -> Result<(UTXO, Receipt), ExecutionFailureKind> {
    let mut builder = ExecutorEnv::builder();

    builder
        .write(&amount_to_mint)
        .map_err(ExecutionFailureKind::write_error)?;
    builder
        .write(&owner)
        .map_err(ExecutionFailureKind::write_error)?;

    let mut randomness = Randomness::default();
    OsRng.fill_bytes(&mut randomness);
    builder
        .write(&randomness)
        .map_err(ExecutionFailureKind::write_error)?;

    let env = builder
        .build()
        .map_err(ExecutionFailureKind::builder_error)?;

    let prover = default_prover();

    let receipt = prover
        .prove(env, test_methods::MINT_UTXO_ELF)
        .map_err(ExecutionFailureKind::prove_error)?
        .receipt;

    let digest: UTXOPayload = receipt
        .journal
        .decode()
        .map_err(|e| ExecutionFailureKind::DecodeError(e.to_string()))?;

    Ok((UTXO::create_utxo_from_payload(digest), receipt))
}

#[allow(clippy::result_large_err)]
pub fn prove_send_utxo(
    spent_utxo: UTXO,
    owners_parts: Vec<(u128, Address)>,
) -> Result<(Vec<(UTXO, Address)>, Receipt), ExecutionFailureKind> {
    let cumulative_spent = owners_parts.iter().fold(0, |acc, item| acc + item.0);

    if cumulative_spent != spent_utxo.amount {
        return Err(ExecutionFailureKind::AmountMismatchError);
    }

    let mut builder = ExecutorEnv::builder();
    let utxo_payload = spent_utxo.into_payload();

    builder
        .write(&utxo_payload)
        .map_err(ExecutionFailureKind::write_error)?;

    let owners_parts_with_randomness = owners_parts
        .into_iter()
        .map(|(amount, addr)| {
            let mut randomness = Randomness::default();
            OsRng.fill_bytes(&mut randomness);
            (amount, addr, randomness)
        })
        .collect::<Vec<_>>();

    builder
        .write(&owners_parts_with_randomness)
        .map_err(ExecutionFailureKind::write_error)?;

    let env = builder
        .build()
        .map_err(ExecutionFailureKind::builder_error)?;

    let prover = default_prover();

    let receipt = prover
        .prove(env, test_methods::SEND_UTXO_ELF)
        .map_err(ExecutionFailureKind::prove_error)?
        .receipt;

    let digest: Vec<(UTXOPayload, Address)> = receipt
        .journal
        .decode()
        .map_err(|e| ExecutionFailureKind::DecodeError(e.to_string()))?;

    Ok((
        digest
            .into_iter()
            .map(|(payload, addr)| (UTXO::create_utxo_from_payload(payload), addr))
            .collect(),
        receipt,
    ))
}

#[allow(clippy::result_large_err)]
pub fn prove_send_utxo_multiple_assets_one_receiver(
    spent_utxos: Vec<UTXO>,
    number_to_send: usize,
    receiver: Address,
) -> Result<(Vec<UTXO>, Vec<UTXO>, Receipt), ExecutionFailureKind> {
    if number_to_send > spent_utxos.len() {
        return Err(ExecutionFailureKind::AmountMismatchError);
    }

    let mut builder = ExecutorEnv::builder();
    let utxo_payload: Vec<UTXOPayload> = spent_utxos
        .into_iter()
        .map(|spent_utxo| spent_utxo.into_payload())
        .collect();

    builder
        .write(&utxo_payload)
        .map_err(ExecutionFailureKind::write_error)?;
    builder
        .write(&number_to_send)
        .map_err(ExecutionFailureKind::write_error)?;
    builder
        .write(&receiver)
        .map_err(ExecutionFailureKind::write_error)?;

    let env = builder
        .build()
        .map_err(ExecutionFailureKind::builder_error)?;

    let prover = default_prover();

    let receipt = prover
        .prove(env, test_methods::SEND_UTXO_MULTIPLE_ASSETS_ELF)
        .map_err(ExecutionFailureKind::prove_error)?
        .receipt;

    let digest: (Vec<UTXOPayload>, Vec<UTXOPayload>) = receipt
        .journal
        .decode()
        .map_err(|e| ExecutionFailureKind::DecodeError(e.to_string()))?;

    Ok((
        digest
            .0
            .into_iter()
            .map(UTXO::create_utxo_from_payload)
            .collect(),
        digest
            .1
            .into_iter()
            .map(UTXO::create_utxo_from_payload)
            .collect(),
        receipt,
    ))
}

#[allow(clippy::result_large_err)]
pub fn prove_send_utxo_shielded(
    owner: Address,
    amount: u128,
    owners_parts: Vec<(u128, Address)>,
) -> Result<(Vec<(UTXO, Address)>, Receipt), ExecutionFailureKind> {
    let cumulative_spent = owners_parts.iter().fold(0, |acc, item| acc + item.0);

    if cumulative_spent != amount {
        return Err(ExecutionFailureKind::AmountMismatchError);
    }

    let temp_utxo_to_spend = UTXO::new(*owner.value(), vec![], amount, true);
    let utxo_payload = temp_utxo_to_spend.into_payload();

    let mut builder = ExecutorEnv::builder();

    builder
        .write(&utxo_payload)
        .map_err(ExecutionFailureKind::write_error)?;

    let owners_parts_with_randomness = owners_parts
        .into_iter()
        .map(|(amount, addr)| {
            let mut randomness = Randomness::default();
            OsRng.fill_bytes(&mut randomness);
            (amount, addr, randomness)
        })
        .collect::<Vec<_>>();

    builder
        .write(&owners_parts_with_randomness)
        .map_err(ExecutionFailureKind::write_error)?;

    let env = builder
        .build()
        .map_err(ExecutionFailureKind::builder_error)?;

    let prover = default_prover();

    let receipt = prover
        .prove(env, test_methods::SEND_UTXO_ELF)
        .map_err(ExecutionFailureKind::prove_error)?
        .receipt;

    let digest: Vec<(UTXOPayload, Address)> = receipt
        .journal
        .decode()
        .map_err(|e| ExecutionFailureKind::DecodeError(e.to_string()))?;

    Ok((
        digest
            .into_iter()
            .map(|(payload, addr)| (UTXO::create_utxo_from_payload(payload), addr))
            .collect(),
        receipt,
    ))
}

#[allow(clippy::result_large_err)]
pub fn prove_send_utxo_deshielded(
    spent_utxo: UTXO,
    owners_parts: Vec<(u128, Address)>,
) -> Result<(Vec<(u128, Address)>, Receipt), ExecutionFailureKind> {
    let cumulative_spent = owners_parts.iter().fold(0, |acc, item| acc + item.0);

    if cumulative_spent != spent_utxo.amount {
        return Err(ExecutionFailureKind::AmountMismatchError);
    }

    let mut builder = ExecutorEnv::builder();
    let utxo_payload = spent_utxo.into_payload();

    builder
        .write(&utxo_payload)
        .map_err(ExecutionFailureKind::write_error)?;

    let owners_parts_with_randomness = owners_parts
        .into_iter()
        .map(|(amount, addr)| {
            let mut randomness = Randomness::default();
            OsRng.fill_bytes(&mut randomness);
            (amount, addr, randomness)
        })
        .collect::<Vec<_>>();

    builder
        .write(&owners_parts_with_randomness)
        .map_err(ExecutionFailureKind::write_error)?;

    let env = builder
        .build()
        .map_err(ExecutionFailureKind::builder_error)?;

    let prover = default_prover();

    let receipt = prover
        .prove(env, test_methods::SEND_UTXO_ELF)
        .map_err(ExecutionFailureKind::prove_error)?
        .receipt;

    let digest: Vec<(UTXOPayload, Address)> = receipt
        .journal
        .decode()
        .map_err(|e| ExecutionFailureKind::DecodeError(e.to_string()))?;

    Ok((
        digest
            .into_iter()
            .map(|(payload, addr)| (payload.amount, addr))
            .collect(),
        receipt,
    ))
}

#[allow(clippy::result_large_err)]
pub fn prove_mint_utxo_multiple_assets(
    amount_to_mint: u128,
    number_of_assets: usize,
    owner: Address,
) -> Result<(Vec<UTXO>, Receipt), ExecutionFailureKind> {
    let mut builder = ExecutorEnv::builder();

    builder
        .write(&amount_to_mint)
        .map_err(ExecutionFailureKind::write_error)?;
    builder
        .write(&number_of_assets)
        .map_err(ExecutionFailureKind::write_error)?;
    builder
        .write(&owner)
        .map_err(ExecutionFailureKind::write_error)?;

    let env = builder
        .build()
        .map_err(ExecutionFailureKind::builder_error)?;

    let prover = default_prover();

    let receipt = prover
        .prove(env, test_methods::MINT_UTXO_MULTIPLE_ASSETS_ELF)
        .map_err(ExecutionFailureKind::prove_error)?
        .receipt;

    let digest: Vec<UTXOPayload> = receipt
        .journal
        .decode()
        .map_err(|e| ExecutionFailureKind::DecodeError(e.to_string()))?;

    Ok((
        digest
            .into_iter()
            .map(UTXO::create_utxo_from_payload)
            .collect(),
        receipt,
    ))
}

pub fn execute_mint_utxo(
    amount_to_mint: u128,
    owner: Address,
    randomness: [u8; 32],
) -> anyhow::Result<UTXO> {
    let mut builder = ExecutorEnv::builder();

    builder.write(&amount_to_mint)?;
    builder.write(&owner)?;
    builder.write(&randomness)?;

    let env = builder.build()?;

    let executor = default_executor();

    let receipt = executor.execute(env, test_methods::MINT_UTXO_ELF)?;

    let digest: UTXOPayload = receipt.journal.decode()?;

    Ok(UTXO::create_utxo_from_payload(digest))
}

pub fn execute_send_utxo(
    spent_utxo: UTXO,
    owners_parts: Vec<(u128, Address)>,
) -> anyhow::Result<(UTXO, Vec<(UTXO, Address)>)> {
    let mut builder = ExecutorEnv::builder();

    let utxo_payload = spent_utxo.into_payload();

    builder.write(&utxo_payload)?;
    let owners_parts_with_randomness = owners_parts
        .into_iter()
        .map(|(amount, addr)| {
            let mut randomness = Randomness::default();
            OsRng.fill_bytes(&mut randomness);
            (amount, addr, randomness)
        })
        .collect::<Vec<_>>();

    builder.write(&owners_parts_with_randomness)?;

    let env = builder.build()?;

    let executor = default_executor();

    let receipt = executor.execute(env, test_methods::SEND_UTXO_ELF)?;

    let digest: (UTXOPayload, Vec<(UTXOPayload, Address)>) = receipt.journal.decode()?;

    Ok((
        UTXO::create_utxo_from_payload(digest.0),
        digest
            .1
            .into_iter()
            .map(|(payload, addr)| (UTXO::create_utxo_from_payload(payload), addr))
            .collect(),
    ))
}

pub fn prove<T: serde::ser::Serialize>(
    input_vec: Vec<T>,
    elf: &[u8],
) -> anyhow::Result<(u64, Receipt)> {
    let mut builder = ExecutorEnv::builder();

    for input in input_vec {
        builder.write(&input)?;
    }

    let env = builder.build()?;

    let prover = default_prover();

    let receipt = prover.prove(env, elf)?.receipt;

    let digest = receipt.journal.decode()?;
    Ok((digest, receipt))
}

// This only executes the program and does not generate a receipt.
pub fn execute<T: serde::ser::Serialize + for<'de> serde::Deserialize<'de>>(
    input_vec: Vec<T>,
    elf: &[u8],
) -> anyhow::Result<T> {
    let mut builder = ExecutorEnv::builder();

    for input in input_vec {
        builder.write(&input)?;
    }

    let env = builder.build()?;

    let exec = default_executor();
    let session = exec.execute(env, elf)?;

    // We read the result committed to the journal by the guest code.
    let result: T = session.journal.decode()?;

    Ok(result)
}

pub fn verify(receipt: Receipt, image_id: impl Into<Digest>) -> anyhow::Result<()> {
    Ok(receipt.verify(image_id)?)
}

#[cfg(test)]
mod tests {
    use crate::gas_calculator::GasCalculator;

    use super::*;
    use test_methods::BIG_CALCULATION_ELF;
    use test_methods::{MULTIPLICATION_ELF, MULTIPLICATION_ID};
    use test_methods::{SUMMATION_ELF, SUMMATION_ID};

    #[test]
    fn prove_simple_sum() {
        let message = 1;
        let message_2 = 2;

        let (digest, receipt) = prove(vec![message, message_2], SUMMATION_ELF).unwrap();

        verify(receipt, SUMMATION_ID).unwrap();
        assert_eq!(digest, message + message_2);
    }

    #[test]
    fn prove_bigger_sum() {
        let message = 123476;
        let message_2 = 2342384;

        let (digest, receipt) = prove(vec![message, message_2], SUMMATION_ELF).unwrap();

        verify(receipt, SUMMATION_ID).unwrap();
        assert_eq!(digest, message + message_2);
    }

    #[test]
    fn prove_simple_multiplication() {
        let message = 1;
        let message_2 = 2;

        let (digest, receipt) = prove(vec![message, message_2], MULTIPLICATION_ELF).unwrap();

        verify(receipt, MULTIPLICATION_ID).unwrap();
        assert_eq!(digest, message * message_2);
    }

    #[test]
    fn prove_bigger_multiplication() {
        let message = 3498;
        let message_2 = 438563;

        let (digest, receipt) = prove(vec![message, message_2], MULTIPLICATION_ELF).unwrap();

        verify(receipt, MULTIPLICATION_ID).unwrap();
        assert_eq!(digest, message * message_2);
    }

    #[test]
    fn execute_simple_sum() {
        let message: u64 = 1;
        let message_2: u64 = 2;

        let result = execute(vec![message, message_2], SUMMATION_ELF).unwrap();
        assert_eq!(result, message + message_2);
    }

    #[test]
    fn execute_bigger_sum() {
        let message: u64 = 123476;
        let message_2: u64 = 2342384;

        let result = execute(vec![message, message_2], SUMMATION_ELF).unwrap();
        assert_eq!(result, message + message_2);
    }

    #[test]
    fn execute_big_calculation() {
        let message: u128 = 1;
        let message_2: u128 = 2;

        let result = execute(vec![message, message_2], BIG_CALCULATION_ELF).unwrap();
        assert_eq!(result, big_calculation(message, message_2));
    }

    #[test]
    fn execute_big_calculation_long() {
        let message: u128 = 20;
        let message_2: u128 = 10;

        let result = execute(vec![message, message_2], BIG_CALCULATION_ELF).unwrap();
        assert_eq!(result, big_calculation(message, message_2));
    }

    fn big_calculation(lhs: u128, rhs: u128) -> u128 {
        let mut res = 1_u128;
        for _ in 0..lhs {
            res *= rhs;
            res += lhs;
        }

        res
    }

    #[test]
    fn test_gas_limits_check_sufficient_funds() {
        let message = 1;
        let message_2 = 2;
        let gas_calc = GasCalculator::new(1, 1, 1, 1, 1, 1000000, 1000000);

        let result = gas_limits_check(vec![message, message_2], SUMMATION_ELF, &gas_calc, 1000000);
        assert!(result.is_ok());
    }

    #[test]
    fn test_gas_limits_check_insufficient_funds() {
        let message = 1;
        let message_2 = 2;
        let gas_calc = GasCalculator::new(1, 1, 1, 1, 1, 1000000, 1000000);

        let result = gas_limits_check(vec![message, message_2], SUMMATION_ELF, &gas_calc, 1);
        assert!(matches!(
            result,
            Err(ExecutionFailureKind::InsufficientFundsError)
        ));
    }

    #[test]
    fn test_execute_mint_utxo() {
        let owner = Address::default();
        let amount = 123456789;
        let mut randomness = [0u8; 32];
        OsRng.fill_bytes(&mut randomness);

        let utxo_exec = execute_mint_utxo(amount, owner, randomness).expect("execution failed");
        assert_eq!(utxo_exec.amount, amount);
        assert_eq!(utxo_exec.owner, *owner.value());
    }

    #[test]
    fn test_prove_mint_utxo() {
        let owner = Address::default();
        let amount = 123456789;

        let (utxo, _) = prove_mint_utxo(amount, owner).expect("proof failed");
        assert_eq!(utxo.amount, amount);
        assert_eq!(utxo.owner, *owner.value());
    }

    #[test]
    fn test_prove_send_utxo() {
        let owner = Address::default();
        let amount = 100;
        let (input_utxo, _) = prove_mint_utxo(amount, owner).expect("mint failed");

        let parts = vec![(40, owner), (60, owner)];
        let (outputs, _receipt) = prove_send_utxo(input_utxo, parts.clone()).expect("send failed");

        let total: u128 = outputs.iter().map(|(utxo, _)| utxo.amount).sum();
        assert_eq!(total, amount);
        assert_eq!(outputs.len(), 2);
    }

    #[test]
    fn test_prove_send_utxo_deshielded() {
        let owner = Address::default();
        let amount = 100;
        let (utxo, _) = prove_mint_utxo(amount, owner).unwrap();
        let parts = vec![(60, owner), (40, owner)];

        let (outputs, _) = prove_send_utxo_deshielded(utxo, parts.clone()).unwrap();

        let total: u128 = outputs.iter().map(|(amt, _)| amt).sum();
        assert_eq!(total, amount);
        assert_eq!(outputs.len(), 2);
    }

    #[test]
    fn test_prove_send_utxo_shielded() {
        let owner = Address::default();
        let amount = 100;
        let parts = vec![(60, owner), (40, owner)];

        let (outputs, _) = prove_send_utxo_shielded(owner, amount, parts.clone()).unwrap();

        let total: u128 = outputs.iter().map(|(utxo, _)| utxo.amount).sum();
        assert_eq!(total, amount);
        assert_eq!(outputs.len(), 2);
    }

    #[test]
    fn test_prove_send_utxo_multiple_assets_one_receiver() {
        let owner = Address::default();
        let receiver = Address::default();

        let utxos = vec![
            prove_mint_utxo(100, owner).unwrap().0,
            prove_mint_utxo(50, owner).unwrap().0,
        ];

        let (to_receiver, to_change, _receipt) =
            prove_send_utxo_multiple_assets_one_receiver(utxos, 1, receiver).unwrap();
        let total_to_receiver: u128 = to_receiver.iter().map(|u| u.amount).sum();

        assert!(total_to_receiver > 0);
        assert_eq!(to_receiver.len() + to_change.len(), 2);
    }
}
