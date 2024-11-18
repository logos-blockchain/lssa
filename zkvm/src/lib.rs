use risc0_zkvm::{default_executor, default_prover, sha::Digest, ExecutorEnv, Receipt};

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
}
