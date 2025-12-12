use nssa_core::{
    account::{Account, AccountId, AccountWithMetadata, Data},
    program::{
        AccountPostState, DEFAULT_PROGRAM_ID, ProgramInput, read_nssa_inputs, write_nssa_outputs,
    },
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
// 4. Burn tokens from a Token Holding account (thus lowering total supply)
//    Arguments to this function are:
//      * Two accounts: [definition_account, holding_account].
//      * Authorization required: holding_account
//      * An instruction data byte string of length 23, indicating the balance to burn with the folloiwng layout
//       [0x03 || amount (little-endian 16 bytes) || 0x00 || 0x00 || 0x00 || 0x00 || 0x00 || 0x00].
// 5. Mint additional supply of tokens tokens to a Token Holding account (thus increasing total supply)
//    Arguments to this function are:
//      * Two accounts: [definition_account, holding_account].
//      * Authorization required: definition_account
//      * An instruction data byte string of length 23, indicating the balance to mint with the folloiwng layout
//       [0x04 || amount (little-endian 16 bytes) || 0x00 || 0x00 || 0x00 || 0x00 || 0x00 || 0x00].

type TokenStandard = u8;

enum TokenStandardEnum {
    FungibleToken,
    FungibleAsset,
    NonFungible,
}
enum MetadataStandardEnum {
    SimpleMetadata,
    ExpandedMetadata,
}

// Remove enums...unnecessary structure
fn helper_token_standard_constructor(selection: TokenStandardEnum) -> u8 {
    match selection {
        TokenStandardEnum::FungibleToken => 0,
        TokenStandardEnum::FungibleAsset => 1,
        TokenStandardEnum::NonFungible => 2,
        _ => panic!("Invalid selection"),
    }
}



fn helper_metadata_standard_constructor(selection: MetadataStandardEnum) -> u8 {
    match selection {
        MetadataStandardEnum::SimpleMetadata => 0,
        MetadataStandardEnum::ExpandedMetadata => 1,
    }
}

const TOKEN_DEFINITION_DATA_SIZE: usize = 55;

const TOKEN_HOLDING_TYPE: u8 = 1;
const TOKEN_HOLDING_MASTER_EDITION: u8 = 2;
const TOKEN_HOLDING_PRINTED: u8 = 3;



const TOKEN_HOLDING_DATA_SIZE: usize = 49;
const CURRENT_VERSION: u8 = 1;
const NUMBER_OF_METADATA_TYPES: u8 = 2;

const TOKEN_METADATA_DATA_SIZE: usize = 463;

struct TokenDefinition {
    account_type: u8, // Token Standard
    name: [u8; 6],
    total_supply: u128,
    metadata_id: AccountId,
}

impl TokenDefinition {
    fn into_data(self) -> Data {
        let mut bytes = Vec::<u8>::new();
        bytes.extend_from_slice(&[self.account_type]);
        bytes.extend_from_slice(&self.name);
        bytes.extend_from_slice(&self.total_supply.to_le_bytes());
        bytes.extend_from_slice(&self.metadata_id.to_bytes());

        if bytes.len() != TOKEN_DEFINITION_DATA_SIZE {
            panic!("Invalid Token Definition data");
        }

        Data::try_from(bytes).expect("Invalid data")
    }

    fn parse(data: &Data) -> Option<Self> {
        let data = Vec::<u8>::from(data.clone());

        if data.len() != TOKEN_DEFINITION_DATA_SIZE {
            None
        } else {
            let account_type = data[0];
            let name = data[1..7].try_into().expect("Name must be a 6 bytes");
            let total_supply = u128::from_le_bytes(
                data[7..23]
                    .try_into()
                    .expect("Total supply must be 16 bytes little-endian"),
            );
            let metadata_id = AccountId::new(
                data[23..TOKEN_DEFINITION_DATA_SIZE]
                    .try_into()
                    .expect("Token Program expects valid Account Id for Metadata"),
            );

            let this = Some(Self {
                account_type,
                name,
                total_supply,
                metadata_id: metadata_id.clone(),
            });

            //TODO tests
            if account_type == //NFTs must have supply 1
                helper_token_standard_constructor(TokenStandardEnum::NonFungible)
                && total_supply != 1
            {
                None
            } else if account_type == //Fungible Tokens do not have metadata.
                helper_token_standard_constructor(TokenStandardEnum::FungibleToken)
                && metadata_id != AccountId::new([0; 32])
            {
                None
            } else {
                this
            }
        }
    }
}

//TODO(edition/master edition/printable)
// => use balance for total prints allowed
// fix logic for NFTs to allow for printables
struct TokenHolding {
    account_type: u8,
    definition_id: AccountId,
    balance: u128,
}

impl TokenHolding {
    fn new(definition_id: &AccountId) -> Self {
        Self {
            account_type: TOKEN_HOLDING_TYPE,
            definition_id: definition_id.clone(),
            balance: 0,
        }
    }

    fn parse(data: &Data) -> Option<Self> {
        let data = Vec::<u8>::from(data.clone());

        if data.len() != TOKEN_HOLDING_DATA_SIZE || data[0] != TOKEN_HOLDING_TYPE {
            return None;
        }

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

    fn into_data(self) -> Data {
        let mut bytes = Vec::<u8>::new();
        bytes.extend_from_slice(&[self.account_type]);
        bytes.extend_from_slice(&self.definition_id.to_bytes());
        bytes.extend_from_slice(&self.balance.to_le_bytes());

        if bytes.len() != TOKEN_HOLDING_DATA_SIZE {
            panic!("Invalid Token Holding data");
        }

        Data::try_from(bytes).expect("Invalid data")
    }
}

struct TokenMetadata {
    account_type: u8, //Not sure if necessary
    version: u8,
    definition_id: AccountId,
    uri: [u8; 200],         //TODO: add to specs; this is the limit Solana uses
    creators: [u8; 250],    //TODO: double check this value;
    primary_sale_date: u64, //BlockId
}

//TODO remove any unwraps
impl TokenMetadata {
    fn into_data(self) -> Data {
        if self.account_type
            != helper_metadata_standard_constructor(MetadataStandardEnum::SimpleMetadata)
            || self.account_type
                != helper_metadata_standard_constructor(MetadataStandardEnum::ExpandedMetadata)
        {
            panic!("Invalid Metadata type");
        }

        let mut bytes = Vec::<u8>::new();
        bytes.extend_from_slice(&[self.account_type]);
        bytes.extend_from_slice(&[self.version]);
        bytes.extend_from_slice(&self.definition_id.to_bytes());
        bytes.extend_from_slice(&self.uri);
        bytes.extend_from_slice(&self.creators);
        bytes.extend_from_slice(&self.primary_sale_date.to_le_bytes());

        if bytes.len() != TOKEN_METADATA_DATA_SIZE {
            panic!("Invalid Token Definition data length");
        }

        Data::try_from(bytes).expect("Invalid data")
    }

    fn parse(data: &Data) -> Option<Self> {
        let data = Vec::<u8>::from(data.clone());

        if data.len() != TOKEN_METADATA_DATA_SIZE || data[0] >= NUMBER_OF_METADATA_TYPES {
            None
        } else {
            let account_type = data[0];
            let version = data[1];
            let definition_id = AccountId::new(
                data[2..34]
                    .try_into()
                    .expect("Token Program expects valid Account Id for Metadata"),
            );
            let uri: [u8; 200] = data[34..234]
                .try_into()
                .expect("Token Program expects valid uri for Metadata");
            let creators: [u8; 250] = data[234..484]
                .try_into()
                .expect("Token Program expects valid creators for Metadata");
            let primary_sale_date = u64::from_le_bytes(
                data[484..TOKEN_METADATA_DATA_SIZE]
                    .try_into()
                    .expect("Token Program expects valid blockid for Metadata"),
            );
            Some(Self {
                account_type,
                version,
                definition_id,
                uri,
                creators,
                primary_sale_date,
            })
        }
    }
}

fn transfer(pre_states: &[AccountWithMetadata], balance_to_move: u128) -> Vec<AccountPostState> {
    if pre_states.len() != 2 {
        panic!("Invalid number of input accounts");
    }
    let sender = &pre_states[0];
    let recipient = &pre_states[1];

    if !sender.is_authorized {
        panic!("Sender authorization is missing");
    }

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

    sender_holding.balance -= sender_holding
        .balance
        .checked_sub(balance_to_move)
        .expect("Checked above");
    recipient_holding.balance = recipient_holding
        .balance
        .checked_add(balance_to_move)
        .expect("Recipient balance overflow");

    let sender_post = {
        let mut this = sender.account.clone();
        this.data = sender_holding.into_data();
        AccountPostState::new(this)
    };

    let recipient_post = {
        let mut this = recipient.account.clone();
        this.data = recipient_holding.into_data();

        // Claim the recipient account if it has default program owner
        if this.program_owner == DEFAULT_PROGRAM_ID {
            AccountPostState::new_claimed(this)
        } else {
            AccountPostState::new(this)
        }
    };

    vec![sender_post, recipient_post]
}

fn new_definition(
    pre_states: &[AccountWithMetadata],
    name: [u8; 6],
    total_supply: u128,
) -> Vec<AccountPostState> {
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
        account_type: helper_token_standard_constructor(TokenStandardEnum::FungibleToken),
        name,
        total_supply,
        metadata_id: AccountId::new([0; 32]),
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

    vec![
        AccountPostState::new_claimed(definition_target_account_post),
        AccountPostState::new_claimed(holding_target_account_post),
    ]
}

fn new_definition_with_metadata(
    pre_states: &[AccountWithMetadata],
    name: [u8; 6],
    total_supply: u128,
    token_standard: u8,
    metadata_standard: u8,
    metadata_values: &Data,
) -> Vec<AccountPostState> {
    if pre_states.len() != 3 {
        panic!("Invalid number of input accounts");
    }

    let definition_target_account = &pre_states[0];
    let metadata_target_account = &pre_states[1];
    let holding_target_account = &pre_states[2];

    if definition_target_account.account != Account::default() {
        panic!("Definition target account must have default values");
    }

    if metadata_target_account.account != Account::default() {
        panic!("Metadata target account must have default values");
    }

    if holding_target_account.account != Account::default() {
        panic!("Holding target account must have default values");
    }

    if !valid_total_supply_for_token_standard(total_supply, token_standard) {
        panic!("Invalid total supply for the specified token supply");
    }

    let token_definition = TokenDefinition {
        account_type: token_standard,
        name,
        total_supply,
        metadata_id: metadata_target_account.account_id.clone(),
    };

    let token_holding = TokenHolding {
        account_type: TOKEN_HOLDING_TYPE,
        definition_id: definition_target_account.account_id.clone(),
        balance: total_supply,
    };

    if metadata_values.len() != 450 {
        panic!("Metadata values data should be 450 bytes");
    }

    let uri: [u8; 200] = metadata_values[0..200]
        .try_into()
        .expect("Token program expects valid uri for Metadata");
    let creators: [u8; 250] = metadata_values[200..450]
        .try_into()
        .expect("Token program expects valid creators for Metadata");

    let token_metadata = TokenMetadata {
        account_type: metadata_standard,
        version: CURRENT_VERSION,
        definition_id: definition_target_account.account_id.clone(),
        uri,
        creators,
        primary_sale_date: 0u64, //TODO: future works to implement this
    };

    let mut definition_target_account_post = definition_target_account.account.clone();
    definition_target_account_post.data = token_definition.into_data();

    let mut holding_target_account_post = holding_target_account.account.clone();
    holding_target_account_post.data = token_holding.into_data();

    let mut metadata_target_account_post = metadata_target_account.account.clone();
    metadata_target_account_post.data = token_metadata.into_data();

    vec![
        AccountPostState::new_claimed(definition_target_account_post),
        AccountPostState::new_claimed(holding_target_account_post),
        AccountPostState::new_claimed(metadata_target_account_post),
    ]
}

fn valid_total_supply_for_token_standard(total_supply: u128, token_standard: u8) -> bool {
    if token_standard == helper_token_standard_constructor(TokenStandardEnum::NonFungible)
        && total_supply != 1
    {
        false
    } else {
        true
    }
}

fn initialize_account(pre_states: &[AccountWithMetadata]) -> Vec<AccountPostState> {
    if pre_states.len() != 2 {
        panic!("Invalid number of accounts");
    }

    let definition = &pre_states[0];
    let account_to_initialize = &pre_states[1];

    if account_to_initialize.account != Account::default() {
        panic!("Only Uninitialized accounts can be initialized");
    }

    // TODO: #212 We should check that this is an account owned by the token program.
    // This check can't be done here since the ID of the program is known only after compiling it
    //
    // Check definition account is valid
    let _definition_values =
        TokenDefinition::parse(&definition.account.data).expect("Definition account must be valid");
    let holding_values = TokenHolding::new(&definition.account_id);

    let definition_post = definition.account.clone();
    let mut account_to_initialize = account_to_initialize.account.clone();
    account_to_initialize.data = holding_values.into_data();

    vec![
        AccountPostState::new(definition_post),
        AccountPostState::new_claimed(account_to_initialize),
    ]
}

fn burn(pre_states: &[AccountWithMetadata], balance_to_burn: u128) -> Vec<AccountPostState> {
    if pre_states.len() != 2 {
        panic!("Invalid number of accounts");
    }

    let definition = &pre_states[0];
    let user_holding = &pre_states[1];

    if !user_holding.is_authorized {
        panic!("Authorization is missing");
    }

    let definition_values = TokenDefinition::parse(&definition.account.data)
        .expect("Token Definition account must be valid");
    let user_values = TokenHolding::parse(&user_holding.account.data)
        .expect("Token Holding account must be valid");

    if definition.account_id != user_values.definition_id {
        panic!("Mismatch Token Definition and Token Holding");
    }

    if user_values.balance < balance_to_burn {
        panic!("Insufficient balance to burn");
    }

    let mut post_user_holding = user_holding.account.clone();
    let mut post_definition = definition.account.clone();

    post_user_holding.data = TokenHolding::into_data(TokenHolding {
        account_type: user_values.account_type,
        definition_id: user_values.definition_id,
        balance: user_values
            .balance
            .checked_sub(balance_to_burn)
            .expect("Checked above"),
    });

    post_definition.data = TokenDefinition::into_data(TokenDefinition {
        account_type: definition_values.account_type,
        name: definition_values.name,
        total_supply: definition_values
            .total_supply
            .checked_sub(balance_to_burn)
            .expect("Total supply underflow"),
        metadata_id: definition_values.metadata_id,
    });

    vec![
        AccountPostState::new(post_definition),
        AccountPostState::new(post_user_holding),
    ]
}

fn is_mintable(account_type: u8) -> bool {
    if account_type == helper_token_standard_constructor(TokenStandardEnum::NonFungible) {
        false
    } else {
        true
    }
}

fn mint_additional_supply(
    pre_states: &[AccountWithMetadata],
    amount_to_mint: u128,
) -> Vec<AccountPostState> {
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

    let token_holding_values: TokenHolding = if token_holding.account == Account::default() {
        TokenHolding::new(&definition.account_id)
    } else {
        TokenHolding::parse(&token_holding.account.data).expect("Holding account must be valid")
    };

    if !is_mintable(definition_values.account_type) {
        panic!("Token Definition's standard does not permit minting additional supply");
    }

    if definition.account_id != token_holding_values.definition_id {
        panic!("Mismatch Token Definition and Token Holding");
    }

    let token_holding_post_data = TokenHolding {
        account_type: token_holding_values.account_type,
        definition_id: token_holding_values.definition_id,
        balance: token_holding_values
            .balance
            .checked_add(amount_to_mint)
            .expect("New balance overflow"),
    };

    let post_total_supply = definition_values
        .total_supply
        .checked_add(amount_to_mint)
        .expect("Total supply overflow");

    let post_definition_data = TokenDefinition {
        account_type: definition_values.account_type,
        name: definition_values.name,
        total_supply: post_total_supply,
        metadata_id: definition_values.metadata_id,
    };

    let post_definition = {
        let mut this = definition.account.clone();
        this.data = post_definition_data.into_data();
        AccountPostState::new(this)
    };

    let token_holding_post = {
        let mut this = token_holding.account.clone();
        this.data = token_holding_post_data.into_data();

        // Claim the recipient account if it has default program owner
        if this.program_owner == DEFAULT_PROGRAM_ID {
            AccountPostState::new_claimed(this)
        } else {
            AccountPostState::new(this)
        }
    };
    vec![post_definition, token_holding_post]
}

//TODO: add vars for 23 and 474
type Instruction = Vec<u8>;
fn main() {
    let ProgramInput {
        pre_states,
        instruction,
    } = read_nssa_inputs::<Instruction>();

    let post_states = match instruction[0] {
        0 => {
            if instruction.len() != 23 {
                panic!("Invalid instruction length");
            }

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
            new_definition(&pre_states, name, total_supply)
        }
        1 => {
            if instruction.len() != 23 {
                panic!("Invalid instruction length");
            }

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
            transfer(&pre_states, balance_to_move)
        }
        2 => {
            if instruction.len() != 23 {
                panic!("Invalid instruction length");
            }

            // Initialize account
            if instruction[1..] != [0; 22] {
                panic!("Invalid instruction for initialize account");
            }
            initialize_account(&pre_states)
        }
        3 => {
            if instruction.len() != 23 {
                panic!("Invalid instruction length");
            }

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
            burn(&pre_states, balance_to_burn)
        }
        4 => {
            if instruction.len() != 23 {
                panic!("Invalid instruction length");
            }

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
            mint_additional_supply(&pre_states, balance_to_mint)
        }
        5 => {
            if instruction.len() != 474 {
                panic!("Invalid instruction length")
            }

            let total_supply = u128::from_le_bytes(instruction[1..17].try_into().expect("Total supply must be 16 bytes little-endian"),);
            let name = instruction[17..23].try_into().expect("Name must be 6 bytes long");
            assert_ne!(name, [0;6]);
            let token_standard = instruction[23];
            let metadata_standard = instruction[24];
            let metadata_values: Data = Data::try_from(instruction[25..474].to_vec()).expect("Invalid metadata");

            new_definition_with_metadata(&pre_states, name, total_supply, token_standard, metadata_standard, &metadata_values)
        }
        _ => panic!("Invalid instruction"),
    };

    write_nssa_outputs(pre_states, post_states);
}

#[cfg(test)]
mod tests {
    use nssa_core::account::{Account, AccountId, AccountWithMetadata, Data};

    use crate::{
        TOKEN_DEFINITION_DATA_SIZE, TOKEN_HOLDING_DATA_SIZE, TOKEN_HOLDING_TYPE, TokenDefinition,
        TokenHolding, TokenStandardEnum, burn, helper_token_standard_constructor,
        initialize_account, mint_additional_supply, new_definition, new_definition_with_metadata,
        transfer,
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

    /*
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
            definition_account.account().data,
            vec![
                0, 0xca, 0xfe, 0xca, 0xfe, 0xca, 0xfe, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0
            ]
        );
        assert_eq!(
            holding_account.account().data,
            vec![
                1, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22,
                23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0
            ]
        );
    }
    */

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
                    data: Data::try_from(vec![invalid_type; TOKEN_HOLDING_DATA_SIZE])
                        .expect("Invalid data"),
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

    /*
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
                sender_post.account().data,
                vec![
                    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                    1, 1, 1, 1, 1, 26, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
                ]
            );
            assert_eq!(
                recipient_post.account().data,
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
                        data: [0; TOKEN_DEFINITION_DATA_SIZE - 16]
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
            assert_eq!(definition.account().data, pre_states[0].account.data);
            assert_eq!(
                holding.account().data,
                vec![
                    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                    1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
                ]
            );
        }
    */
    enum BalanceEnum {
        InitSupply,
        HoldingBalance,
        InitSupplyBurned,
        HoldingBalanceBurned,
        BurnSuccess,
        BurnInsufficient,
        MintSuccess,
        InitSupplyMint,
        HoldingBalanceMint,
        MintOverflow,
    }

    enum AccountsEnum {
        DefinitionAccountAuth,
        DefinitionAccountNotAuth,
        HoldingDiffDef,
        HoldingSameDefAuth,
        HoldingSameDefNotAuth,
        HoldingSameDefNotAuthOverflow,
        DefinitionAccountPostBurn,
        HoldingAccountPostBurn,
        Uninit,
        InitMint,
        DefinitionAccountMint,
        HoldingSameDefMint,
        HoldingSameDefAuthLargeBalance,
        DefinitionAccountAuthNotMintable,
    }

    enum IdEnum {
        PoolDefinitionId,
        PoolDefinitionIdDiff,
        HoldingId,
        MetadataId,
    }

    fn helper_account_constructor(selection: AccountsEnum) -> AccountWithMetadata {
        match selection {
            AccountsEnum::DefinitionAccountAuth => AccountWithMetadata {
                account: Account {
                    program_owner: [5u32; 8],
                    balance: 0u128,
                    data: TokenDefinition::into_data(TokenDefinition {
                        account_type: helper_token_standard_constructor(TokenStandardEnum::FungibleToken),
                        name: [2; 6],
                        total_supply: helper_balance_constructor(BalanceEnum::InitSupply),
                        metadata_id: AccountId::new([0;32]),
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::PoolDefinitionId),
            },
            AccountsEnum::DefinitionAccountNotAuth => AccountWithMetadata {
                account: Account {
                    program_owner: [5u32; 8],
                    balance: 0u128,
                    data: TokenDefinition::into_data(TokenDefinition {
                        account_type: helper_token_standard_constructor(TokenStandardEnum::FungibleToken),
                        name: [2; 6],
                        total_supply: helper_balance_constructor(BalanceEnum::InitSupply),
                        metadata_id: AccountId::new([0;32]),
                    }),
                    nonce: 0,
                },
                is_authorized: false,
                account_id: helper_id_constructor(IdEnum::PoolDefinitionId),
            },
            AccountsEnum::HoldingDiffDef => AccountWithMetadata {
                account: Account {
                    program_owner: [5u32; 8],
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: TOKEN_HOLDING_TYPE,
                        definition_id: helper_id_constructor(IdEnum::PoolDefinitionIdDiff),
                        balance: helper_balance_constructor(BalanceEnum::HoldingBalance),
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::HoldingId),
            },
            AccountsEnum::HoldingSameDefAuth => AccountWithMetadata {
                account: Account {
                    program_owner: [5u32; 8],
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: TOKEN_HOLDING_TYPE,
                        definition_id: helper_id_constructor(IdEnum::PoolDefinitionId),
                        balance: helper_balance_constructor(BalanceEnum::HoldingBalance),
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::HoldingId),
            },
            AccountsEnum::HoldingSameDefNotAuth => AccountWithMetadata {
                account: Account {
                    program_owner: [5u32; 8],
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: TOKEN_HOLDING_TYPE,
                        definition_id: helper_id_constructor(IdEnum::PoolDefinitionId),
                        balance: helper_balance_constructor(BalanceEnum::HoldingBalance),
                    }),
                    nonce: 0,
                },
                is_authorized: false,
                account_id: helper_id_constructor(IdEnum::HoldingId),
            },
            AccountsEnum::HoldingSameDefNotAuthOverflow => AccountWithMetadata {
                account: Account {
                    program_owner: [5u32; 8],
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: TOKEN_HOLDING_TYPE,
                        definition_id: helper_id_constructor(IdEnum::PoolDefinitionId),
                        balance: helper_balance_constructor(BalanceEnum::InitSupply),
                    }),
                    nonce: 0,
                },
                is_authorized: false,
                account_id: helper_id_constructor(IdEnum::HoldingId),
            },
            AccountsEnum::DefinitionAccountPostBurn => AccountWithMetadata {
                account: Account {
                    program_owner: [5u32; 8],
                    balance: 0u128,
                    data: TokenDefinition::into_data(TokenDefinition {
                        account_type: helper_token_standard_constructor(TokenStandardEnum::FungibleToken),
                        name: [2; 6],
                        total_supply: helper_balance_constructor(BalanceEnum::InitSupplyBurned),
                        metadata_id: AccountId::new([0;32]),
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::PoolDefinitionId),
            },
            AccountsEnum::HoldingAccountPostBurn => AccountWithMetadata {
                account: Account {
                    program_owner: [5u32; 8],
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: TOKEN_HOLDING_TYPE,
                        definition_id: helper_id_constructor(IdEnum::PoolDefinitionId),
                        balance: helper_balance_constructor(BalanceEnum::HoldingBalanceBurned),
                    }),
                    nonce: 0,
                },
                is_authorized: false,
                account_id: helper_id_constructor(IdEnum::HoldingId),
            },
            AccountsEnum::Uninit => AccountWithMetadata {
                account: Account::default(),
                is_authorized: false,
                account_id: helper_id_constructor(IdEnum::HoldingId),
            },
            AccountsEnum::InitMint => AccountWithMetadata {
                account: Account {
                    program_owner: [0u32; 8],
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: TOKEN_HOLDING_TYPE,
                        definition_id: helper_id_constructor(IdEnum::PoolDefinitionId),
                        balance: helper_balance_constructor(BalanceEnum::MintSuccess),
                    }),
                    nonce: 0,
                },
                is_authorized: false,
                account_id: helper_id_constructor(IdEnum::HoldingId),
            },
            AccountsEnum::HoldingSameDefMint => AccountWithMetadata {
                account: Account {
                    program_owner: [5u32; 8],
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: TOKEN_HOLDING_TYPE,
                        definition_id: helper_id_constructor(IdEnum::PoolDefinitionId),
                        balance: helper_balance_constructor(BalanceEnum::HoldingBalanceMint),
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::PoolDefinitionId),
            },
            AccountsEnum::DefinitionAccountMint => AccountWithMetadata {
                account: Account {
                    program_owner: [5u32; 8],
                    balance: 0u128,
                    data: TokenDefinition::into_data(TokenDefinition {
                        account_type: helper_token_standard_constructor(TokenStandardEnum::FungibleToken),
                        name: [2; 6],
                        total_supply: helper_balance_constructor(BalanceEnum::InitSupplyMint),
                        metadata_id: AccountId::new([0;32]),
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::PoolDefinitionId),
            },
            AccountsEnum::HoldingSameDefAuthLargeBalance => AccountWithMetadata {
                account: Account {
                    program_owner: [5u32; 8],
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: TOKEN_HOLDING_TYPE,
                        definition_id: helper_id_constructor(IdEnum::PoolDefinitionId),
                        balance: helper_balance_constructor(BalanceEnum::MintOverflow),
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::PoolDefinitionId),
            },
            AccountsEnum::DefinitionAccountAuthNotMintable => AccountWithMetadata {
                account: Account {
                    program_owner: [5u32; 8],
                    balance: 0u128,
                    data: TokenDefinition::into_data(TokenDefinition {
                        account_type: helper_token_standard_constructor(
                            TokenStandardEnum::NonFungible,
                        ),
                        name: [2; 6],
                        total_supply: helper_balance_constructor(BalanceEnum::InitSupplyMint),
                        metadata_id: AccountId::new([0;32]),
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: helper_id_constructor(IdEnum::PoolDefinitionId),
            },
            _ => panic!("Invalid selection"),
        }
    }

    fn helper_balance_constructor(selection: BalanceEnum) -> u128 {
        match selection {
            BalanceEnum::InitSupply => 100_000,
            BalanceEnum::HoldingBalance => 1_000,
            BalanceEnum::InitSupplyBurned => 99_500,
            BalanceEnum::HoldingBalanceBurned => 500,
            BalanceEnum::BurnSuccess => 500,
            BalanceEnum::BurnInsufficient => 1_500,
            BalanceEnum::MintSuccess => 50_000,
            BalanceEnum::InitSupplyMint => 150_000,
            BalanceEnum::HoldingBalanceMint => 51_000,
            BalanceEnum::MintOverflow => (2 as u128).pow(128) - 40_000,
            _ => panic!("Invalid selection"),
        }
    }

    fn helper_id_constructor(selection: IdEnum) -> AccountId {
        match selection {
            IdEnum::PoolDefinitionId => AccountId::new([15; 32]),
            IdEnum::PoolDefinitionIdDiff => AccountId::new([16; 32]),
            IdEnum::HoldingId => AccountId::new([17; 32]),
            IdEnum::MetadataId => AccountId::new([42; 32]),
        }
    }

    #[test]
    #[should_panic(expected = "Invalid number of accounts")]
    fn test_burn_invalid_number_of_accounts() {
        let pre_states = vec![helper_account_constructor(
            AccountsEnum::DefinitionAccountAuth,
        )];
        let _post_states = burn(
            &pre_states,
            helper_balance_constructor(BalanceEnum::BurnSuccess),
        );
    }

    #[test]
    #[should_panic(expected = "Mismatch Token Definition and Token Holding")]
    fn test_burn_mismatch_def() {
        let pre_states = vec![
            helper_account_constructor(AccountsEnum::DefinitionAccountAuth),
            helper_account_constructor(AccountsEnum::HoldingDiffDef),
        ];
        let _post_states = burn(
            &pre_states,
            helper_balance_constructor(BalanceEnum::BurnSuccess),
        );
    }

    #[test]
    #[should_panic(expected = "Authorization is missing")]
    fn test_burn_missing_authorization() {
        let pre_states = vec![
            helper_account_constructor(AccountsEnum::DefinitionAccountAuth),
            helper_account_constructor(AccountsEnum::HoldingSameDefNotAuth),
        ];
        let _post_states = burn(
            &pre_states,
            helper_balance_constructor(BalanceEnum::BurnSuccess),
        );
    }

    #[test]
    #[should_panic(expected = "Insufficient balance to burn")]
    fn test_burn_insufficient_balance() {
        let pre_states = vec![
            helper_account_constructor(AccountsEnum::DefinitionAccountAuth),
            helper_account_constructor(AccountsEnum::HoldingSameDefAuth),
        ];
        let _post_states = burn(
            &pre_states,
            helper_balance_constructor(BalanceEnum::BurnInsufficient),
        );
    }

    #[test]
    #[should_panic(expected = "Total supply underflow")]
    fn test_burn_total_supply_underflow() {
        let pre_states = vec![
            helper_account_constructor(AccountsEnum::DefinitionAccountAuth),
            helper_account_constructor(AccountsEnum::HoldingSameDefAuthLargeBalance),
        ];
        let _post_states = burn(
            &pre_states,
            helper_balance_constructor(BalanceEnum::MintOverflow),
        );
    }

    #[test]
    fn test_burn_success() {
        let pre_states = vec![
            helper_account_constructor(AccountsEnum::DefinitionAccountAuth),
            helper_account_constructor(AccountsEnum::HoldingSameDefAuth),
        ];
        let post_states = burn(
            &pre_states,
            helper_balance_constructor(BalanceEnum::BurnSuccess),
        );

        let def_post = post_states[0].clone();
        let holding_post = post_states[1].clone();

        assert!(
            *def_post.account()
                == helper_account_constructor(AccountsEnum::DefinitionAccountPostBurn).account
        );
        assert!(
            *holding_post.account()
                == helper_account_constructor(AccountsEnum::HoldingAccountPostBurn).account
        );
    }

    #[test]
    #[should_panic(expected = "Invalid number of accounts")]
    fn test_mint_invalid_number_of_accounts() {
        let pre_states = vec![helper_account_constructor(
            AccountsEnum::DefinitionAccountAuth,
        )];
        let _post_states = mint_additional_supply(
            &pre_states,
            helper_balance_constructor(BalanceEnum::MintSuccess),
        );
    }

    #[test]
    #[should_panic(expected = "Holding account must be valid")]
    fn test_mint_not_valid_holding_account() {
        let pre_states = vec![
            helper_account_constructor(AccountsEnum::DefinitionAccountAuth),
            helper_account_constructor(AccountsEnum::DefinitionAccountNotAuth),
        ];
        let _post_states = mint_additional_supply(
            &pre_states,
            helper_balance_constructor(BalanceEnum::MintSuccess),
        );
    }

    #[test]
    #[should_panic(expected = "Definition authorization is missing")]
    fn test_mint_missing_authorization() {
        let pre_states = vec![
            helper_account_constructor(AccountsEnum::DefinitionAccountNotAuth),
            helper_account_constructor(AccountsEnum::HoldingSameDefNotAuth),
        ];
        let _post_states = mint_additional_supply(
            &pre_states,
            helper_balance_constructor(BalanceEnum::MintSuccess),
        );
    }

    #[test]
    #[should_panic(expected = "Mismatch Token Definition and Token Holding")]
    fn test_mint_mismatched_token_definition() {
        let pre_states = vec![
            helper_account_constructor(AccountsEnum::DefinitionAccountAuth),
            helper_account_constructor(AccountsEnum::HoldingDiffDef),
        ];
        let _post_states = mint_additional_supply(
            &pre_states,
            helper_balance_constructor(BalanceEnum::MintSuccess),
        );
    }

    #[test]
    fn test_mint_success() {
        let pre_states = vec![
            helper_account_constructor(AccountsEnum::DefinitionAccountAuth),
            helper_account_constructor(AccountsEnum::HoldingSameDefNotAuth),
        ];
        let post_states = mint_additional_supply(
            &pre_states,
            helper_balance_constructor(BalanceEnum::MintSuccess),
        );

        let def_post = post_states[0].clone();
        let holding_post = post_states[1].clone();

        assert!(
            *def_post.account()
                == helper_account_constructor(AccountsEnum::DefinitionAccountMint).account
        );
        assert!(
            *holding_post.account()
                == helper_account_constructor(AccountsEnum::HoldingSameDefMint).account
        );
    }

    #[test]
    fn test_mint_uninit_holding_success() {
        let pre_states = vec![
            helper_account_constructor(AccountsEnum::DefinitionAccountAuth),
            helper_account_constructor(AccountsEnum::Uninit),
        ];
        let post_states = mint_additional_supply(
            &pre_states,
            helper_balance_constructor(BalanceEnum::MintSuccess),
        );

        let def_post = post_states[0].clone();
        let holding_post = post_states[1].clone();

        assert!(
            *def_post.account()
                == helper_account_constructor(AccountsEnum::DefinitionAccountMint).account
        );
        assert!(
            *holding_post.account() == helper_account_constructor(AccountsEnum::InitMint).account
        );
        assert!(holding_post.requires_claim() == true);
    }

    #[test]
    #[should_panic(expected = "Total supply overflow")]
    fn test_mint_total_supply_overflow() {
        let pre_states = vec![
            helper_account_constructor(AccountsEnum::DefinitionAccountAuth),
            helper_account_constructor(AccountsEnum::HoldingSameDefNotAuth),
        ];
        let _post_states = mint_additional_supply(
            &pre_states,
            helper_balance_constructor(BalanceEnum::MintOverflow),
        );
    }

    #[test]
    #[should_panic(expected = "New balance overflow")]
    fn test_mint_holding_account_overflow() {
        let pre_states = vec![
            helper_account_constructor(AccountsEnum::DefinitionAccountAuth),
            helper_account_constructor(AccountsEnum::HoldingSameDefNotAuthOverflow),
        ];
        let _post_states = mint_additional_supply(
            &pre_states,
            helper_balance_constructor(BalanceEnum::MintOverflow),
        );
    }

    #[test]
    #[should_panic(
        expected = "Token Definition's standard does not permit minting additional supply"
    )]
    fn test_mint_cannot_mint_unmintable_tokens() {
        let pre_states = vec![
            helper_account_constructor(AccountsEnum::DefinitionAccountAuthNotMintable),
            helper_account_constructor(AccountsEnum::HoldingSameDefNotAuth),
        ];
        let _post_states = mint_additional_supply(
            &pre_states,
            helper_balance_constructor(BalanceEnum::MintSuccess),
        );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_new_definition_metadata_with_invalid_number_of_accounts_1() {
        let name = [0xca, 0xfe, 0xca, 0xfe, 0xca, 0xfe];
        let total_supply = 15u128;
        let token_standard = 0u8;
        let metadata_standard = 0u8;
        let metadata_values: Data = Data::try_from([1u8;450].to_vec()).unwrap();

        let pre_states = vec![AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([1; 32]),
        }];
        let _post_states = new_definition_with_metadata(&pre_states,  name, total_supply, token_standard, metadata_standard, &metadata_values);
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_new_definition_metadata_with_invalid_number_of_accounts_2() {
        let name = [0xca, 0xfe, 0xca, 0xfe, 0xca, 0xfe];
        let total_supply = 15u128;
        let token_standard = 0u8;
        let metadata_standard = 0u8;
        let metadata_values: Data = Data::try_from([1u8;450].to_vec()).unwrap();

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
        ];
        let _post_states = new_definition_with_metadata(&pre_states,  name, total_supply, token_standard, metadata_standard, &metadata_values);
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_new_definition_metadata_with_invalid_number_of_accounts_3() {
        let name = [0xca, 0xfe, 0xca, 0xfe, 0xca, 0xfe];
        let total_supply = 15u128;
        let token_standard = 0u8;
        let metadata_standard = 0u8;
        let metadata_values: Data = Data::try_from([1u8;450].to_vec()).unwrap();

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
            AccountWithMetadata {
                account: Account::default(),
                is_authorized: true,
                account_id: AccountId::new([4; 32]),
            },
        ];
        let _post_states = new_definition_with_metadata(&pre_states,  name, total_supply, token_standard, metadata_standard, &metadata_values);
    }

    #[should_panic(expected = "Definition target account must have default values")]
    #[test]
    fn test_call_new_definition_metadata_with_init_definition() {
        let name = [0xca, 0xfe, 0xca, 0xfe, 0xca, 0xfe];
        let total_supply = 15u128;
        let token_standard = 0u8;
        let metadata_standard = 0u8;
        let metadata_values: Data = Data::try_from([1u8;450].to_vec()).unwrap();

        let pre_states = vec![
            helper_account_constructor(AccountsEnum::DefinitionAccountAuth),
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
        let _post_states = new_definition_with_metadata(&pre_states,  name, total_supply, token_standard, metadata_standard, &metadata_values);
    }

    #[should_panic(expected ="Metadata target account must have default values")]
    #[test]
    fn test_call_new_definition_metadata_with_init_metadata() {
        let name = [0xca, 0xfe, 0xca, 0xfe, 0xca, 0xfe];
        let total_supply = 15u128;
        let token_standard = 0u8;
        let metadata_standard = 0u8;
        let metadata_values: Data = Data::try_from([1u8;450].to_vec()).unwrap();

        let pre_states = vec![
            AccountWithMetadata {
                account: Account::default(),
                is_authorized: true,
                account_id: AccountId::new([1; 32]),
            },
            helper_account_constructor(AccountsEnum::HoldingSameDefMint), //TODO: change to a metadata account
            AccountWithMetadata {
                account: Account::default(),
                is_authorized: true,
                account_id: AccountId::new([3; 32]),
            },
        ];
        let _post_states = new_definition_with_metadata(&pre_states,  name, total_supply, token_standard, metadata_standard, &metadata_values);
    }

    #[should_panic(expected ="Holding target account must have default values")]
    #[test]
    fn test_call_new_definition_metadata_with_init_holding() {
        let name = [0xca, 0xfe, 0xca, 0xfe, 0xca, 0xfe];
        let total_supply = 15u128;
        let token_standard = 0u8;
        let metadata_standard = 0u8;
        let metadata_values: Data = Data::try_from([1u8;450].to_vec()).unwrap();

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
            helper_account_constructor(AccountsEnum::HoldingSameDefMint),
        ];
        let _post_states = new_definition_with_metadata(&pre_states,  name, total_supply, token_standard, metadata_standard, &metadata_values);
    }

    #[should_panic(expected ="Metadata values data should be 450 bytes")]
    #[test]
    fn test_call_new_definition_metadata_with_too_short_metadata_length() {
        let name = [0xca, 0xfe, 0xca, 0xfe, 0xca, 0xfe];
        let total_supply = 15u128;
        let token_standard = 0u8;
        let metadata_standard = 0u8;
        let metadata_values: Data = Data::try_from([1u8;449].to_vec()).unwrap();

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
        let _post_states = new_definition_with_metadata(&pre_states,  name, total_supply, token_standard, metadata_standard, &metadata_values);
    }

    #[should_panic(expected ="Metadata values data should be 450 bytes")]
    #[test]
    fn test_call_new_definition_metadata_with_too_long_metadata_length() {
        let name = [0xca, 0xfe, 0xca, 0xfe, 0xca, 0xfe];
        let total_supply = 15u128;
        let token_standard = 0u8;
        let metadata_standard = 0u8;
        let metadata_values: Data = Data::try_from([1u8;451].to_vec()).unwrap();

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
        let _post_states = new_definition_with_metadata(&pre_states,  name, total_supply, token_standard, metadata_standard, &metadata_values);
    }

}

/*
    pre_states: &[AccountWithMetadata],
    name: [u8; 6],
    total_supply: u128,
    token_standard: TokenStandardEnum,
    metadata_standard: MetadataStandardEnum,
    metadata_values: &Data,

*/