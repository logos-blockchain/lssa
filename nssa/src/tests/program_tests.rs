use nssa_core::account::{Account, AccountWithMetadata};

use crate::program::Program;

impl Program {
    /// A program that changes the nonce of an account
    pub fn nonce_changer_program() -> Self {
        use test_program_methods::{NONCE_CHANGER_ELF, NONCE_CHANGER_ID};

        Program {
            id: NONCE_CHANGER_ID,
            elf: NONCE_CHANGER_ELF,
        }
    }

    /// A program that produces more output accounts than the inputs it received
    pub fn extra_output_program() -> Self {
        use test_program_methods::{EXTRA_OUTPUT_ELF, EXTRA_OUTPUT_ID};

        Program {
            id: EXTRA_OUTPUT_ID,
            elf: EXTRA_OUTPUT_ELF,
        }
    }

    /// A program that produces less output accounts than the inputs it received
    pub fn missing_output_program() -> Self {
        use test_program_methods::{MISSING_OUTPUT_ELF, MISSING_OUTPUT_ID};

        Program {
            id: MISSING_OUTPUT_ID,
            elf: MISSING_OUTPUT_ELF,
        }
    }

    /// A program that changes the program owner of an account to [0, 1, 2, 3, 4, 5, 6, 7]
    pub fn program_owner_changer() -> Self {
        use test_program_methods::{PROGRAM_OWNER_CHANGER_ELF, PROGRAM_OWNER_CHANGER_ID};

        Program {
            id: PROGRAM_OWNER_CHANGER_ID,
            elf: PROGRAM_OWNER_CHANGER_ELF,
        }
    }

    /// A program that transfers balance without caring about authorizations
    pub fn simple_balance_transfer() -> Self {
        use test_program_methods::{SIMPLE_BALANCE_TRANSFER_ELF, SIMPLE_BALANCE_TRANSFER_ID};

        Program {
            id: SIMPLE_BALANCE_TRANSFER_ID,
            elf: SIMPLE_BALANCE_TRANSFER_ELF,
        }
    }

    /// A program that modifies the data of an account
    pub fn data_changer() -> Self {
        use test_program_methods::{DATA_CHANGER_ELF, DATA_CHANGER_ID};

        Program {
            id: DATA_CHANGER_ID,
            elf: DATA_CHANGER_ELF,
        }
    }

    /// A program that mints balance
    pub fn minter() -> Self {
        use test_program_methods::{MINTER_ELF, MINTER_ID};

        Program {
            id: MINTER_ID,
            elf: MINTER_ELF,
        }
    }

    /// A program that burns balance
    pub fn burner() -> Self {
        use test_program_methods::{BURNER_ELF, BURNER_ID};

        Program {
            id: BURNER_ID,
            elf: BURNER_ELF,
        }
    }
}

#[test]
fn test_program_execution() {
    let program = Program::simple_balance_transfer();
    let balance_to_move: u128 = 11223344556677;
    let instruction_data = Program::serialize_instruction(balance_to_move).unwrap();
    let sender = AccountWithMetadata {
        account: Account {
            balance: 77665544332211,
            ..Account::default()
        },
        is_authorized: false,
    };
    let recipient = AccountWithMetadata {
        account: Account::default(),
        is_authorized: false,
    };

    let expected_sender_post = Account {
        balance: 77665544332211 - balance_to_move,
        program_owner: program.id(),
        ..Account::default()
    };
    let expected_recipient_post = Account {
        balance: balance_to_move,
        // Program claims the account since the pre_state has default prorgam owner
        program_owner: program.id(),
        ..Account::default()
    };
    let [sender_post, recipient_post] = program
        .execute(&[sender, recipient], &instruction_data)
        .unwrap()
        .try_into()
        .unwrap();

    assert_eq!(sender_post, expected_sender_post);
    assert_eq!(recipient_post, expected_recipient_post);
}
