use ark_bn254::Fr;
use ark_ff::{BigInteger, PrimeField};
use jf_poseidon2::{Poseidon2, constants::bn254::Poseidon2ParamsBn3};
use light_poseidon::{Poseidon, PoseidonHasher};
use nssa_core::{
    account::AccountWithMetadata,
    program::{
        AccountPostState, ChainedCall, ProgramInput, read_nssa_inputs, write_nssa_outputs, write_nssa_outputs_with_chained_call
    },
};
use rln::hashers::poseidon_hash;
use rln::utils::{bytes_le_to_fr, fr_to_bytes_le};
use rust_poseidon_bn254_pure::bn254::bigint::BigInt;
use rust_poseidon_bn254_pure::bn254::field::Felt;
use rust_poseidon_bn254_pure::poseidon2::permutation::permute_felt;

/// Select which hash implementation to use
#[derive(Clone, Copy)]
enum HashImpl {
    /// Zerokit's poseidon hash (BN254 curve)
    Zerokit = 0,
    /// Light-poseidon hash (BN254 curve)
    LightPoseidon = 1,
    /// Jellyfish poseidon2 hash (BN254 curve)
    JfPoseidon2 = 2,
    /// Logos pure Rust poseidon2 hash (BN254 curve)
    LogosPoseidon2 = 3,
}

impl HashImpl {
    fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(HashImpl::Zerokit),
            1 => Some(HashImpl::LightPoseidon),
            2 => Some(HashImpl::JfPoseidon2),
            3 => Some(HashImpl::LogosPoseidon2),
            _ => None,
        }
    }
}

type Instruction = Vec<u8>;

enum InstructionType {
    ValidateAndStoreIdentityCommitment = 0,
}

impl InstructionType {
    fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(InstructionType::ValidateAndStoreIdentityCommitment),
            _ => None,
        }
    }
}

struct Membership {
    identity_commitment: [u8; 32],
    user_message_limit: u32,
}

impl Membership {
    fn to_bytes(&self) -> [u8; 36] {
        let mut bytes = [0u8; 36];
        bytes[..32].copy_from_slice(&self.identity_commitment);
        bytes[32..36].copy_from_slice(&self.user_message_limit.to_le_bytes());
        bytes
    }

    /// Hash the membership data
    /// Returns a 32-byte hash after `iterations` rounds of chained hashing
    fn hash(&self, hash_impl: HashImpl, iterations: u16) -> [u8; 32] {
        // First iteration uses the original membership data
        let mut current_hash = self.hash_once(hash_impl);

        // Chain the hash: use previous output as input for next iteration
        for i in 0..iterations {
            let chained_membership = Membership {
                identity_commitment: current_hash,
                user_message_limit: self.user_message_limit,
            };
            current_hash = chained_membership.hash_once(hash_impl);
            println!("iteration {}: current_hash: {}", i + 1, current_hash.iter().map(|b| format!("{:02x}", b)).collect::<String>());
        }

        current_hash
    }

    /// Single hash iteration
    fn hash_once(&self, hash_impl: HashImpl) -> [u8; 32] {
        match hash_impl {
            HashImpl::Zerokit => self.hash_zerokit_poseidon(),
            HashImpl::LightPoseidon => self.hash_light_poseidon(),
            HashImpl::JfPoseidon2 => self.hash_jf_poseidon2(),
            HashImpl::LogosPoseidon2 => self.hash_logos_poseidon2(),
        }
    }

    /// Hash using zerokit's poseidon hash (BN254 curve)
    fn hash_zerokit_poseidon(&self) -> [u8; 32] {
        // Convert identity_commitment bytes to Fr
        let (id_commitment_fr, _) = bytes_le_to_fr(&self.identity_commitment).unwrap();

        // Convert user_message_limit to Fr (as 32-byte little-endian)
        let mut user_limit_bytes = [0u8; 32];
        user_limit_bytes[..4].copy_from_slice(&self.user_message_limit.to_le_bytes());
        let (user_limit_fr, _) = bytes_le_to_fr(&user_limit_bytes).unwrap();

        println!("before zerokit poseidon hash");

        // Compute poseidon hash
        let hash_fr = poseidon_hash(&[id_commitment_fr, user_limit_fr]).unwrap();

        // Convert back to bytes
        fr_to_bytes_le(&hash_fr).try_into().expect("hash should be 32 bytes")
    }

    /// Hash using light-poseidon (BN254 curve)
    fn hash_light_poseidon(&self) -> [u8; 32] {
        let mut poseidon = Poseidon::<Fr>::new_circom(2).unwrap();

        // Convert identity_commitment to Fr (little-endian, mod order)
        let id_commitment_fr = Fr::from_le_bytes_mod_order(&self.identity_commitment);

        // Convert user_message_limit to Fr
        let mut user_limit_bytes = [0u8; 32];
        user_limit_bytes[..4].copy_from_slice(&self.user_message_limit.to_le_bytes());
        let user_limit_fr = Fr::from_le_bytes_mod_order(&user_limit_bytes);

        // Hash field elements
        let hash_fr = poseidon.hash(&[id_commitment_fr, user_limit_fr]).unwrap();

        // Convert result to bytes (little-endian)
        let mut hash_bytes = [0u8; 32];
        hash_bytes.copy_from_slice(&hash_fr.into_bigint().to_bytes_le());
        hash_bytes
    }

    /// Hash using Jellyfish poseidon2 (BN254 curve)
    fn hash_jf_poseidon2(&self) -> [u8; 32] {
        // Convert identity_commitment to Fr (little-endian, mod order)
        let id_commitment_fr = Fr::from_le_bytes_mod_order(&self.identity_commitment);

        // Convert user_message_limit to Fr
        let mut user_limit_bytes = [0u8; 32];
        user_limit_bytes[..4].copy_from_slice(&self.user_message_limit.to_le_bytes());
        let user_limit_fr = Fr::from_le_bytes_mod_order(&user_limit_bytes);

        // Hash field elements using Jellyfish Poseidon2
        // State size 3: 2 inputs + 1 capacity element
        let mut state = [id_commitment_fr, user_limit_fr, Fr::from(0u64)];
        Poseidon2::permute_mut::<Poseidon2ParamsBn3, 3>(&mut state);
        let hash_fr = state[0];

        // Convert result to bytes (little-endian)
        let mut hash_bytes = [0u8; 32];
        hash_bytes.copy_from_slice(&hash_fr.into_bigint().to_bytes_le());
        hash_bytes
    }

    /// Hash using Logos pure Rust poseidon2 (BN254 curve)
    fn hash_logos_poseidon2(&self) -> [u8; 32] {
        // Convert 32 bytes to 8 u32s (little-endian)
        fn bytes_to_u32_array(bytes: &[u8; 32]) -> [u32; 8] {
            let mut result = [0u32; 8];
            for i in 0..8 {
                result[i] = u32::from_le_bytes(
                    bytes[i * 4..(i + 1) * 4].try_into().unwrap()
                );
            }
            result
        }

        // Convert 8 u32s back to 32 bytes (little-endian)
        fn u32_array_to_bytes(arr: &[u32; 8]) -> [u8; 32] {
            let mut result = [0u8; 32];
            for i in 0..8 {
                result[i * 4..(i + 1) * 4].copy_from_slice(&arr[i].to_le_bytes());
            }
            result
        }

        // Convert identity_commitment to Felt
        let id_commitment_felt = Felt::checked_make(bytes_to_u32_array(&self.identity_commitment));

        // Convert user_message_limit to Felt (as 32-byte little-endian)
        let mut user_limit_bytes = [0u8; 32];
        user_limit_bytes[..4].copy_from_slice(&self.user_message_limit.to_le_bytes());
        let user_limit_felt = Felt::checked_make(bytes_to_u32_array(&user_limit_bytes));

        // Create the input triple: 2 inputs + 1 capacity element (zero)
        let input: (Felt, Felt, Felt) = (id_commitment_felt, user_limit_felt, Felt::zero());

        // Perform the permutation
        let output = permute_felt(&input);

        // Extract the first element as the hash result
        let hash_felt = output.0;
        let hash_big = Felt::unwrap(hash_felt);

        // Convert back to bytes
        let hash_limbs: [u32; 8] = BigInt::unwrap(hash_big);
        u32_array_to_bytes(&hash_limbs)
    }

}

const USER_MESSAGE_LIMIT_MIN: u32 = 300;
const USER_MESSAGE_LIMIT_MAX: u32 = 600;

fn validate_and_store_identity_commitment(
    pre_state: AccountWithMetadata,
    instruction: &[u8],
) -> (Vec<AccountPostState>, Vec<ChainedCall>) {
    // 1. Validate that pre_state matches what we expect
    if !pre_state.is_authorized {
        panic!("Missing required authorization");
    }

    // Instruction layout (after the 1-byte instruction type):
    // - Byte 0: hash_impl (0=Zerokit, 1=LightPoseidon, 2=JfPoseidon2, 3=LogosPoseidon2)
    // - Bytes 1-2: hash_iterations (u16 little-endian)
    // - Bytes 3-34: identity commitment (32 bytes)
    // - Bytes 35-66: user message limit (32 bytes, only first 4 used as u32)

    // Validate instruction size
    assert!(instruction.len() >= 67, "Instruction must contain at least 67 bytes");

    // Parse hash implementation
    let hash_impl = HashImpl::from_u8(instruction[0])
        .expect("Invalid hash implementation value");

    // Parse hash iterations
    let hash_iterations = u16::from_le_bytes(
        instruction[1..3].try_into().expect("slice should be 2 bytes")
    );
    assert!(hash_iterations >= 1, "Hash iterations must be at least 1");

    // Extract identity commitment (32 bytes)
    let identity_commitment = &instruction[3..35];

    // Extract user message limit as u32 (first 4 bytes of the 32-byte field, little-endian)
    let user_message_limit = u32::from_le_bytes(
        instruction[35..39].try_into().expect("slice should be 4 bytes")
    );

    assert!(
        user_message_limit >= USER_MESSAGE_LIMIT_MIN && user_message_limit <= USER_MESSAGE_LIMIT_MAX,
        "User message limit must be between {} and {}, got {}",
        USER_MESSAGE_LIMIT_MIN, USER_MESSAGE_LIMIT_MAX,
        user_message_limit
    );

    // Create membership struct
    let membership = Membership {
        identity_commitment: identity_commitment.try_into().expect("slice should be 32 bytes"),
        user_message_limit,
    };

    // Compute hash with specified implementation and iterations
    let membership_hash = membership.hash(hash_impl, hash_iterations);

    // Store membership data and hash in account data
    let post_account = {
        let mut account = pre_state.account.clone();
        let mut data = account.data.into_inner();
        data.extend_from_slice(&membership.to_bytes()); // 36 bytes
        data.extend_from_slice(&membership_hash); // 32 bytes
        account.data = data
            .try_into()
            .expect("Data should fit within the allowed limits");
        account
    };

    (vec![AccountPostState::new_claimed_if_default(post_account)], vec![])
}

// Validates and stores an identity commitment
fn main() {
    let (
        ProgramInput {
            pre_states,
            instruction,
        },
        instruction_words,
    ) = read_nssa_inputs::<Instruction>();

    let (post_states, chained_calls) = match InstructionType::from_u8(instruction[0]) {
        Some(InstructionType::ValidateAndStoreIdentityCommitment) => {
            validate_and_store_identity_commitment(pre_states[0].clone(), &instruction[1..])
        }
        _ => panic!("Invalid instruction"),
    };

    if chained_calls.is_empty() {
        write_nssa_outputs(instruction_words, pre_states, post_states);
    } else {
        write_nssa_outputs_with_chained_call(instruction_words, pre_states, post_states, chained_calls);
    }
}
