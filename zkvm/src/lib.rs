use risc0_zkvm::{default_prover, sha::Digest, ExecutorEnv, Receipt};

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
    fn simple_sum() {
        let message = 1;
        let message_2 = 2;

        let (digest, receipt) = prove(vec![message, message_2], SUMMATION_ELF);

        verify(receipt, SUMMATION_ID);
        assert_eq!(digest, message + message_2);
    }

    #[test]
    fn bigger_sum() {
        let message = 123476;
        let message_2 = 2342384;

        let (digest, receipt) = prove(vec![message, message_2], SUMMATION_ELF);

        verify(receipt, SUMMATION_ID);
        assert_eq!(digest, message + message_2);
    }

    #[test]
    fn simple_multiplication() {
        let message = 1;
        let message_2 = 2;

        let (digest, receipt) = prove(vec![message, message_2], MULTIPLICATION_ELF);

        verify(receipt, MULTIPLICATION_ID);
        assert_eq!(digest, message * message_2);
    }

    #[test]
    fn bigger_multiplication() {
        let message = 3498;
        let message_2 = 438563;

        let (digest, receipt) = prove(vec![message, message_2], MULTIPLICATION_ELF);

        verify(receipt, MULTIPLICATION_ID);
        assert_eq!(digest, message * message_2);
    }
}
