use crate::{PublicKey, Signature, tests::bip340_test_vectors};

#[test]
fn test_signature_generation_from_bip340_test_vectors() {
    for (i, test_vector) in bip340_test_vectors::test_vectors().into_iter().enumerate() {
        let Some(private_key) = test_vector.seckey else {
            continue;
        };
        let Some(aux_random) = test_vector.aux_rand else {
            continue;
        };
        let Some(message) = test_vector.message else {
            continue;
        };
        if !test_vector.verification_result {
            continue;
        }
        let expected_signature = &test_vector.signature;

        let signature = Signature::new_with_aux_random(&private_key, &message, aux_random);

        assert_eq!(&signature, expected_signature, "Failed test vector {i}");
    }
}

#[test]
fn test_signature_verification_from_bip340_test_vectors() {
    for (i, test_vector) in bip340_test_vectors::test_vectors().into_iter().enumerate() {
        let message = test_vector.message.unwrap_or(vec![]);
        let expected_result = test_vector.verification_result;

        let result = test_vector
            .signature
            .is_valid_for(&message, &test_vector.pubkey);

        assert_eq!(result, expected_result, "Failed test vector {i}");
    }
}

#[test]
fn test_public_key_generation_from_bip340_test_vectors() {
    for (i, test_vector) in bip340_test_vectors::test_vectors().into_iter().enumerate() {
        let Some(private_key) = &test_vector.seckey else {
            continue;
        };
        let public_key = PublicKey::new(private_key);
        let expected_public_key = &test_vector.pubkey;
        assert_eq!(
            &public_key, expected_public_key,
            "Failed test vector at index {i}"
        );
    }
}
