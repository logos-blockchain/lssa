use nssa_core::{
    account::{Account, AccountId, AccountWithMetadata, Data},
    program::{ProgramInput, read_nssa_inputs, write_nssa_outputs},
};

// The token program has three functions:
// 1. New token definition.
//    Arguments to this function are:
//      * Two **default** accounts: [definition_account, holding_account].
//        The first default account will be initialized with the token definition account values. The second account will
//        be initialized to a token holding account for the new token, holding the entire total supply.
//      * An instruction data of 23-bytes, indicating the total supply and the token name, with
//        the following layout:
//        [0x00 || total_supply (little-endian 16 bytes) || name (6 bytes)]
//        The name cannot be equal to [0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
// 2. Token transfer
//    Arguments to this function are:
//      * Two accounts: [sender_account, recipient_account].
//      * An instruction data byte string of length 23, indicating the total supply with the following layout
//        [0x01 || amount (little-endian 16 bytes) || 0x00 || 0x00 || 0x00 || 0x00 || 0x00 || 0x00].
// 3. Initialize account with zero balance
//    Arguments to this function are:
//      * Two accounts: [definition_account, account_to_initialize].
//      * An dummy byte string of length 23, with the following layout
//        [0x02 || 0x00 || 0x00 || 0x00 || ... || 0x00 || 0x00].
// 4. Burn tokens from a Toking Holding account (thus lowering total supply)
//    Arguments to this function are:
//      * Two accounts: [definition_account, holding_account].
//      * An instruction data byte string of length 23, indicating the balance to burn with the folloiwng layout
//       [0x03 || amount (little-endian 16 bytes) || 0x00 || 0x00 || 0x00 || 0x00 || 0x00 || 0x00].
// 5. Mint additional supply of tokens tokens to a Toking Holding account (thus increasing total supply)
//    Arguments to this function are:
//      * Two accounts: [definition_account, holding_account].
//      * An instruction data byte string of length 23, indicating the balance to mint with the folloiwng layout
//       [0x04 || amount (little-endian 16 bytes) || 0x00 || 0x00 || 0x00 || 0x00 || 0x00 || 0x00].

const TOKEN_DEFINITION_TYPE: u8 = 0;
const TOKEN_DEFINITION_DATA_SIZE: usize = 23;

const TOKEN_HOLDING_TYPE: u8 = 1;
const TOKEN_HOLDING_DATA_SIZE: usize = 49;

struct TokenDefinition {
    account_type: u8,
    name: [u8; 6],
    total_supply: u128,
}

struct TokenHolding {
    account_type: u8,
    definition_id: AccountId,
    balance: u128,
}

impl TokenDefinition {
    fn into_data(self) -> Vec<u8> {
        let mut bytes = [0; TOKEN_DEFINITION_DATA_SIZE];
        bytes[0] = self.account_type;
        bytes[1..7].copy_from_slice(&self.name);
        bytes[7..].copy_from_slice(&self.total_supply.to_le_bytes());
        bytes.into()
    }

    fn parse(data: &[u8]) -> Option<Self> {
        if data.len() != TOKEN_DEFINITION_DATA_SIZE || data[0] != TOKEN_DEFINITION_TYPE {
            None
        } else {
            let account_type = data[0];
            let name = data[1..7].try_into().unwrap();
            let total_supply = u128::from_le_bytes(
                data[7..]
                    .try_into()
                    .expect("Total supply must be 16 bytes little-endian"),
            );
            Some(Self {
                account_type,
                name,
                total_supply,
            })
        }
    }
}

impl TokenHolding {
    fn new(definition_id: &AccountId) -> Self {
        Self {
            account_type: TOKEN_HOLDING_TYPE,
            definition_id: definition_id.clone(),
            balance: 0,
        }
    }

    fn parse(data: &[u8]) -> Option<Self> {
        if data.len() != TOKEN_HOLDING_DATA_SIZE || data[0] != TOKEN_HOLDING_TYPE {
            None
        } else {
            let account_type = data[0];
            let definition_id = AccountId::new(
                data[1..33]
                    .try_into()
                    .expect("Defintion ID must be 32 bytes long"),
            );
            let balance = u128::from_le_bytes(
                data[33..]
                    .try_into()
                    .expect("balance must be 16 bytes little-endian"),
            );
            Some(Self {
                definition_id,
                balance,
                account_type,
            })
        }
    }

    fn into_data(self) -> Data {
        let mut bytes = [0; TOKEN_HOLDING_DATA_SIZE];
        bytes[0] = self.account_type;
        bytes[1..33].copy_from_slice(&self.definition_id.to_bytes());
        bytes[33..].copy_from_slice(&self.balance.to_le_bytes());
        bytes.into()
    }
}

fn transfer(pre_states: &[AccountWithMetadata], balance_to_move: u128) -> Vec<Account> {
    if pre_states.len() != 2 {
        panic!("Invalid number of input accounts");
    }
    let sender = &pre_states[0];
    let recipient = &pre_states[1];

    let mut sender_holding =
        TokenHolding::parse(&sender.account.data).expect("Invalid sender data");
    let mut recipient_holding = if recipient.account == Account::default() {
        TokenHolding::new(&sender_holding.definition_id)
    } else {
        TokenHolding::parse(&recipient.account.data).expect("Invalid recipient data")
    };

    if sender_holding.definition_id != recipient_holding.definition_id {
        panic!("Sender and recipient definition id mismatch");
    }

    if sender_holding.balance < balance_to_move {
        panic!("Insufficient balance");
    }

    if !sender.is_authorized {
        panic!("Sender authorization is missing");
    }

    sender_holding.balance -= balance_to_move;
    recipient_holding.balance = recipient_holding
        .balance
        .checked_add(balance_to_move)
        .expect("Recipient balance overflow.");

    let sender_post = {
        let mut this = sender.account.clone();
        this.data = sender_holding.into_data();
        this
    };
    let recipient_post = {
        let mut this = recipient.account.clone();
        this.data = recipient_holding.into_data();
        this
    };

    vec![sender_post, recipient_post]
}

fn new_definition(
    pre_states: &[AccountWithMetadata],
    name: [u8; 6],
    total_supply: u128,
) -> Vec<Account> {
    if pre_states.len() != 2 {
        panic!("Invalid number of input accounts");
    }
    let definition_target_account = &pre_states[0];
    let holding_target_account = &pre_states[1];

    if definition_target_account.account != Account::default() {
        panic!("Definition target account must have default values");
    }

    if holding_target_account.account != Account::default() {
        panic!("Holding target account must have default values");
    }

    let token_definition = TokenDefinition {
        account_type: TOKEN_DEFINITION_TYPE,
        name,
        total_supply,
    };

    let token_holding = TokenHolding {
        account_type: TOKEN_HOLDING_TYPE,
        definition_id: definition_target_account.account_id.clone(),
        balance: total_supply,
    };

    let mut definition_target_account_post = definition_target_account.account.clone();
    definition_target_account_post.data = token_definition.into_data();

    let mut holding_target_account_post = holding_target_account.account.clone();
    holding_target_account_post.data = token_holding.into_data();

    vec![definition_target_account_post, holding_target_account_post]
}

fn initialize_account(pre_states: &[AccountWithMetadata]) -> Vec<Account> {
    if pre_states.len() != 2 {
        panic!("Invalid number of accounts");
    }

    let definition = &pre_states[0];
    let account_to_initialize = &pre_states[1];

    if account_to_initialize.account != Account::default() {
        panic!("Only uninitialized accounts can be initialized");
    }

    // TODO: We should check that this is an account owned by the token program.
    // This check can't be done here since the ID of the program is known only after compiling it
    //
    // Check definition account is valid
    let _definition_values =
        TokenDefinition::parse(&definition.account.data).expect("Definition account must be valid");
    let holding_values = TokenHolding::new(&definition.account_id);

    let definition_post = definition.account.clone();
    let mut account_to_initialize_post = account_to_initialize.account.clone();
    account_to_initialize_post.data = holding_values.into_data();

    vec![definition_post, account_to_initialize_post]
}

fn burn(pre_states: &[AccountWithMetadata], balance_to_burn: u128) -> Vec<Account> {

    if pre_states.len() != 2 {
        panic!("Invalid number of accounts");
    }

    let definition = &pre_states[0];
    let user_holding = &pre_states[1];

    let definition_values =
        TokenDefinition::parse(&definition.account.data).expect("Definition account must be valid");
    let user_values = 
        TokenHolding::parse(&user_holding.account.data).expect("Token Holding account must be valid");

    if definition.account_id != user_values.definition_id {
        panic!("Mismatch token definition and token holding");
    }

    if !user_holding.is_authorized {
        panic!("Authorization is missing");
    }

    if user_values.balance < balance_to_burn {
        panic!("Insufficient balance to burn");
    }

    let mut post_user_holding = user_holding.account.clone();
    let mut post_definition = definition.account.clone();

    post_user_holding.data = TokenHolding::into_data(
        TokenHolding {
            account_type: user_values.account_type,
            definition_id: user_values.definition_id,
            balance: user_values.balance - balance_to_burn,
        }
    );

    post_definition.data = TokenDefinition::into_data(
        TokenDefinition {
            account_type: definition_values.account_type,
            name: definition_values.name,
            total_supply: definition_values.total_supply - balance_to_burn,
        }
    );

    vec![post_definition, post_user_holding]
}

fn mint_additional_supply(pre_states: &[AccountWithMetadata], amount_to_mint: u128) -> Vec<Account> {
    if pre_states.len() != 2 {
        panic!("Invalid number of accounts");
    }
    
    let definition = &pre_states[0];
    let token_holding = &pre_states[1];

    if !definition.is_authorized {
        panic!("Definition authorization is missing");
    }

    let definition_values =
        TokenDefinition::parse(&definition.account.data).expect("Definition account must be valid");

    let mut token_holding_post = token_holding.account.clone();

    //TODO: add overflow protection
    // TokenDefinition.supply_limit + amount_to_mint

    let token_holding_values: TokenHolding = if token_holding.account == Account::default() {
        TokenHolding::new(&definition.account_id)
    } else { TokenHolding::parse(&token_holding.account.data).expect("Holding account must be valid") };

    if definition.account_id != token_holding_values.definition_id {
        panic!("Mismatch token definition and token holding");
    }

    let mut post_definition = definition.account.clone();

    let mut token_holding_post = token_holding.account.clone();

    token_holding_post.data = TokenHolding::into_data(
        TokenHolding {
            account_type: token_holding_values.account_type,
            definition_id: token_holding_values.definition_id,
            balance: token_holding_values.balance + amount_to_mint,
        }
    );

    post_definition.data = TokenDefinition::into_data(
        TokenDefinition {
            account_type: definition_values.account_type,
            name: definition_values.name,
            total_supply: definition_values.total_supply + amount_to_mint,
        }
    );

    vec![post_definition, token_holding_post]
}


type Instruction = [u8; 23];

fn main() {
    let ProgramInput {
        pre_states,
        instruction,
    } = read_nssa_inputs::<Instruction>();

    let (pre_states, post_states) = match instruction[0] {
        0 => {
            // Parse instruction
            let total_supply = u128::from_le_bytes(
                instruction[1..17]
                    .try_into()
                    .expect("Total supply must be 16 bytes little-endian"),
            );
            let name: [u8; 6] = instruction[17..]
                .try_into()
                .expect("Name must be 6 bytes long");
            assert_ne!(name, [0; 6]);

            // Execute
            let post_states = new_definition(&pre_states, name, total_supply);
            (pre_states, post_states)
        }
        1 => {
            // Parse instruction
            let balance_to_move = u128::from_le_bytes(
                instruction[1..17]
                    .try_into()
                    .expect("Balance to move must be 16 bytes little-endian"),
            );
            let name: [u8; 6] = instruction[17..]
                .try_into()
                .expect("Name must be 6 bytes long");
            assert_eq!(name, [0; 6]);

            // Execute
            let post_states = transfer(&pre_states, balance_to_move);
            (pre_states, post_states)
        }
        2 => {
            // Initialize account
            assert_eq!(instruction[1..], [0; 22]);
            let post_states = initialize_account(&pre_states);
            (pre_states, post_states)
        }
        3 => {
            let balance_to_burn = u128::from_le_bytes(
                instruction[1..17]
                    .try_into()
                    .expect("Balance to burn must be 16 bytes little-endian"),
            );
            let name: [u8; 6] = instruction[17..]
                .try_into()
                .expect("Name must be 6 bytes long");
            assert_eq!(name, [0; 6]);

            // Execute
            let post_states = burn(&pre_states, balance_to_burn);
            (pre_states, post_states)
        }
        4 => {
            let balance_to_mint = u128::from_le_bytes(
                instruction[1..17]
                    .try_into()
                    .expect("Balance to burn must be 16 bytes little-endian"),
            );
            let name: [u8; 6] = instruction[17..]
                .try_into()
                .expect("Name must be 6 bytes long");
            assert_eq!(name, [0; 6]);

            // Execute
            let post_states = mint_additional_supply(&pre_states, balance_to_mint);
            (pre_states, post_states)
        }
        _ => panic!("Invalid instruction"),
    };

    write_nssa_outputs(pre_states, post_states);
}

#[cfg(test)]
mod tests {
    use nssa_core::account::{Account, AccountId, AccountWithMetadata};

    use crate::{
        TOKEN_DEFINITION_DATA_SIZE, TOKEN_HOLDING_DATA_SIZE, TOKEN_HOLDING_TYPE,
        TOKEN_DEFINITION_TYPE, TokenDefinition, TokenHolding,
        initialize_account, new_definition, transfer, burn, mint_additional_supply,
    };

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_new_definition_with_invalid_number_of_accounts_1() {
        let pre_states = vec![AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([1; 32]),
        }];
        let _post_states = new_definition(&pre_states, [0xca, 0xfe, 0xca, 0xfe, 0xca, 0xfe], 10);
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_new_definition_with_invalid_number_of_accounts_2() {
        let pre_states = vec![
            AccountWithMetadata {
                account: Account::default(),
                is_authorized: true,
                account_id: AccountId::new([1; 32]),
            },
            AccountWithMetadata {
                account: Account::default(),
                is_authorized: true,
                account_id: AccountId::new([2; 32]),
            },
            AccountWithMetadata {
                account: Account::default(),
                is_authorized: true,
                account_id: AccountId::new([3; 32]),
            },
        ];
        let _post_states = new_definition(&pre_states, [0xca, 0xfe, 0xca, 0xfe, 0xca, 0xfe], 10);
    }

    #[should_panic(expected = "Definition target account must have default values")]
    #[test]
    fn test_new_definition_non_default_first_account_should_fail() {
        let pre_states = vec![
            AccountWithMetadata {
                account: Account {
                    program_owner: [1, 2, 3, 4, 5, 6, 7, 8],
                    ..Account::default()
                },
                is_authorized: true,
                account_id: AccountId::new([1; 32]),
            },
            AccountWithMetadata {
                account: Account::default(),
                is_authorized: true,
                account_id: AccountId::new([2; 32]),
            },
        ];
        let _post_states = new_definition(&pre_states, [0xca, 0xfe, 0xca, 0xfe, 0xca, 0xfe], 10);
    }

    #[should_panic(expected = "Holding target account must have default values")]
    #[test]
    fn test_new_definition_non_default_second_account_should_fail() {
        let pre_states = vec![
            AccountWithMetadata {
                account: Account::default(),
                is_authorized: true,
                account_id: AccountId::new([1; 32]),
            },
            AccountWithMetadata {
                account: Account {
                    program_owner: [1, 2, 3, 4, 5, 6, 7, 8],
                    ..Account::default()
                },
                is_authorized: true,
                account_id: AccountId::new([2; 32]),
            },
        ];
        let _post_states = new_definition(&pre_states, [0xca, 0xfe, 0xca, 0xfe, 0xca, 0xfe], 10);
    }

    #[test]
    fn test_new_definition_with_valid_inputs_succeeds() {
        let pre_states = vec![
            AccountWithMetadata {
                account: Account::default(),
                is_authorized: false,
                account_id: AccountId::new([
                    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22,
                    23, 24, 25, 26, 27, 28, 29, 30, 31, 32,
                ]),
            },
            AccountWithMetadata {
                account: Account {
                    ..Account::default()
                },
                is_authorized: false,
                account_id: AccountId::new([2; 32]),
            },
        ];

        let post_states = new_definition(&pre_states, [0xca, 0xfe, 0xca, 0xfe, 0xca, 0xfe], 10);
        let [definition_account, holding_account] = post_states.try_into().ok().unwrap();
        assert_eq!(
            definition_account.data,
            vec![
                0, 0xca, 0xfe, 0xca, 0xfe, 0xca, 0xfe, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0
            ]
        );
        assert_eq!(
            holding_account.data,
            vec![
                1, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22,
                23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0
            ]
        );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_transfer_with_invalid_number_of_accounts_1() {
        let pre_states = vec![AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([1; 32]),
        }];
        let _post_states = transfer(&pre_states, 10);
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_transfer_with_invalid_number_of_accounts_2() {
        let pre_states = vec![
            AccountWithMetadata {
                account: Account::default(),
                is_authorized: true,
                account_id: AccountId::new([1; 32]),
            },
            AccountWithMetadata {
                account: Account::default(),
                is_authorized: true,
                account_id: AccountId::new([2; 32]),
            },
            AccountWithMetadata {
                account: Account::default(),
                is_authorized: true,
                account_id: AccountId::new([3; 32]),
            },
        ];
        let _post_states = transfer(&pre_states, 10);
    }

    #[should_panic(expected = "Invalid sender data")]
    #[test]
    fn test_transfer_invalid_instruction_type_should_fail() {
        let invalid_type = TOKEN_HOLDING_TYPE ^ 1;
        let pre_states = vec![
            AccountWithMetadata {
                account: Account {
                    // First byte should be `TOKEN_HOLDING_TYPE` for token holding accounts
                    data: vec![invalid_type; TOKEN_HOLDING_DATA_SIZE],
                    ..Account::default()
                },
                is_authorized: true,
                account_id: AccountId::new([1; 32]),
            },
            AccountWithMetadata {
                account: Account::default(),
                is_authorized: true,
                account_id: AccountId::new([2; 32]),
            },
        ];
        let _post_states = transfer(&pre_states, 10);
    }

    #[should_panic(expected = "Invalid sender data")]
    #[test]
    fn test_transfer_invalid_data_size_should_fail_1() {
        let pre_states = vec![
            AccountWithMetadata {
                account: Account {
                    // Data must be of exact length `TOKEN_HOLDING_DATA_SIZE`
                    data: vec![1; TOKEN_HOLDING_DATA_SIZE - 1],
                    ..Account::default()
                },
                is_authorized: true,
                account_id: AccountId::new([1; 32]),
            },
            AccountWithMetadata {
                account: Account::default(),
                is_authorized: true,
                account_id: AccountId::new([2; 32]),
            },
        ];
        let _post_states = transfer(&pre_states, 10);
    }

    #[should_panic(expected = "Invalid sender data")]
    #[test]
    fn test_transfer_invalid_data_size_should_fail_2() {
        let pre_states = vec![
            AccountWithMetadata {
                account: Account {
                    // Data must be of exact length `TOKEN_HOLDING_DATA_SIZE`
                    data: vec![1; TOKEN_HOLDING_DATA_SIZE + 1],
                    ..Account::default()
                },
                is_authorized: true,
                account_id: AccountId::new([1; 32]),
            },
            AccountWithMetadata {
                account: Account::default(),
                is_authorized: true,
                account_id: AccountId::new([2; 32]),
            },
        ];
        let _post_states = transfer(&pre_states, 10);
    }

    #[should_panic(expected = "Sender and recipient definition id mismatch")]
    #[test]
    fn test_transfer_with_different_definition_ids_should_fail() {
        let pre_states = vec![
            AccountWithMetadata {
                account: Account {
                    data: vec![1; TOKEN_HOLDING_DATA_SIZE],
                    ..Account::default()
                },
                is_authorized: true,
                account_id: AccountId::new([1; 32]),
            },
            AccountWithMetadata {
                account: Account {
                    data: vec![1]
                        .into_iter()
                        .chain(vec![2; TOKEN_HOLDING_DATA_SIZE - 1])
                        .collect(),
                    ..Account::default()
                },
                is_authorized: true,
                account_id: AccountId::new([2; 32]),
            },
        ];
        let _post_states = transfer(&pre_states, 10);
    }

    #[should_panic(expected = "Insufficient balance")]
    #[test]
    fn test_transfer_with_insufficient_balance_should_fail() {
        let pre_states = vec![
            AccountWithMetadata {
                account: Account {
                    // Account with balance 37
                    data: vec![1; TOKEN_HOLDING_DATA_SIZE - 16]
                        .into_iter()
                        .chain(u128::to_le_bytes(37))
                        .collect(),
                    ..Account::default()
                },
                is_authorized: true,
                account_id: AccountId::new([1; 32]),
            },
            AccountWithMetadata {
                account: Account {
                    data: vec![1; TOKEN_HOLDING_DATA_SIZE],
                    ..Account::default()
                },
                is_authorized: true,
                account_id: AccountId::new([2; 32]),
            },
        ];
        // Attempt to transfer 38 tokens
        let _post_states = transfer(&pre_states, 38);
    }

    #[should_panic(expected = "Sender authorization is missing")]
    #[test]
    fn test_transfer_without_sender_authorization_should_fail() {
        let pre_states = vec![
            AccountWithMetadata {
                account: Account {
                    // Account with balance 37
                    data: vec![1; TOKEN_HOLDING_DATA_SIZE - 16]
                        .into_iter()
                        .chain(u128::to_le_bytes(37))
                        .collect(),
                    ..Account::default()
                },
                is_authorized: false,
                account_id: AccountId::new([1; 32]),
            },
            AccountWithMetadata {
                account: Account {
                    data: vec![1; TOKEN_HOLDING_DATA_SIZE],
                    ..Account::default()
                },
                is_authorized: true,
                account_id: AccountId::new([2; 32]),
            },
        ];
        let _post_states = transfer(&pre_states, 37);
    }

    #[test]
    fn test_transfer_with_valid_inputs_succeeds() {
        let pre_states = vec![
            AccountWithMetadata {
                account: Account {
                    // Account with balance 37
                    data: vec![1; TOKEN_HOLDING_DATA_SIZE - 16]
                        .into_iter()
                        .chain(u128::to_le_bytes(37))
                        .collect(),
                    ..Account::default()
                },
                is_authorized: true,
                account_id: AccountId::new([1; 32]),
            },
            AccountWithMetadata {
                account: Account {
                    // Account with balance 255
                    data: vec![1; TOKEN_HOLDING_DATA_SIZE - 16]
                        .into_iter()
                        .chain(u128::to_le_bytes(255))
                        .collect(),
                    ..Account::default()
                },
                is_authorized: true,
                account_id: AccountId::new([2; 32]),
            },
        ];
        let post_states = transfer(&pre_states, 11);
        let [sender_post, recipient_post] = post_states.try_into().ok().unwrap();
        assert_eq!(
            sender_post.data,
            vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 26, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ]
        );
        assert_eq!(
            recipient_post.data,
            vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ]
        );
    }

    #[test]
    fn test_token_initialize_account_succeeds() {
        let pre_states = vec![
            AccountWithMetadata {
                account: Account {
                    // Definition ID with
                    data: vec![0; TOKEN_DEFINITION_DATA_SIZE - 16]
                        .into_iter()
                        .chain(u128::to_le_bytes(1000))
                        .collect(),
                    ..Account::default()
                },
                is_authorized: false,
                account_id: AccountId::new([1; 32]),
            },
            AccountWithMetadata {
                account: Account::default(),
                is_authorized: false,
                account_id: AccountId::new([2; 32]),
            },
        ];
        let post_states = initialize_account(&pre_states);
        let [definition, holding] = post_states.try_into().ok().unwrap();
        assert_eq!(definition.data, pre_states[0].account.data);
        assert_eq!(
            holding.data,
            vec![
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ]
        );
    }

    enum BalanceEnum {
        init_supply,
        holding_balance,
        init_supply_burned,
        holding_balance_burned,
        burn_success,
        burn_insufficient,
        mint_success,
        init_supply_mint,
        holding_balance_mint,
    }

    enum AccountsEnum {
        definition_account_auth,
        definition_account_not_auth,
        holding_diff_def,
        holding_same_def_auth,
        holding_same_def_not_auth,
        definition_account_post_burn,
        holding_account_post_burn,
        uninit,
        init_mint,
        definition_account_mint,
        holding_same_def_mint,
    }

    enum IdEnum {
        pool_definition_id,
        pool_definition_id_diff,
        holding_id,
    }

    fn helper_account_constructor(selection: AccountsEnum) -> AccountWithMetadata{
        match selection {
            AccountsEnum::definition_account_auth => AccountWithMetadata {
                account: Account {
                        program_owner: [5u32;8],
                        balance: 0u128,
                        data: TokenDefinition::into_data(
                        TokenDefinition {
                            account_type: TOKEN_DEFINITION_TYPE,
                            name: [2; 6],
                            total_supply:  helper_balance_constructor(BalanceEnum::init_supply),
                        }),
                        nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::pool_definition_id),
            },
            AccountsEnum::definition_account_not_auth => AccountWithMetadata {
                account: Account {
                        program_owner: [5u32; 8],
                        balance: 0u128,
                        data: TokenDefinition::into_data(
                        TokenDefinition {
                            account_type: TOKEN_DEFINITION_TYPE,
                            name: [2; 6],
                            total_supply:  helper_balance_constructor(BalanceEnum::init_supply),
                        }),
                        nonce: 0,
                },
                is_authorized: false,
                account_id: helper_id_constructor(IdEnum::pool_definition_id),
            },
            AccountsEnum::holding_diff_def => AccountWithMetadata {
                account: Account {
                    program_owner: [5u32;8],
                    balance: 0u128,
                    data: TokenHolding::into_data(
                        TokenHolding {
                            account_type: TOKEN_HOLDING_TYPE,
                            definition_id: helper_id_constructor(IdEnum::pool_definition_id_diff),
                            balance:  helper_balance_constructor(BalanceEnum::holding_balance),
                        }
                    ),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::holding_id),
            },
            AccountsEnum::holding_same_def_auth => AccountWithMetadata {
                account: Account {
                    program_owner: [5u32;8],
                    balance: 0u128,
                    data: TokenHolding::into_data(
                        TokenHolding {
                            account_type: TOKEN_HOLDING_TYPE,
                            definition_id: helper_id_constructor(IdEnum::pool_definition_id),
                            balance:  helper_balance_constructor(BalanceEnum::holding_balance),
                        }
                    ),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::holding_id),
            },
            AccountsEnum::holding_same_def_not_auth => AccountWithMetadata {
                account: Account {
                    program_owner: [5u32;8],
                    balance: 0u128,
                    data: TokenHolding::into_data(
                        TokenHolding {
                            account_type: TOKEN_HOLDING_TYPE,
                            definition_id: helper_id_constructor(IdEnum::pool_definition_id),
                            balance:  helper_balance_constructor(BalanceEnum::holding_balance),
                        }
                    ),
                    nonce: 0,
                },
                is_authorized: false,
                account_id: helper_id_constructor(IdEnum::holding_id),
            },
            AccountsEnum::definition_account_post_burn => AccountWithMetadata {
                account: Account {
                        program_owner: [5u32;8],
                        balance: 0u128,
                        data: TokenDefinition::into_data(
                        TokenDefinition {
                            account_type: TOKEN_DEFINITION_TYPE,
                            name: [2; 6],
                            total_supply:  helper_balance_constructor(BalanceEnum::init_supply_burned),
                        }),
                        nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::pool_definition_id),
            },
            AccountsEnum::holding_same_def_auth => AccountWithMetadata {
                account: Account {
                    program_owner: [5u32;8],
                    balance: 0u128,
                    data: TokenHolding::into_data(
                        TokenHolding {
                            account_type: TOKEN_HOLDING_TYPE,
                            definition_id: helper_id_constructor(IdEnum::pool_definition_id),
                            balance:  helper_balance_constructor(BalanceEnum::holding_balance),
                        }
                    ),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::holding_id),
            },
            AccountsEnum::holding_account_post_burn => AccountWithMetadata {
                account: Account {
                    program_owner: [5u32;8],
                    balance: 0u128,
                    data: TokenHolding::into_data(
                        TokenHolding {
                            account_type: TOKEN_HOLDING_TYPE,
                            definition_id: helper_id_constructor(IdEnum::pool_definition_id),
                            balance:  helper_balance_constructor(BalanceEnum::holding_balance_burned),
                        }
                    ),
                    nonce: 0,
                },
                is_authorized: false,
                account_id: helper_id_constructor(IdEnum::holding_id),
            },
            AccountsEnum::uninit => AccountWithMetadata {
                account: Account::default(),
                is_authorized: false,
                account_id: helper_id_constructor(IdEnum::holding_id),
            },
            AccountsEnum::init_mint => AccountWithMetadata {
                account: Account {
                    program_owner: [0u32;8],
                    balance: 0u128,
                    data: TokenHolding::into_data(
                        TokenHolding {
                            account_type: TOKEN_HOLDING_TYPE,
                            definition_id: helper_id_constructor(IdEnum::pool_definition_id),
                            balance:  helper_balance_constructor(BalanceEnum::mint_success),
                        }
                    ),
                    nonce: 0,
                },
                is_authorized: false,
                account_id: helper_id_constructor(IdEnum::holding_id),
            },
            AccountsEnum::holding_same_def_mint => AccountWithMetadata {
                account: Account {
                        program_owner: [5u32;8],
                        balance: 0u128,
                        data: TokenHolding::into_data(
                        TokenHolding {
                            account_type: TOKEN_HOLDING_TYPE,
                            definition_id: helper_id_constructor(IdEnum::pool_definition_id),
                            balance:  helper_balance_constructor(BalanceEnum::holding_balance_mint),
                        }
                    ),
                        nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::pool_definition_id),
            },
            AccountsEnum::definition_account_mint => AccountWithMetadata {
                account: Account {
                        program_owner: [5u32;8],
                        balance: 0u128,
                        data: TokenDefinition::into_data(
                        TokenDefinition {
                            account_type: TOKEN_DEFINITION_TYPE,
                            name: [2; 6],
                            total_supply:  helper_balance_constructor(BalanceEnum::init_supply_mint),
                        }),
                        nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::pool_definition_id),
            },
            _ => panic!("Invalid selection")
        }
    }

    fn helper_balance_constructor(selection: BalanceEnum) -> u128 {
        match selection {
            BalanceEnum::init_supply => 100_000,
            BalanceEnum::holding_balance => 1_000,
            BalanceEnum::init_supply_burned => 99_500,
            BalanceEnum::holding_balance_burned => 500,
            BalanceEnum::burn_success => 500,
            BalanceEnum::burn_insufficient => 1_500,
            BalanceEnum::mint_success => 50_000,
            BalanceEnum::init_supply_mint => 150_000,
            BalanceEnum::holding_balance_mint => 51_000,
            _ => panic!("Invalid selection")
        }
    }

    fn helper_id_constructor(selection: IdEnum) -> AccountId {
        match selection {
            IdEnum::pool_definition_id => AccountId::new([15;32]),
            IdEnum::pool_definition_id_diff => AccountId::new([16;32]),
            IdEnum::holding_id => AccountId::new([17;32]),
        }
    }

    #[test]
    #[should_panic(expected = "Invalid number of accounts")]
    fn test_burn_invalid_number_of_accounts() {
        let pre_states = vec![
                helper_account_constructor(AccountsEnum::definition_account_auth),
        ];
        let _post_states = burn(&pre_states, helper_balance_constructor(BalanceEnum::burn_success));
    }

    #[test]
    #[should_panic(expected = "Mismatch token definition and token holding")]
    fn test_burn_mismatch_def() {
        let pre_states = vec![
                helper_account_constructor(AccountsEnum::definition_account_auth),
                helper_account_constructor(AccountsEnum::holding_diff_def),
        ];
        let _post_states = burn(&pre_states, helper_balance_constructor(BalanceEnum::burn_success));
    }

    #[test]
    #[should_panic(expected = "Authorization is missing")]
    fn test_burn_missing_authorization() {
        let pre_states = vec![
                helper_account_constructor(AccountsEnum::definition_account_auth),
                helper_account_constructor(AccountsEnum::holding_same_def_not_auth),
        ];
        let _post_states = burn(&pre_states, helper_balance_constructor(BalanceEnum::burn_success));
    }

    #[test]
    #[should_panic(expected = "Insufficient balance to burn")]
    fn test_burn_insufficient_balance() {
        let pre_states = vec![
                helper_account_constructor(AccountsEnum::definition_account_auth),
                helper_account_constructor(AccountsEnum::holding_same_def_auth),
        ];
        let _post_states = burn(&pre_states, helper_balance_constructor(BalanceEnum::burn_insufficient));
    }

    #[test]
    fn test_burn_success() {
        let pre_states = vec![
                helper_account_constructor(AccountsEnum::definition_account_auth),
                helper_account_constructor(AccountsEnum::holding_same_def_auth),
        ];
        let post_states = burn(&pre_states, helper_balance_constructor(BalanceEnum::burn_success));

        let def_post = post_states[0].clone();
        let holding_post = post_states[1].clone();

        assert!(def_post == helper_account_constructor(AccountsEnum::definition_account_post_burn).account);
        assert!(holding_post == helper_account_constructor(AccountsEnum::holding_account_post_burn).account);
    }

    #[test]
    #[should_panic(expected = "Invalid number of accounts")]
    fn test_mint_invalid_number_of_accounts() {
        let pre_states = vec![
                helper_account_constructor(AccountsEnum::definition_account_auth),
        ];
        let _post_states = mint_additional_supply(&pre_states, helper_balance_constructor(BalanceEnum::mint_success));
    }

    #[test]
    #[should_panic(expected = "Holding account must be valid")]
    fn test_mint_not_valid_holding_account() {
        let pre_states = vec![
                helper_account_constructor(AccountsEnum::definition_account_auth),
                helper_account_constructor(AccountsEnum::definition_account_not_auth),
        ];
        let _post_states = mint_additional_supply(&pre_states, helper_balance_constructor(BalanceEnum::mint_success));
    }

    #[test]
    #[should_panic(expected = "Definition authorization is missing")]
    fn test_mint_missing_authorization() {
        let pre_states = vec![
                helper_account_constructor(AccountsEnum::definition_account_not_auth),
                helper_account_constructor(AccountsEnum::holding_same_def_not_auth),
        ];
        let _post_states = mint_additional_supply(&pre_states, helper_balance_constructor(BalanceEnum::mint_success));
    }

    #[test]
    #[should_panic(expected = "Mismatch token definition and token holding")]
    fn test_mint_mismatched_token_definition() {
        let pre_states = vec![
                helper_account_constructor(AccountsEnum::definition_account_auth),
                helper_account_constructor(AccountsEnum::holding_diff_def),
        ];
        let _post_states = mint_additional_supply(&pre_states, helper_balance_constructor(BalanceEnum::mint_success));
    }

    #[test]
    fn test_mint_success() {
        let pre_states = vec![
                helper_account_constructor(AccountsEnum::definition_account_auth),
                helper_account_constructor(AccountsEnum::holding_same_def_not_auth),
        ];
        let post_states = mint_additional_supply(&pre_states, helper_balance_constructor(BalanceEnum::mint_success));

        let def_post = post_states[0].clone();
        let holding_post = post_states[1].clone();

        assert!(def_post == helper_account_constructor(AccountsEnum::definition_account_mint).account);
        assert!(holding_post == helper_account_constructor(AccountsEnum::holding_same_def_mint).account);
    }

    #[test]
    fn test_mint_uninit_holding_success() {
        let pre_states = vec![
                helper_account_constructor(AccountsEnum::definition_account_auth),
                helper_account_constructor(AccountsEnum::uninit),
        ];
        let post_states = mint_additional_supply(&pre_states, helper_balance_constructor(BalanceEnum::mint_success));

        let def_post = post_states[0].clone();
        let holding_post = post_states[1].clone();

        assert!(def_post == helper_account_constructor(AccountsEnum::definition_account_mint).account);
        assert!(holding_post == helper_account_constructor(AccountsEnum::init_mint).account);
    }


}