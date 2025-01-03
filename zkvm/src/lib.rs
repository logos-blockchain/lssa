use accounts::account_core::AccountAddress;
use risc0_zkvm::{default_executor, default_prover, sha::Digest, ExecutorEnv, Receipt};
use utxo::utxo_core::{UTXOPayload, UTXO};

pub fn prove_mint_utxo(amount_to_mint: u128, owner: AccountAddress) -> (UTXO, Receipt) {
    let mut builder = ExecutorEnv::builder();

    builder.write(&amount_to_mint).unwrap();
    builder.write(&owner).unwrap();

    let env = builder.build().unwrap();

    let prover = default_prover();

    let receipt = prover
        .prove(env, test_methods::MINT_UTXO_ELF)
        .unwrap()
        .receipt;

    let digest: UTXOPayload = receipt.journal.decode().unwrap();

    (UTXO::create_utxo_from_payload(digest), receipt)
}

pub fn prove_send_utxo(
    spent_utxo: UTXO,
    owners_parts: Vec<(u128, AccountAddress)>,
) -> (Vec<(UTXO, AccountAddress)>, Receipt) {
    let mut builder = ExecutorEnv::builder();
    let utxo_payload = spent_utxo.into_payload();

    builder.write(&utxo_payload).unwrap();
    builder.write(&owners_parts).unwrap();

    let env = builder.build().unwrap();

    let prover = default_prover();

    let receipt = prover
        .prove(env, test_methods::SEND_UTXO_ELF)
        .unwrap()
        .receipt;

    let digest: Vec<(UTXOPayload, AccountAddress)> = receipt.journal.decode().unwrap();

    (
        digest
            .into_iter()
            .map(|(payload, addr)| (UTXO::create_utxo_from_payload(payload), addr))
            .collect(),
        receipt,
    )
}

pub fn prove_send_utxo_multiple_assets_one_receiver(
    spent_utxos: Vec<UTXO>,
    number_to_send: usize,
    receiver: AccountAddress,
) -> (Vec<UTXO>, Vec<UTXO>, Receipt) {
    let mut builder = ExecutorEnv::builder();
    let utxo_payload: Vec<UTXOPayload> = spent_utxos
        .into_iter()
        .map(|spent_utxo| spent_utxo.into_payload())
        .collect();

    builder.write(&utxo_payload).unwrap();
    builder.write(&number_to_send).unwrap();
    builder.write(&receiver).unwrap();

    let env = builder.build().unwrap();

    let prover = default_prover();

    let receipt = prover
        .prove(env, test_methods::SEND_UTXO_MULTIPLE_ASSETS_ELF)
        .unwrap()
        .receipt;

    let digest: (Vec<UTXOPayload>, Vec<UTXOPayload>) = receipt.journal.decode().unwrap();

    (
        digest
            .0
            .into_iter()
            .map(|payload| UTXO::create_utxo_from_payload(payload))
            .collect(),
        digest
            .1
            .into_iter()
            .map(|payload| UTXO::create_utxo_from_payload(payload))
            .collect(),
        receipt,
    )
}

pub fn prove_send_utxo_shielded(
    owner: AccountAddress,
    amount: u128,
    owners_parts: Vec<(u128, AccountAddress)>,
) -> (Vec<(UTXO, AccountAddress)>, Receipt) {
    let temp_utxo_to_spend = UTXO::create_utxo_from_payload(UTXOPayload {
        owner,
        asset: vec![],
        amount,
        privacy_flag: true,
    });
    let utxo_payload = temp_utxo_to_spend.into_payload();

    let mut builder = ExecutorEnv::builder();

    builder.write(&utxo_payload).unwrap();
    builder.write(&owners_parts).unwrap();

    let env = builder.build().unwrap();

    let prover = default_prover();

    let receipt = prover
        .prove(env, test_methods::SEND_UTXO_ELF)
        .unwrap()
        .receipt;

    let digest: Vec<(UTXOPayload, AccountAddress)> = receipt.journal.decode().unwrap();

    (
        digest
            .into_iter()
            .map(|(payload, addr)| (UTXO::create_utxo_from_payload(payload), addr))
            .collect(),
        receipt,
    )
}

pub fn prove_send_utxo_deshielded(
    spent_utxo: UTXO,
    owners_parts: Vec<(u128, AccountAddress)>,
) -> (Vec<(u128, AccountAddress)>, Receipt) {
    let mut builder = ExecutorEnv::builder();
    let utxo_payload = spent_utxo.into_payload();

    builder.write(&utxo_payload).unwrap();
    builder.write(&owners_parts).unwrap();

    let env = builder.build().unwrap();

    let prover = default_prover();

    let receipt = prover
        .prove(env, test_methods::SEND_UTXO_ELF)
        .unwrap()
        .receipt;

    let digest: Vec<(UTXOPayload, AccountAddress)> = receipt.journal.decode().unwrap();

    (
        digest
            .into_iter()
            .map(|(payload, addr)| (payload.amount, addr))
            .collect(),
        receipt,
    )
}

pub fn prove_mint_utxo_multiple_assets(
    amount_to_mint: u128,
    number_of_assets: usize,
    owner: AccountAddress,
) -> (Vec<UTXO>, Receipt) {
    let mut builder = ExecutorEnv::builder();

    builder.write(&amount_to_mint).unwrap();
    builder.write(&number_of_assets).unwrap();
    builder.write(&owner).unwrap();

    let env = builder.build().unwrap();

    let prover = default_prover();

    let receipt = prover
        .prove(env, test_methods::MINT_UTXO_MULTIPLE_ASSETS_ELF)
        .unwrap()
        .receipt;

    let digest: Vec<UTXOPayload> = receipt.journal.decode().unwrap();

    (
        digest
            .into_iter()
            .map(UTXO::create_utxo_from_payload)
            .collect(),
        receipt,
    )
}

pub fn execute_mint_utxo(amount_to_mint: u128, owner: AccountAddress) -> UTXO {
    let mut builder = ExecutorEnv::builder();

    builder.write(&amount_to_mint).unwrap();
    builder.write(&owner).unwrap();

    let env = builder.build().unwrap();

    let executor = default_executor();

    let receipt = executor.execute(env, test_methods::MINT_UTXO_ELF).unwrap();

    let digest: UTXOPayload = receipt.journal.decode().unwrap();

    UTXO::create_utxo_from_payload(digest)
}

pub fn execute_send_utxo(
    spent_utxo: UTXO,
    owners_parts: Vec<(u128, AccountAddress)>,
) -> (UTXO, Vec<(UTXO, AccountAddress)>) {
    let mut builder = ExecutorEnv::builder();

    let utxo_payload = spent_utxo.into_payload();

    builder.write(&utxo_payload).unwrap();
    builder.write(&owners_parts).unwrap();

    let env = builder.build().unwrap();

    let executor = default_executor();

    let receipt = executor.execute(env, test_methods::SEND_UTXO_ELF).unwrap();

    let digest: (UTXOPayload, Vec<(UTXOPayload, AccountAddress)>) =
        receipt.journal.decode().unwrap();

    (
        UTXO::create_utxo_from_payload(digest.0),
        digest
            .1
            .into_iter()
            .map(|(payload, addr)| (UTXO::create_utxo_from_payload(payload), addr))
            .collect(),
    )
}

pub fn prove<T: serde::ser::Serialize>(input_vec: Vec<T>, elf: &[u8]) -> (u64, Receipt) {
    let mut builder = ExecutorEnv::builder();

    for input in input_vec {
        builder.write(&input).unwrap();
    }

    let env = builder.build().unwrap();

    let prover = default_prover();

    let receipt = prover.prove(env, elf).unwrap().receipt;

    let digest = receipt.journal.decode().unwrap();
    (digest, receipt)
}

// This only executes the program and does not generate a receipt.
pub fn execute<T: serde::ser::Serialize + for<'de> serde::Deserialize<'de>>(
    input_vec: Vec<T>,
    elf: &[u8],
) -> T {
    let mut builder = ExecutorEnv::builder();

    for input in input_vec {
        builder.write(&input).unwrap();
    }

    let env = builder.build().unwrap();

    let exec = default_executor();
    let session = exec.execute(env, elf).unwrap();

    // We read the result committed to the journal by the guest code.
    let result: T = session.journal.decode().unwrap();

    result
}

pub fn verify(receipt: Receipt, image_id: impl Into<Digest>) {
    receipt
        .verify(image_id)
        .expect("receipt verification failed");
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

        let (digest, receipt) = prove(vec![message, message_2], SUMMATION_ELF);

        verify(receipt, SUMMATION_ID);
        assert_eq!(digest, message + message_2);
    }

    #[test]
    fn prove_bigger_sum() {
        let message = 123476;
        let message_2 = 2342384;

        let (digest, receipt) = prove(vec![message, message_2], SUMMATION_ELF);

        verify(receipt, SUMMATION_ID);
        assert_eq!(digest, message + message_2);
    }

    #[test]
    fn prove_simple_multiplication() {
        let message = 1;
        let message_2 = 2;

        let (digest, receipt) = prove(vec![message, message_2], MULTIPLICATION_ELF);

        verify(receipt, MULTIPLICATION_ID);
        assert_eq!(digest, message * message_2);
    }

    #[test]
    fn prove_bigger_multiplication() {
        let message = 3498;
        let message_2 = 438563;

        let (digest, receipt) = prove(vec![message, message_2], MULTIPLICATION_ELF);

        verify(receipt, MULTIPLICATION_ID);
        assert_eq!(digest, message * message_2);
    }

    #[test]
    fn execute_simple_sum() {
        let message: u64 = 1;
        let message_2: u64 = 2;

        let result = execute(vec![message, message_2], SUMMATION_ELF);
        assert_eq!(result, message + message_2);
    }

    #[test]
    fn execute_bigger_sum() {
        let message: u64 = 123476;
        let message_2: u64 = 2342384;

        let result = execute(vec![message, message_2], SUMMATION_ELF);
        assert_eq!(result, message + message_2);
    }

    #[test]
    fn execute_big_calculation() {
        let message: u128 = 1;
        let message_2: u128 = 2;

        let result = execute(vec![message, message_2], BIG_CALCULATION_ELF);
        assert_eq!(result, big_calculation(message, message_2));
    }

    #[test]
    fn execute_big_calculation_long() {
        let message: u128 = 20;
        let message_2: u128 = 10;

        let result = execute(vec![message, message_2], BIG_CALCULATION_ELF);
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
