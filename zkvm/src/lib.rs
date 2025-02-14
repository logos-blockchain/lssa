use accounts::account_core::AccountAddress;
use common::ExecutionFailureKind;
use risc0_zkvm::{default_executor, default_prover, sha::Digest, ExecutorEnv, Receipt};
use utxo::utxo_core::{UTXOPayload, UTXO};

pub fn prove_mint_utxo(
    amount_to_mint: u128,
    owner: AccountAddress,
) -> Result<(UTXO, Receipt), ExecutionFailureKind> {
    let mut builder = ExecutorEnv::builder();

    builder
        .write(&amount_to_mint)
        .map_err(ExecutionFailureKind::write_error)?;
    builder
        .write(&owner)
        .map_err(ExecutionFailureKind::write_error)?;

    let env = builder
        .build()
        .map_err(ExecutionFailureKind::builder_error)?;

    let prover = default_prover();

    let receipt = prover
        .prove(env, test_methods::MINT_UTXO_ELF)
        .map_err(ExecutionFailureKind::prove_error)?
        .receipt;

    let digest: UTXOPayload = receipt.journal.decode()?;

    Ok((
        UTXO::create_utxo_from_payload(digest).map_err(ExecutionFailureKind::write_error)?,
        receipt,
    ))
}

pub fn prove_send_utxo(
    spent_utxo: UTXO,
    owners_parts: Vec<(u128, AccountAddress)>,
) -> Result<(Vec<(UTXO, AccountAddress)>, Receipt), ExecutionFailureKind> {
    let cumulative_spent = owners_parts.iter().fold(0, |acc, item| acc + item.0);

    if cumulative_spent != spent_utxo.amount {
        return Err(ExecutionFailureKind::AmountMismatchError);
    }

    let mut builder = ExecutorEnv::builder();
    let utxo_payload = spent_utxo.into_payload();

    builder
        .write(&utxo_payload)
        .map_err(ExecutionFailureKind::write_error)?;
    builder
        .write(&owners_parts)
        .map_err(ExecutionFailureKind::write_error)?;

    let env = builder
        .build()
        .map_err(ExecutionFailureKind::builder_error)?;

    let prover = default_prover();

    let receipt = prover
        .prove(env, test_methods::SEND_UTXO_ELF)
        .map_err(ExecutionFailureKind::prove_error)?
        .receipt;

    let digest: Vec<(UTXOPayload, AccountAddress)> = receipt.journal.decode()?;

    Ok((
        digest
            .into_iter()
            .map(|(payload, addr)| (UTXO::create_utxo_from_payload(payload).map(|sel| (sel, addr))))
            .collect::<anyhow::Result<Vec<(UTXO, [u8; 32])>>>()
            .map_err(ExecutionFailureKind::write_error)?,
        receipt,
    ))
}

pub fn prove_send_utxo_multiple_assets_one_receiver(
    spent_utxos: Vec<UTXO>,
    number_to_send: usize,
    receiver: AccountAddress,
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

    let digest: (Vec<UTXOPayload>, Vec<UTXOPayload>) = receipt.journal.decode()?;

    Ok((
        digest
            .0
            .into_iter()
            .map(|payload| UTXO::create_utxo_from_payload(payload))
            .collect::<anyhow::Result<Vec<UTXO>>>()
            .map_err(ExecutionFailureKind::write_error)?,
        digest
            .1
            .into_iter()
            .map(|payload| UTXO::create_utxo_from_payload(payload))
            .collect::<anyhow::Result<Vec<UTXO>>>()
            .map_err(ExecutionFailureKind::write_error)?,
        receipt,
    ))
}

pub fn prove_send_utxo_shielded(
    owner: AccountAddress,
    amount: u128,
    owners_parts: Vec<(u128, AccountAddress)>,
) -> Result<(Vec<(UTXO, AccountAddress)>, Receipt), ExecutionFailureKind> {
    let cumulative_spent = owners_parts.iter().fold(0, |acc, item| acc + item.0);

    if cumulative_spent != amount {
        return Err(ExecutionFailureKind::AmountMismatchError);
    }

    let temp_utxo_to_spend = UTXO::create_utxo_from_payload(UTXOPayload {
        owner,
        asset: vec![],
        amount,
        privacy_flag: true,
    })
    .map_err(ExecutionFailureKind::write_error)?;
    let utxo_payload = temp_utxo_to_spend.into_payload();

    let mut builder = ExecutorEnv::builder();

    builder
        .write(&utxo_payload)
        .map_err(ExecutionFailureKind::write_error)?;
    builder
        .write(&owners_parts)
        .map_err(ExecutionFailureKind::write_error)?;

    let env = builder
        .build()
        .map_err(ExecutionFailureKind::builder_error)?;

    let prover = default_prover();

    let receipt = prover
        .prove(env, test_methods::SEND_UTXO_ELF)
        .map_err(ExecutionFailureKind::prove_error)?
        .receipt;

    let digest: Vec<(UTXOPayload, AccountAddress)> = receipt.journal.decode()?;

    Ok((
        digest
            .into_iter()
            .map(|(payload, addr)| (UTXO::create_utxo_from_payload(payload).map(|sel| (sel, addr))))
            .collect::<anyhow::Result<Vec<(UTXO, [u8; 32])>>>()
            .map_err(ExecutionFailureKind::write_error)?,
        receipt,
    ))
}

pub fn prove_send_utxo_deshielded(
    spent_utxo: UTXO,
    owners_parts: Vec<(u128, AccountAddress)>,
) -> Result<(Vec<(u128, AccountAddress)>, Receipt), ExecutionFailureKind> {
    let cumulative_spent = owners_parts.iter().fold(0, |acc, item| acc + item.0);

    if cumulative_spent != spent_utxo.amount {
        return Err(ExecutionFailureKind::AmountMismatchError);
    }

    let mut builder = ExecutorEnv::builder();
    let utxo_payload = spent_utxo.into_payload();

    builder
        .write(&utxo_payload)
        .map_err(ExecutionFailureKind::write_error)?;
    builder
        .write(&owners_parts)
        .map_err(ExecutionFailureKind::write_error)?;

    let env = builder
        .build()
        .map_err(ExecutionFailureKind::builder_error)?;

    let prover = default_prover();

    let receipt = prover
        .prove(env, test_methods::SEND_UTXO_ELF)
        .map_err(ExecutionFailureKind::prove_error)?
        .receipt;

    let digest: Vec<(UTXOPayload, AccountAddress)> = receipt.journal.decode()?;

    Ok((
        digest
            .into_iter()
            .map(|(payload, addr)| (payload.amount, addr))
            .collect(),
        receipt,
    ))
}

pub fn prove_mint_utxo_multiple_assets(
    amount_to_mint: u128,
    number_of_assets: usize,
    owner: AccountAddress,
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

    let digest: Vec<UTXOPayload> = receipt.journal.decode()?;

    Ok((
        digest
            .into_iter()
            .map(UTXO::create_utxo_from_payload)
            .collect::<anyhow::Result<Vec<UTXO>>>()
            .map_err(ExecutionFailureKind::write_error)?,
        receipt,
    ))
}

pub fn execute_mint_utxo(amount_to_mint: u128, owner: AccountAddress) -> anyhow::Result<UTXO> {
    let mut builder = ExecutorEnv::builder();

    builder.write(&amount_to_mint)?;
    builder.write(&owner)?;

    let env = builder.build()?;

    let executor = default_executor();

    let receipt = executor.execute(env, test_methods::MINT_UTXO_ELF)?;

    let digest: UTXOPayload = receipt.journal.decode()?;

    UTXO::create_utxo_from_payload(digest)
}

pub fn execute_send_utxo(
    spent_utxo: UTXO,
    owners_parts: Vec<(u128, AccountAddress)>,
) -> anyhow::Result<(UTXO, Vec<(UTXO, AccountAddress)>)> {
    let mut builder = ExecutorEnv::builder();

    let utxo_payload = spent_utxo.into_payload();

    builder.write(&utxo_payload)?;
    builder.write(&owners_parts)?;

    let env = builder.build()?;

    let executor = default_executor();

    let receipt = executor.execute(env, test_methods::SEND_UTXO_ELF)?;

    let digest: (UTXOPayload, Vec<(UTXOPayload, AccountAddress)>) = receipt.journal.decode()?;

    Ok((
        UTXO::create_utxo_from_payload(digest.0)?,
        digest
            .1
            .into_iter()
            .map(|(payload, addr)| (UTXO::create_utxo_from_payload(payload).map(|sel| (sel, addr))))
            .collect::<anyhow::Result<Vec<(UTXO, [u8; 32])>>>()
            .map_err(ExecutionFailureKind::write_error)?,
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
    use super::*;
    use test_methods::BIG_CALCULATION_ELF;
    use test_methods::{MULTIPLICATION_ELF, MULTIPLICATION_ID};
    use test_methods::{SUMMATION_ELF, SUMMATION_ID};

    #[test]
    fn prove_simple_sum() {
        let message = 1;
        let message_2 = 2;

        let (digest, receipt) = prove(vec![message, message_2], SUMMATION_ELF).unwrap();

        verify(receipt, SUMMATION_ID);
        assert_eq!(digest, message + message_2);
    }

    #[test]
    fn prove_bigger_sum() {
        let message = 123476;
        let message_2 = 2342384;

        let (digest, receipt) = prove(vec![message, message_2], SUMMATION_ELF).unwrap();

        verify(receipt, SUMMATION_ID);
        assert_eq!(digest, message + message_2);
    }

    #[test]
    fn prove_simple_multiplication() {
        let message = 1;
        let message_2 = 2;

        let (digest, receipt) = prove(vec![message, message_2], MULTIPLICATION_ELF).unwrap();

        verify(receipt, MULTIPLICATION_ID);
        assert_eq!(digest, message * message_2);
    }

    #[test]
    fn prove_bigger_multiplication() {
        let message = 3498;
        let message_2 = 438563;

        let (digest, receipt) = prove(vec![message, message_2], MULTIPLICATION_ELF).unwrap();

        verify(receipt, MULTIPLICATION_ID);
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
}
