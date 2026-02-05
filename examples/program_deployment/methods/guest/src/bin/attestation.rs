use nssa_core::{
    account::{Account, AccountId, AccountWithMetadata, Data},
    program::{
        AccountPostState, ProgramInput, read_nssa_inputs, write_nssa_outputs,
    },
};

// Attestation registry program.
//
// Any account holder can create attestations about any subject, identified by
// a 32-byte key, with an arbitrary-length value. Only the original creator can
// update or revoke their attestation.
//
// Data layout (stored in the attestation account's `data` field):
//   Bytes  0..32  : creator  (AccountId, 32 bytes)
//   Bytes  32..64 : subject  (AccountId, 32 bytes)
//   Bytes  64..96 : key      ([u8; 32], 32 bytes)
//   Byte   96     : revoked  (bool, 0 = active, 1 = revoked)
//   Bytes  97..   : value    (variable length)
//
// Instructions:
//   0x00  Attest  — payload: subject (32) || key (32) || value (var)
//                   accounts: [creator (authorized), attestation_account]
//   0x01  Revoke  — payload: (none)
//                   accounts: [creator (authorized), attestation_account]

const ATTESTATION_HEADER_SIZE: usize = 97; // 32 + 32 + 32 + 1

struct Attestation {
    creator: AccountId,
    subject: AccountId,
    key: [u8; 32],
    revoked: bool,
    value: Vec<u8>,
}

impl Attestation {
    fn into_data(self) -> Data {
        let mut bytes = Vec::<u8>::new();
        bytes.extend_from_slice(&self.creator.to_bytes());
        bytes.extend_from_slice(&self.subject.to_bytes());
        bytes.extend_from_slice(&self.key);
        bytes.push(self.revoked as u8);
        bytes.extend_from_slice(&self.value);

        Data::try_from(bytes).expect("Attestation data must fit within the allowed limits")
    }

    fn parse(data: &Data) -> Option<Self> {
        let data = Vec::<u8>::from(data.clone());

        if data.len() < ATTESTATION_HEADER_SIZE {
            return None;
        }

        let creator = AccountId::new(
            data[0..32]
                .try_into()
                .expect("Creator must be 32 bytes"),
        );
        let subject = AccountId::new(
            data[32..64]
                .try_into()
                .expect("Subject must be 32 bytes"),
        );
        let key: [u8; 32] = data[64..96]
            .try_into()
            .expect("Key must be 32 bytes");
        let revoked = data[96] != 0;
        let value = data[97..].to_vec();

        Some(Self {
            creator,
            subject,
            key,
            revoked,
            value,
        })
    }
}

fn attest(pre_states: &[AccountWithMetadata], payload: &[u8]) -> Vec<AccountPostState> {
    if pre_states.len() != 2 {
        panic!("Attest requires exactly 2 accounts");
    }

    let creator_account = &pre_states[0];
    let attestation_account = &pre_states[1];

    if !creator_account.is_authorized {
        panic!("Missing required authorization for creator");
    }

    if payload.len() < 64 {
        panic!("Attest payload must contain at least subject (32) and key (32)");
    }

    let subject = AccountId::new(
        payload[0..32]
            .try_into()
            .expect("Subject must be 32 bytes"),
    );
    let key: [u8; 32] = payload[32..64]
        .try_into()
        .expect("Key must be 32 bytes");
    let value = payload[64..].to_vec();

    let attestation_post = if attestation_account.account == Account::default() {
        // Creating a new attestation
        let attestation = Attestation {
            creator: creator_account.account_id,
            subject,
            key,
            revoked: false,
            value,
        };

        let mut account = attestation_account.account.clone();
        account.data = attestation.into_data();
        AccountPostState::new_claimed(account)
    } else {
        // Updating an existing attestation
        let mut existing = Attestation::parse(&attestation_account.account.data)
            .expect("Invalid existing attestation data");

        assert_eq!(
            existing.creator, creator_account.account_id,
            "Only the original creator can update an attestation"
        );
        assert!(
            !existing.revoked,
            "Cannot update a revoked attestation"
        );

        existing.value = value;

        let mut account = attestation_account.account.clone();
        account.data = existing.into_data();
        AccountPostState::new(account)
    };

    let creator_post = AccountPostState::new(creator_account.account.clone());

    vec![creator_post, attestation_post]
}

fn revoke(pre_states: &[AccountWithMetadata]) -> Vec<AccountPostState> {
    if pre_states.len() != 2 {
        panic!("Revoke requires exactly 2 accounts");
    }

    let creator_account = &pre_states[0];
    let attestation_account = &pre_states[1];

    if !creator_account.is_authorized {
        panic!("Missing required authorization for creator");
    }

    if attestation_account.account == Account::default() {
        panic!("Cannot revoke a non-existent attestation");
    }

    let mut existing = Attestation::parse(&attestation_account.account.data)
        .expect("Invalid existing attestation data");

    assert_eq!(
        existing.creator, creator_account.account_id,
        "Only the original creator can revoke an attestation"
    );
    assert!(
        !existing.revoked,
        "Attestation is already revoked"
    );

    existing.revoked = true;

    let mut account = attestation_account.account.clone();
    account.data = existing.into_data();

    let creator_post = AccountPostState::new(creator_account.account.clone());
    let attestation_post = AccountPostState::new(account);

    vec![creator_post, attestation_post]
}

type Instruction = Vec<u8>;

fn main() {
    let (
        ProgramInput {
            pre_states,
            instruction,
        },
        instruction_data,
    ) = read_nssa_inputs::<Instruction>();

    let post_states = match instruction[0] {
        0 => attest(&pre_states, &instruction[1..]),
        1 => revoke(&pre_states),
        _ => panic!("Invalid instruction opcode"),
    };

    write_nssa_outputs(instruction_data, pre_states, post_states);
}

#[cfg(test)]
mod tests {
    use nssa_core::account::{Account, AccountId, AccountWithMetadata, Data};

    use crate::{ATTESTATION_HEADER_SIZE, Attestation, attest, revoke};

    fn creator_id() -> AccountId {
        AccountId::new([1; 32])
    }

    fn subject_id() -> AccountId {
        AccountId::new([2; 32])
    }

    fn attestation_key() -> [u8; 32] {
        [3; 32]
    }

    fn creator_account(is_authorized: bool) -> AccountWithMetadata {
        AccountWithMetadata {
            account: Account {
                program_owner: [5u32; 8],
                balance: 0,
                data: Data::default(),
                nonce: 0,
            },
            is_authorized,
            account_id: creator_id(),
        }
    }

    fn uninit_attestation_account() -> AccountWithMetadata {
        AccountWithMetadata {
            account: Account::default(),
            is_authorized: false,
            account_id: AccountId::new([10; 32]),
        }
    }

    fn existing_attestation_account(revoked: bool, value: &[u8]) -> AccountWithMetadata {
        let attestation = Attestation {
            creator: creator_id(),
            subject: subject_id(),
            key: attestation_key(),
            revoked,
            value: value.to_vec(),
        };
        AccountWithMetadata {
            account: Account {
                program_owner: [5u32; 8],
                balance: 0,
                data: attestation.into_data(),
                nonce: 0,
            },
            is_authorized: false,
            account_id: AccountId::new([10; 32]),
        }
    }

    fn build_attest_payload(subject: &AccountId, key: &[u8; 32], value: &[u8]) -> Vec<u8> {
        let mut payload = Vec::new();
        payload.extend_from_slice(subject.value());
        payload.extend_from_slice(key);
        payload.extend_from_slice(value);
        payload
    }

    #[test]
    fn test_attestation_serialize_deserialize() {
        let original = Attestation {
            creator: creator_id(),
            subject: subject_id(),
            key: attestation_key(),
            revoked: false,
            value: b"hello world".to_vec(),
        };

        let data = original.into_data();
        let parsed = Attestation::parse(&data).expect("Should parse successfully");

        assert_eq!(parsed.creator, creator_id());
        assert_eq!(parsed.subject, subject_id());
        assert_eq!(parsed.key, attestation_key());
        assert!(!parsed.revoked);
        assert_eq!(parsed.value, b"hello world");
    }

    #[test]
    fn test_attestation_parse_invalid_length() {
        let short_data = Data::try_from(vec![0u8; ATTESTATION_HEADER_SIZE - 1]).unwrap();
        assert!(Attestation::parse(&short_data).is_none());
    }

    #[test]
    fn test_attestation_parse_empty() {
        let empty_data = Data::default();
        assert!(Attestation::parse(&empty_data).is_none());
    }

    #[test]
    fn test_attest_create_new() {
        let pre_states = vec![creator_account(true), uninit_attestation_account()];
        let payload = build_attest_payload(&subject_id(), &attestation_key(), b"test value");

        let post_states = attest(&pre_states, &payload);
        assert_eq!(post_states.len(), 2);

        // Creator account should be unchanged
        assert_eq!(*post_states[0].account(), pre_states[0].account);
        assert!(!post_states[0].requires_claim());

        // Attestation account should be claimed with new data
        assert!(post_states[1].requires_claim());
        let parsed = Attestation::parse(&post_states[1].account().data)
            .expect("Should parse attestation");
        assert_eq!(parsed.creator, creator_id());
        assert_eq!(parsed.subject, subject_id());
        assert_eq!(parsed.key, attestation_key());
        assert!(!parsed.revoked);
        assert_eq!(parsed.value, b"test value");
    }

    #[test]
    fn test_attest_update_existing() {
        let pre_states = vec![
            creator_account(true),
            existing_attestation_account(false, b"old value"),
        ];
        let payload = build_attest_payload(&subject_id(), &attestation_key(), b"new value");

        let post_states = attest(&pre_states, &payload);
        assert_eq!(post_states.len(), 2);

        assert!(!post_states[1].requires_claim());
        let parsed = Attestation::parse(&post_states[1].account().data)
            .expect("Should parse attestation");
        assert_eq!(parsed.value, b"new value");
        assert!(!parsed.revoked);
    }

    #[test]
    #[should_panic(expected = "Missing required authorization for creator")]
    fn test_attest_missing_authorization() {
        let pre_states = vec![creator_account(false), uninit_attestation_account()];
        let payload = build_attest_payload(&subject_id(), &attestation_key(), b"test");

        attest(&pre_states, &payload);
    }

    #[test]
    #[should_panic(expected = "Only the original creator can update an attestation")]
    fn test_attest_update_wrong_creator() {
        let different_creator = AccountWithMetadata {
            account: Account {
                program_owner: [5u32; 8],
                balance: 0,
                data: Data::default(),
                nonce: 0,
            },
            is_authorized: true,
            account_id: AccountId::new([99; 32]),
        };
        let pre_states = vec![
            different_creator,
            existing_attestation_account(false, b"old"),
        ];
        let payload = build_attest_payload(&subject_id(), &attestation_key(), b"new");

        attest(&pre_states, &payload);
    }

    #[test]
    #[should_panic(expected = "Cannot update a revoked attestation")]
    fn test_attest_update_revoked() {
        let pre_states = vec![
            creator_account(true),
            existing_attestation_account(true, b"old"),
        ];
        let payload = build_attest_payload(&subject_id(), &attestation_key(), b"new");

        attest(&pre_states, &payload);
    }

    #[test]
    #[should_panic(expected = "Attest requires exactly 2 accounts")]
    fn test_attest_wrong_account_count() {
        let pre_states = vec![creator_account(true)];
        let payload = build_attest_payload(&subject_id(), &attestation_key(), b"test");

        attest(&pre_states, &payload);
    }

    #[test]
    #[should_panic(expected = "Attest payload must contain at least subject (32) and key (32)")]
    fn test_attest_payload_too_short() {
        let pre_states = vec![creator_account(true), uninit_attestation_account()];

        attest(&pre_states, &[0u8; 63]);
    }

    #[test]
    fn test_revoke_success() {
        let pre_states = vec![
            creator_account(true),
            existing_attestation_account(false, b"some value"),
        ];

        let post_states = revoke(&pre_states);
        assert_eq!(post_states.len(), 2);

        assert_eq!(*post_states[0].account(), pre_states[0].account);
        assert!(!post_states[0].requires_claim());

        assert!(!post_states[1].requires_claim());
        let parsed = Attestation::parse(&post_states[1].account().data)
            .expect("Should parse attestation");
        assert!(parsed.revoked);
        assert_eq!(parsed.value, b"some value");
    }

    #[test]
    #[should_panic(expected = "Missing required authorization for creator")]
    fn test_revoke_missing_authorization() {
        let pre_states = vec![
            creator_account(false),
            existing_attestation_account(false, b"val"),
        ];

        revoke(&pre_states);
    }

    #[test]
    #[should_panic(expected = "Cannot revoke a non-existent attestation")]
    fn test_revoke_nonexistent() {
        let pre_states = vec![creator_account(true), uninit_attestation_account()];

        revoke(&pre_states);
    }

    #[test]
    #[should_panic(expected = "Only the original creator can revoke an attestation")]
    fn test_revoke_wrong_creator() {
        let different_creator = AccountWithMetadata {
            account: Account {
                program_owner: [5u32; 8],
                balance: 0,
                data: Data::default(),
                nonce: 0,
            },
            is_authorized: true,
            account_id: AccountId::new([99; 32]),
        };
        let pre_states = vec![
            different_creator,
            existing_attestation_account(false, b"val"),
        ];

        revoke(&pre_states);
    }

    #[test]
    #[should_panic(expected = "Attestation is already revoked")]
    fn test_revoke_already_revoked() {
        let pre_states = vec![
            creator_account(true),
            existing_attestation_account(true, b"val"),
        ];

        revoke(&pre_states);
    }

    #[test]
    #[should_panic(expected = "Revoke requires exactly 2 accounts")]
    fn test_revoke_wrong_account_count() {
        let pre_states = vec![creator_account(true)];

        revoke(&pre_states);
    }

    #[test]
    fn test_attest_empty_value() {
        let pre_states = vec![creator_account(true), uninit_attestation_account()];
        let payload = build_attest_payload(&subject_id(), &attestation_key(), b"");

        let post_states = attest(&pre_states, &payload);

        let parsed = Attestation::parse(&post_states[1].account().data)
            .expect("Should parse attestation");
        assert_eq!(parsed.value, b"");
    }

    #[test]
    fn test_attestation_header_size_matches_parse() {
        let attestation = Attestation {
            creator: creator_id(),
            subject: subject_id(),
            key: attestation_key(),
            revoked: false,
            value: vec![],
        };
        let data = attestation.into_data();
        assert_eq!(data.len(), ATTESTATION_HEADER_SIZE);
    }
}
