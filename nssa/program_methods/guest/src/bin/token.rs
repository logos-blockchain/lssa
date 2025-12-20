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
// 6. New token definition with metadata.
//    Arguments to this function are:
//      * Three **default** accounts: [definition_account, metadata_account. holding_account].
//        The first default account will be initialized with the token definition account values. The second account
//        will be initialized to a token metadata account for the new token definition. The third account will be
//        initialized to a token holding account for the new token, holding the entire total supply.
//      * An instruction data of 474-bytes, indicating the token name, total supply, token standard, metadata standard
//        and metadata_values (uri and creators).
//        the following layout:
//        [0x05 || total_supply (little-endian 16 bytes) || name (6 bytes) || token_standard || metadata_standard || metadata_values]
//        The name cannot be equal to [0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
// 7. Print NFT copy from Master NFT
//    Arguments to this function are:
//      * Two accounts: [master_nft, printed_account (default)].
//      * Authorization required: master_nft
//      * An dummy byte string of length 23, with the following layout
//        [0x06 || 0x00 || 0x00 || 0x00 || ... || 0x00 || 0x00].
const TOKEN_STANDARD_FUNGIBLE_TOKEN: u8 = 0;
const TOKEN_STANDARD_FUNGIBLE_ASSET: u8 = 1;
const TOKEN_STANDARD_NONFUNGIBLE: u8 = 2;
const TOKEN_STANDARD_NONFUNGIBLE_PRINTABLE: u8 = 3;

const METADATA_TYPE_SIMPLE: u8 = 0;
const METADATA_TYPE_EXPANDED: u8 = 1;

const TOKEN_DEFINITION_DATA_SIZE: usize = 55;

const TOKEN_HOLDING_STANDARD: u8 = 1;
const TOKEN_HOLDING_NFT_MASTER: u8 = 2;
const TOKEN_HOLDING_NFT_PRINTED_COPY: u8 = 3;

const TOKEN_HOLDING_DATA_SIZE: usize = 49;
const CURRENT_VERSION: u8 = 1;

const TOKEN_METADATA_DATA_SIZE: usize = 463;

fn is_token_standard_valid(standard: u8) -> bool {
    if standard == TOKEN_STANDARD_FUNGIBLE_TOKEN {
        true
    } else if standard == TOKEN_STANDARD_FUNGIBLE_ASSET {
        true
    } else if standard == TOKEN_STANDARD_NONFUNGIBLE {
        true
    } else if standard == TOKEN_STANDARD_NONFUNGIBLE_PRINTABLE {
        true
    } else {
        false
    }
}

fn is_metadata_type_valid(standard: u8) -> bool {
    if standard == METADATA_TYPE_SIMPLE {
        true
    } else if standard == METADATA_TYPE_EXPANDED {
        true
    } else {
        false
    }
}

fn is_token_holding_type_valid(standard: u8) -> bool {
    if standard == TOKEN_HOLDING_STANDARD {
        true
    } else if standard == TOKEN_HOLDING_NFT_MASTER {
        true
    } else if standard == TOKEN_HOLDING_NFT_PRINTED_COPY {
        true
    } else {
        false
    }
}

struct TokenDefinition {
    account_type: u8,
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

            if account_type == //NFTs must have supply 1
                TOKEN_STANDARD_NONFUNGIBLE
                && total_supply != 1
            {
                None
            } else if account_type == //Fungible Tokens do not have metadata.
                TOKEN_STANDARD_FUNGIBLE_TOKEN
                && metadata_id != AccountId::new([0; 32])
            {
                None
            } else {
                this
            }
        }
    }
}

struct TokenHolding {
    account_type: u8,
    definition_id: AccountId,
    balance: u128,
}

impl TokenHolding {
    fn new(definition_id: &AccountId) -> Self {
        Self {
            account_type: TOKEN_HOLDING_STANDARD,
            definition_id: definition_id.clone(),
            balance: 0,
        }
    }

    fn parse(data: &Data) -> Option<Self> {
        let data = Vec::<u8>::from(data.clone());

        if data.len() != TOKEN_HOLDING_DATA_SIZE {
            return None;
        }

        // Check account_type
        if !is_token_holding_type_valid(data[0]) {
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
        if !is_token_holding_type_valid(self.account_type) {
            panic!("Invalid Token Holding type");
        }

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
    account_type: u8,
    version: u8,
    definition_id: AccountId,
    uri: [u8; 200],
    creators: [u8; 250],
    primary_sale_date: u64, //BlockId
}

impl TokenMetadata {
    fn into_data(self) -> Data {
        if !is_metadata_type_valid(self.account_type) {
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

        if data.len() != TOKEN_METADATA_DATA_SIZE || !is_metadata_type_valid(data[0]) {
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

    let sender_holding = TokenHolding::parse(&sender.account.data).expect("Invalid sender data");

    let recipient_holding = if recipient.account == Account::default() {
        TokenHolding::new(&sender_holding.definition_id)
    } else {
        TokenHolding::parse(&recipient.account.data).expect("Invalid recipient data")
    };

    if sender_holding.definition_id != recipient_holding.definition_id {
        panic!("Sender and recipient definition id mismatch");
    }

    let (sender_holding, recipient_holding) =
        if sender_holding.account_type != TOKEN_HOLDING_NFT_MASTER {
            standard_transfer(sender_holding, recipient_holding, balance_to_move)
        } else {
            nft_master_transfer(sender_holding, recipient_holding, balance_to_move)
        };

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

fn standard_transfer(
    sender_holding: TokenHolding,
    recipient_holding: TokenHolding,
    balance_to_move: u128,
) -> (TokenHolding, TokenHolding) {
    let mut sender_holding = sender_holding;
    let mut recipient_holding = recipient_holding;

    if sender_holding.balance < balance_to_move {
        panic!("Insufficient balance");
    }

    sender_holding.balance = sender_holding
        .balance
        .checked_sub(balance_to_move)
        .expect("Checked above");
    recipient_holding.balance = recipient_holding
        .balance
        .checked_add(balance_to_move)
        .expect("Recipient balance overflow");

    recipient_holding.account_type = sender_holding.account_type;

    (sender_holding, recipient_holding)
}

fn nft_master_transfer(
    sender_holding: TokenHolding,
    recipient_holding: TokenHolding,
    balance_to_move: u128,
) -> (TokenHolding, TokenHolding) {
    let mut sender_holding = sender_holding;
    let mut recipient_holding = recipient_holding;

    if recipient_holding.balance != 0 {
        panic!("Invalid balance in recipient account for NFT transfer");
    }

    if sender_holding.balance != balance_to_move {
        panic!("Invalid balance for NFT Master transfer");
    }

    sender_holding.balance = 0;
    recipient_holding.balance = balance_to_move;
    recipient_holding.account_type = sender_holding.account_type;

    (sender_holding, recipient_holding)
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
        account_type: TOKEN_STANDARD_FUNGIBLE_TOKEN,
        name,
        total_supply,
        metadata_id: AccountId::new([0; 32]),
    };

    let token_holding = TokenHolding {
        account_type: TOKEN_HOLDING_STANDARD,
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

    if !is_token_standard_valid(token_standard) {
        panic!("Invalid Token Standard provided");
    }

    if !is_metadata_type_valid(metadata_standard) {
        panic!("Invalid Metadata Standadard provided");
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
        account_type: TOKEN_HOLDING_STANDARD,
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
    if token_standard == TOKEN_STANDARD_NONFUNGIBLE && total_supply != 1 {
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
    if account_type == TOKEN_STANDARD_NONFUNGIBLE {
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

fn print_nft(pre_states: &[AccountWithMetadata]) -> Vec<AccountPostState> {
    if pre_states.len() != 2 {
        panic!("Invalid number of accounts");
    }

    let master_account = &pre_states[0];
    let printed_account = &pre_states[1];

    if !master_account.is_authorized {
        panic!("Master NFT Account must be authorized");
    }

    if printed_account.account != Account::default() {
        panic!("Printed Account must be uninitialized");
    }

    let mut master_account_data =
        TokenHolding::parse(&master_account.account.data).expect("Invalid Token Holding data");

    if master_account_data.account_type != TOKEN_HOLDING_NFT_MASTER {
        panic!("Invalid Token Holding provided as NFT Master Account");
    }

    if master_account_data.balance < 2 {
        panic!("Insufficient balance to print another NFT copy");
    }

    let definition_id = master_account_data.definition_id.clone();

    let post_master_account = {
        let mut this = master_account.account.clone();
        master_account_data.balance -= 1;
        this.data = master_account_data.into_data();
        AccountPostState::new(this)
    };

    let post_printed_account = {
        let mut this = printed_account.account.clone();

        let printed_data = TokenHolding {
            account_type: TOKEN_HOLDING_NFT_PRINTED_COPY,
            definition_id,
            balance: 1,
        };

        this.data = TokenHolding::into_data(printed_data);

        AccountPostState::new_claimed(this)
    };

    vec![post_master_account, post_printed_account]
}

type Instruction = Vec<u8>;

fn main() {
    let (
        ProgramInput {
            pre_states,
            instruction,
        },
        instruction_words,
    ) = read_nssa_inputs::<Instruction>();

    let post_states = match instruction[0] {
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
            new_definition(&pre_states, name, total_supply)
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
            transfer(&pre_states, balance_to_move)
        }
        2 => {
            // Initialize account
            if instruction[1..] != [0; 22] {
                panic!("Invalid instruction for initialize account");
            }
            initialize_account(&pre_states)
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
            burn(&pre_states, balance_to_burn)
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
            mint_additional_supply(&pre_states, balance_to_mint)
        }
        5 => {
            if instruction.len() != 474 {
                panic!("Invalid instruction length")
            }

            // Parse instruction
            let total_supply = u128::from_le_bytes(
                instruction[1..17]
                    .try_into()
                    .expect("Total supply must be 16 bytes little-endian"),
            );
            let name = instruction[17..23]
                .try_into()
                .expect("Name must be 6 bytes long");
            assert_ne!(name, [0; 6]);
            let token_standard = instruction[23];
            let metadata_standard = instruction[24];
            let metadata_values: Data =
                Data::try_from(instruction[25..474].to_vec()).expect("Invalid metadata");

            // Execute
            new_definition_with_metadata(
                &pre_states,
                name,
                total_supply,
                token_standard,
                metadata_standard,
                &metadata_values,
            )
        }
        6 => {
            if instruction.len() != 23 {
                panic!("Invalid instruction length");
            }

            // Initialize account
            if instruction[1..] != [0; 22] {
                panic!("Invalid instruction for initialize account");
            }

            print_nft(&pre_states)
        }
        _ => panic!("Invalid instruction"),
    };

    write_nssa_outputs(instruction_words, pre_states, post_states);
}

#[cfg(test)]
mod tests {
    use nssa_core::account::{Account, AccountId, AccountWithMetadata, Data};

    use crate::{
        TOKEN_DEFINITION_DATA_SIZE, TOKEN_HOLDING_DATA_SIZE, TOKEN_HOLDING_NFT_MASTER,
        TOKEN_HOLDING_NFT_PRINTED_COPY, TOKEN_HOLDING_STANDARD, TOKEN_STANDARD_FUNGIBLE_ASSET,
        TOKEN_STANDARD_FUNGIBLE_TOKEN, TOKEN_STANDARD_NONFUNGIBLE, TokenDefinition, TokenHolding,
        burn, initialize_account, mint_additional_supply, new_definition,
        new_definition_with_metadata, print_nft, transfer,
    };

    struct BalanceForTests;
    struct IdForTests;

    struct AccountForTests;

    impl AccountForTests {
        fn definition_account_auth() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: [5u32; 8],
                    balance: 0u128,
                    data: TokenDefinition::into_data(TokenDefinition {
                        account_type: TOKEN_STANDARD_FUNGIBLE_TOKEN,
                        name: [2; 6],
                        total_supply: BalanceForTests::init_supply(),
                        metadata_id: AccountId::new([0; 32]),
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::pool_definition_id(),
            }
        }

        fn definition_account_without_auth() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: [5u32; 8],
                    balance: 0u128,
                    data: TokenDefinition::into_data(TokenDefinition {
                        account_type: TOKEN_STANDARD_FUNGIBLE_TOKEN,
                        name: [2; 6],
                        total_supply: BalanceForTests::init_supply(),
                        metadata_id: AccountId::new([0; 32]),
                    }),
                    nonce: 0,
                },
                is_authorized: false,
                account_id: IdForTests::pool_definition_id(),
            }
        }

        fn holding_different_definition() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: [5u32; 8],
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: TOKEN_HOLDING_STANDARD,
                        definition_id: IdForTests::pool_definition_id_diff(),
                        balance: BalanceForTests::holding_balance(),
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::holding_id(),
            }
        }

        fn holding_same_definition_with_authorization() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: [5u32; 8],
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: TOKEN_HOLDING_STANDARD,
                        definition_id: IdForTests::pool_definition_id(),
                        balance: BalanceForTests::holding_balance(),
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::holding_id(),
            }
        }

        fn holding_same_definition_without_authorization() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: [5u32; 8],
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: TOKEN_HOLDING_STANDARD,
                        definition_id: IdForTests::pool_definition_id(),
                        balance: BalanceForTests::holding_balance(),
                    }),
                    nonce: 0,
                },
                is_authorized: false,
                account_id: IdForTests::holding_id(),
            }
        }

        fn holding_same_definition_without_authorization_overflow() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: [5u32; 8],
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: TOKEN_HOLDING_STANDARD,
                        definition_id: IdForTests::pool_definition_id(),
                        balance: BalanceForTests::init_supply(),
                    }),
                    nonce: 0,
                },
                is_authorized: false,
                account_id: IdForTests::holding_id(),
            }
        }

        fn definition_account_post_burn() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: [5u32; 8],
                    balance: 0u128,
                    data: TokenDefinition::into_data(TokenDefinition {
                        account_type: TOKEN_STANDARD_FUNGIBLE_TOKEN,
                        name: [2; 6],
                        total_supply: BalanceForTests::init_supply_burned(),
                        metadata_id: AccountId::new([0; 32]),
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::pool_definition_id(),
            }
        }

        fn holding_account_post_burn() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: [5u32; 8],
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: TOKEN_HOLDING_STANDARD,
                        definition_id: IdForTests::pool_definition_id(),
                        balance: BalanceForTests::holding_balance_burned(),
                    }),
                    nonce: 0,
                },
                is_authorized: false,
                account_id: IdForTests::holding_id(),
            }
        }

        fn holding_account_uninit() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account::default(),
                is_authorized: false,
                account_id: IdForTests::holding_id_2(),
            }
        }

        fn init_mint() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: [0u32; 8],
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: TOKEN_HOLDING_STANDARD,
                        definition_id: IdForTests::pool_definition_id(),
                        balance: BalanceForTests::mint_success(),
                    }),
                    nonce: 0,
                },
                is_authorized: false,
                account_id: IdForTests::holding_id(),
            }
        }

        fn holding_account_same_definition_mint() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: [5u32; 8],
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: TOKEN_HOLDING_STANDARD,
                        definition_id: IdForTests::pool_definition_id(),
                        balance: BalanceForTests::holding_balance_mint(),
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::pool_definition_id(),
            }
        }
        fn definition_account_mint() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: [5u32; 8],
                    balance: 0u128,
                    data: TokenDefinition::into_data(TokenDefinition {
                        account_type: TOKEN_STANDARD_FUNGIBLE_TOKEN,
                        name: [2; 6],
                        total_supply: BalanceForTests::init_supply_mint(),
                        metadata_id: AccountId::new([0; 32]),
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::pool_definition_id(),
            }
        }
        fn holding_same_definition_with_authorization_and_large_balance() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: [5u32; 8],
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: TOKEN_HOLDING_STANDARD,
                        definition_id: IdForTests::pool_definition_id(),
                        balance: BalanceForTests::mint_overflow(),
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::pool_definition_id(),
            }
        }
        fn definition_account_with_authorization_nonfungible() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: [5u32; 8],
                    balance: 0u128,
                    data: TokenDefinition::into_data(TokenDefinition {
                        account_type: TOKEN_STANDARD_NONFUNGIBLE,
                        name: [2; 6],
                        total_supply: 1,
                        metadata_id: AccountId::new([0; 32]),
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::pool_definition_id(),
            }
        }
        fn definition_account_uninit() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account::default(),
                is_authorized: false,
                account_id: IdForTests::pool_definition_id(),
            }
        }

        fn holding_account_init() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: [5u32; 8],
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: TOKEN_HOLDING_STANDARD,
                        definition_id: IdForTests::pool_definition_id(),
                        balance: BalanceForTests::init_supply(),
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::holding_id(),
            }
        }
        fn definition_account_unclaimed() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: [0u32; 8],
                    balance: 0u128,
                    data: TokenDefinition::into_data(TokenDefinition {
                        account_type: TOKEN_STANDARD_FUNGIBLE_TOKEN,
                        name: [2; 6],
                        total_supply: BalanceForTests::init_supply(),
                        metadata_id: AccountId::new([0; 32]),
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::pool_definition_id(),
            }
        }
        fn holding_account_unclaimed() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: [0u32; 8],
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: TOKEN_HOLDING_STANDARD,
                        definition_id: IdForTests::pool_definition_id(),
                        balance: BalanceForTests::init_supply(),
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::holding_id(),
            }
        }

        fn holding_account2_init() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: [5u32; 8],
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: TOKEN_HOLDING_STANDARD,
                        definition_id: IdForTests::pool_definition_id(),
                        balance: BalanceForTests::init_supply(),
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::holding_id_2(),
            }
        }

        fn holding_account2_init_post_transfer() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: [5u32; 8],
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: TOKEN_HOLDING_STANDARD,
                        definition_id: IdForTests::pool_definition_id(),
                        balance: BalanceForTests::recipient_post_transfer(),
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::holding_id_2(),
            }
        }

        fn holding_account_init_post_transfer() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: [5u32; 8],
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: TOKEN_HOLDING_STANDARD,
                        definition_id: IdForTests::pool_definition_id(),
                        balance: BalanceForTests::sender_post_transfer(),
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::holding_id(),
            }
        }

        fn holding_account2_uninit_post_transfer() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: [0u32; 8],
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: TOKEN_HOLDING_STANDARD,
                        definition_id: IdForTests::pool_definition_id(),
                        balance: BalanceForTests::recipient_uninit_post_transfer(),
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::holding_id_2(),
            }
        }

        fn holding_account_master_nft() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: [5u32; 8],
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: TOKEN_HOLDING_NFT_MASTER,
                        definition_id: IdForTests::pool_definition_id(),
                        balance: BalanceForTests::printable_copies(),
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::holding_id(),
            }
        }

        fn holding_account_master_nft_insufficient_balance() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: [5u32; 8],
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: TOKEN_HOLDING_NFT_MASTER,
                        definition_id: IdForTests::pool_definition_id(),
                        balance: 1,
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::holding_id(),
            }
        }

        fn holding_account_master_nft_after_print() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: [5u32; 8],
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: TOKEN_HOLDING_NFT_MASTER,
                        definition_id: IdForTests::pool_definition_id(),
                        balance: BalanceForTests::printable_copies() - 1,
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::holding_id(),
            }
        }

        fn holding_account_printed_nft() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: [0u32; 8],
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: TOKEN_HOLDING_NFT_PRINTED_COPY,
                        definition_id: IdForTests::pool_definition_id(),
                        balance: 1,
                    }),
                    nonce: 0,
                },
                is_authorized: false,
                account_id: IdForTests::holding_id(),
            }
        }

        fn holding_account_with_master_nft_transferred_to() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: [0u32; 8],
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: TOKEN_HOLDING_NFT_MASTER,
                        definition_id: IdForTests::pool_definition_id(),
                        balance: BalanceForTests::printable_copies(),
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::holding_id_2(),
            }
        }

        fn holding_account_master_nft_post_transfer() -> AccountWithMetadata {
            AccountWithMetadata {
                account: Account {
                    program_owner: [5u32; 8],
                    balance: 0u128,
                    data: TokenHolding::into_data(TokenHolding {
                        account_type: TOKEN_HOLDING_NFT_MASTER,
                        definition_id: IdForTests::pool_definition_id(),
                        balance: 0,
                    }),
                    nonce: 0,
                },
                is_authorized: true,
                account_id: IdForTests::holding_id(),
            }
        }
    }

    impl BalanceForTests {
        fn init_supply() -> u128 {
            100_000
        }

        fn holding_balance() -> u128 {
            1_000
        }

        fn init_supply_burned() -> u128 {
            99_500
        }

        fn holding_balance_burned() -> u128 {
            500
        }

        fn burn_success() -> u128 {
            500
        }

        fn burn_insufficient() -> u128 {
            1_500
        }

        fn mint_success() -> u128 {
            50_000
        }

        fn holding_balance_mint() -> u128 {
            51_000
        }

        fn mint_overflow() -> u128 {
            (2 as u128).pow(128) - 40_000
        }

        fn init_supply_mint() -> u128 {
            150_000
        }

        fn sender_post_transfer() -> u128 {
            95_000
        }

        fn recipient_post_transfer() -> u128 {
            105_000
        }

        fn recipient_uninit_post_transfer() -> u128 {
            5_000
        }

        fn transfer_amount() -> u128 {
            5_000
        }

        fn printable_copies() -> u128 {
            10
        }
    }

    impl IdForTests {
        fn pool_definition_id() -> AccountId {
            AccountId::new([15; 32])
        }

        fn pool_definition_id_diff() -> AccountId {
            AccountId::new([16; 32])
        }

        fn holding_id() -> AccountId {
            AccountId::new([17; 32])
        }

        fn holding_id_2() -> AccountId {
            AccountId::new([42; 32])
        }

        fn metadata_id() -> AccountId {
            AccountId::new([31; 32])
        }
    }

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
            AccountForTests::definition_account_uninit(),
            AccountForTests::holding_account_uninit(),
        ];

        let post_states = new_definition(&pre_states, [2u8; 6], BalanceForTests::init_supply());

        let [definition_account, holding_account] = post_states.try_into().ok().unwrap();
        assert!(
            *definition_account.account()
                == AccountForTests::definition_account_unclaimed().account
        );

        assert!(
            *holding_account.account()
                == AccountForTests::holding_account_unclaimed().account
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
        let invalid_type = TOKEN_HOLDING_STANDARD ^ 1;
        let pre_states = vec![
            AccountWithMetadata {
                account: Account {
                    // First byte should be `TOKEN_HOLDING_STANDARD` for token holding accounts
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

    #[should_panic(expected = "Invalid sender data")]
    #[test]
    fn test_transfer_invalid_data_size_should_fail_1() {
        let pre_states = vec![
            AccountWithMetadata {
                account: Account {
                    // Data must be of exact length `TOKEN_HOLDING_DATA_SIZE`
                    data: Data::try_from(vec![1; TOKEN_HOLDING_DATA_SIZE - 1]).unwrap(),
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
                    data: Data::try_from(vec![1; TOKEN_HOLDING_DATA_SIZE - 1]).unwrap(),
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
            AccountForTests::holding_same_definition_with_authorization(),
            AccountForTests::holding_different_definition(),
        ];
        let _post_states = transfer(&pre_states, 10);
    }

    #[should_panic(expected = "Insufficient balance")]
    #[test]
    fn test_transfer_with_insufficient_balance_should_fail() {
        let pre_states = vec![
            AccountForTests::holding_same_definition_with_authorization(),
            AccountForTests::holding_account_same_definition_mint(),
        ];
        // Attempt to transfer 38 tokens
        let _post_states = transfer(&pre_states, BalanceForTests::burn_insufficient());
    }

    #[should_panic(expected = "Sender authorization is missing")]
    #[test]
    fn test_transfer_without_sender_authorization_should_fail() {
        let mut def_data = Vec::<u8>::new();
        def_data.extend_from_slice(&[1; TOKEN_DEFINITION_DATA_SIZE - 16]);
        def_data.extend_from_slice(&u128::to_le_bytes(37));

        let pre_states = vec![
            AccountWithMetadata {
                account: Account {
                    // Account with balance 37
                    data: Data::try_from(def_data).unwrap(),
                    ..Account::default()
                },
                is_authorized: false,
                account_id: AccountId::new([1; 32]),
            },
            AccountWithMetadata {
                account: Account {
                    data: Data::try_from(vec![1; TOKEN_HOLDING_DATA_SIZE - 1]).unwrap(),
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
            AccountForTests::holding_account_init(),
            AccountForTests::holding_account2_init(),
        ];
        let post_states = transfer(&pre_states, BalanceForTests::transfer_amount());
        let [sender_post, recipient_post] = post_states.try_into().ok().unwrap();

        assert!(
            *sender_post.account()
                == AccountForTests::holding_account_init_post_transfer().account
        );
        assert!(
            *recipient_post.account()
                == AccountForTests::holding_account2_init_post_transfer()
                    .account
        );
    }

    #[should_panic(expected = "Invalid balance for NFT Master transfer")]
    #[test]
    fn test_transfer_with_master_nft_invalid_balance() {
        let pre_states = vec![
            AccountForTests::holding_account_master_nft(),
            AccountForTests::holding_account_uninit(),
        ];
        let post_states = transfer(&pre_states, BalanceForTests::transfer_amount());
    }

    #[should_panic(expected = "Invalid balance in recipient account for NFT transfer")]
    #[test]
    fn test_transfer_with_master_nft_invalid_recipient_balance() {
        let pre_states = vec![
            AccountForTests::holding_account_master_nft(),
            AccountForTests::holding_account_with_master_nft_transferred_to(),
        ];
        let _post_states = transfer(&pre_states, BalanceForTests::printable_copies());
    }

    #[test]
    fn test_transfer_with_master_nft_success() {
        let pre_states = vec![
            AccountForTests::holding_account_master_nft(),
            AccountForTests::holding_account_uninit(),
        ];
        let post_states = transfer(&pre_states, BalanceForTests::printable_copies());
        let [sender_post, recipient_post] = post_states.try_into().ok().unwrap();

        assert!(
            *sender_post.account()
                == AccountForTests::holding_account_master_nft_post_transfer()
                    .account
        );
        assert!(
            *recipient_post.account()
                == AccountForTests::holding_account_with_master_nft_transferred_to()
                    .account
        );
    }

    #[test]
    fn test_token_initialize_account_succeeds() {
        let pre_states = vec![
            AccountForTests::holding_account_init(),
            AccountForTests::holding_account2_init(),
        ];
        let post_states = transfer(&pre_states, BalanceForTests::transfer_amount());
        let [sender_post, recipient_post] = post_states.try_into().ok().unwrap();

        assert!(
            *sender_post.account()
                == AccountForTests::holding_account_init_post_transfer()
                .account
        );
        assert!(
            *recipient_post.account()
                == AccountForTests::holding_account2_init_post_transfer()
                    .account
        );
    }

    #[test]
    #[should_panic(expected = "Invalid number of accounts")]
    fn test_burn_invalid_number_of_accounts() {
        let pre_states = vec![
            AccountForTests::definition_account_auth(),
        ];
        let _post_states = burn(&pre_states, BalanceForTests::burn_success());
    }

    #[test]
    #[should_panic(expected = "Mismatch Token Definition and Token Holding")]
    fn test_burn_mismatch_def() {
        let pre_states = vec![
            AccountForTests::definition_account_auth(),
            AccountForTests::holding_different_definition(),
        ];
        let _post_states = burn(&pre_states, BalanceForTests::burn_success());
    }

    #[test]
    #[should_panic(expected = "Authorization is missing")]
    fn test_burn_missing_authorization() {
        let pre_states = vec![
            AccountForTests::definition_account_auth(),
            AccountForTests::holding_same_definition_without_authorization(),
        ];
        let _post_states = burn(&pre_states, BalanceForTests::burn_success());
    }

    #[test]
    #[should_panic(expected = "Insufficient balance to burn")]
    fn test_burn_insufficient_balance() {
        let pre_states = vec![
            AccountForTests::definition_account_auth(),
            AccountForTests::holding_same_definition_with_authorization(),
        ];
        let _post_states = burn(&pre_states, BalanceForTests::burn_insufficient());
    }

    #[test]
    #[should_panic(expected = "Total supply underflow")]
    fn test_burn_total_supply_underflow() {
        let pre_states = vec![
            AccountForTests::definition_account_auth(),
            AccountForTests::holding_same_definition_with_authorization_and_large_balance(),
        ];
        let _post_states = burn(&pre_states, BalanceForTests::mint_overflow());
    }

    #[test]
    fn test_burn_success() {
        let pre_states = vec![
            AccountForTests::definition_account_auth(),
            AccountForTests::holding_same_definition_with_authorization(),
        ];
        let post_states = burn(&pre_states, BalanceForTests::burn_success());

        let def_post = post_states[0].clone();
        let holding_post = post_states[1].clone();

        assert!(
            *def_post.account()
                == AccountForTests::definition_account_post_burn().account
        );
        assert!(
            *holding_post.account()
                == AccountForTests::holding_account_post_burn().account
        );
    }

    #[test]
    #[should_panic(expected = "Invalid number of accounts")]
    fn test_mint_invalid_number_of_accounts_1() {
        let pre_states = vec![AccountForTests::definition_account_auth()];
        let _post_states = mint_additional_supply(&pre_states, BalanceForTests::mint_success());
    }

    #[test]
    #[should_panic(expected = "Invalid number of accounts")]
    fn test_mint_invalid_number_of_accounts_2() {
        let pre_states = vec![
            AccountForTests::definition_account_auth(),
            AccountForTests::holding_account_same_definition_mint(),
            AccountForTests::holding_same_definition_with_authorization(),
        ];
        let _post_states = mint_additional_supply(&pre_states, BalanceForTests::mint_success());
    }

    #[test]
    #[should_panic(expected = "Holding account must be valid")]
    fn test_mint_not_valid_holding_account() {
        let pre_states = vec![
            AccountForTests::definition_account_auth(),
            AccountForTests::definition_account_without_auth(),
        ];
        let _post_states = mint_additional_supply(&pre_states, BalanceForTests::mint_success());
    }

    #[test]
    #[should_panic(expected = "Definition account must be valid")]
    fn test_mint_not_valid_definition_account() {
        let pre_states = vec![
            AccountForTests::holding_same_definition_with_authorization(),
            AccountForTests::holding_same_definition_without_authorization(),
        ];
        let _post_states = mint_additional_supply(&pre_states, BalanceForTests::mint_success());
    }

    #[test]
    #[should_panic(expected = "Definition authorization is missing")]
    fn test_mint_missing_authorization() {
        let pre_states = vec![
            AccountForTests::definition_account_without_auth(),
            AccountForTests::holding_same_definition_without_authorization(),
        ];
        let _post_states = mint_additional_supply(&pre_states, BalanceForTests::mint_success());
    }

    #[test]
    #[should_panic(expected = "Mismatch Token Definition and Token Holding")]
    fn test_mint_mismatched_token_definition() {
        let pre_states = vec![
            AccountForTests::definition_account_auth(),
            AccountForTests::holding_different_definition(),
        ];
        let _post_states = mint_additional_supply(&pre_states, BalanceForTests::mint_success());
    }

    #[test]
    fn test_mint_success() {
        let pre_states = vec![
            AccountForTests::definition_account_auth(),
            AccountForTests::holding_same_definition_without_authorization(),
        ];
        let post_states = mint_additional_supply(&pre_states, BalanceForTests::mint_success());

        let def_post = post_states[0].clone();
        let holding_post = post_states[1].clone();

        assert!(
            *def_post.account()
                == AccountForTests::definition_account_mint().account
        );
        assert!(
            *holding_post.account()
                == AccountForTests::holding_account_same_definition_mint().account
        );
    }

    #[test]
    fn test_mint_uninit_holding_success() {
        let pre_states = vec![
            AccountForTests::definition_account_auth(),
            AccountForTests::holding_account_uninit(),
        ];
        let post_states = mint_additional_supply(&pre_states, BalanceForTests::mint_success());

        let def_post = post_states[0].clone();
        let holding_post = post_states[1].clone();

        assert!(
            *def_post.account()
                == AccountForTests::definition_account_mint().account
        );
        assert!(
            *holding_post.account() == AccountForTests::init_mint().account
        );
        assert!(holding_post.requires_claim() == true);
    }

    #[test]
    #[should_panic(expected = "Total supply overflow")]
    fn test_mint_total_supply_overflow() {
        let pre_states = vec![
            AccountForTests::definition_account_auth(),
            AccountForTests::holding_same_definition_without_authorization(),
        ];
        let _post_states = mint_additional_supply(&pre_states, BalanceForTests::mint_overflow());
    }

    #[test]
    #[should_panic(expected = "New balance overflow")]
    fn test_mint_holding_account_overflow() {
        let pre_states = vec![
            AccountForTests::definition_account_auth(),
            AccountForTests::holding_same_definition_without_authorization_overflow(),
        ];
        let _post_states = mint_additional_supply(&pre_states, BalanceForTests::mint_overflow());
    }

    #[test]
    #[should_panic(
        expected = "Token Definition's standard does not permit minting additional supply"
    )]
    fn test_mint_cannot_mint_unmintable_tokens() {
        let pre_states = vec![
            AccountForTests::definition_account_with_authorization_nonfungible(),
            AccountForTests::holding_same_definition_without_authorization(),
        ];
        let _post_states = mint_additional_supply(&pre_states, BalanceForTests::mint_success());
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_new_definition_metadata_with_invalid_number_of_accounts_1() {
        let name = [0xca, 0xfe, 0xca, 0xfe, 0xca, 0xfe];
        let total_supply = 15u128;
        let token_standard = 0u8;
        let metadata_standard = 0u8;
        let metadata_values: Data = Data::try_from([1u8; 450].to_vec()).unwrap();

        let pre_states = vec![AccountWithMetadata {
            account: Account::default(),
            is_authorized: true,
            account_id: AccountId::new([1; 32]),
        }];
        let _post_states = new_definition_with_metadata(
            &pre_states,
            name,
            total_supply,
            token_standard,
            metadata_standard,
            &metadata_values,
        );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_new_definition_metadata_with_invalid_number_of_accounts_2() {
        let name = [0xca, 0xfe, 0xca, 0xfe, 0xca, 0xfe];
        let total_supply = 15u128;
        let token_standard = 0u8;
        let metadata_standard = 0u8;
        let metadata_values: Data = Data::try_from([1u8; 450].to_vec()).unwrap();

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
        let _post_states = new_definition_with_metadata(
            &pre_states,
            name,
            total_supply,
            token_standard,
            metadata_standard,
            &metadata_values,
        );
    }

    #[should_panic(expected = "Invalid number of input accounts")]
    #[test]
    fn test_call_new_definition_metadata_with_invalid_number_of_accounts_3() {
        let name = [0xca, 0xfe, 0xca, 0xfe, 0xca, 0xfe];
        let total_supply = 15u128;
        let token_standard = 0u8;
        let metadata_standard = 0u8;
        let metadata_values: Data = Data::try_from([1u8; 450].to_vec()).unwrap();

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
        let _post_states = new_definition_with_metadata(
            &pre_states,
            name,
            total_supply,
            token_standard,
            metadata_standard,
            &metadata_values,
        );
    }

    #[should_panic(expected = "Definition target account must have default values")]
    #[test]
    fn test_call_new_definition_metadata_with_init_definition() {
        let name = [0xca, 0xfe, 0xca, 0xfe, 0xca, 0xfe];
        let total_supply = 15u128;
        let token_standard = 0u8;
        let metadata_standard = 0u8;
        let metadata_values: Data = Data::try_from([1u8; 450].to_vec()).unwrap();

        let pre_states = vec![
            AccountForTests::definition_account_auth(),
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
        let _post_states = new_definition_with_metadata(
            &pre_states,
            name,
            total_supply,
            token_standard,
            metadata_standard,
            &metadata_values,
        );
    }

    #[should_panic(expected = "Metadata target account must have default values")]
    #[test]
    fn test_call_new_definition_metadata_with_init_metadata() {
        let name = [0xca, 0xfe, 0xca, 0xfe, 0xca, 0xfe];
        let total_supply = 15u128;
        let token_standard = 0u8;
        let metadata_standard = 0u8;
        let metadata_values: Data = Data::try_from([1u8; 450].to_vec()).unwrap();

        let pre_states = vec![
            AccountWithMetadata {
                account: Account::default(),
                is_authorized: true,
                account_id: AccountId::new([1; 32]),
            },
            AccountForTests::holding_account_same_definition_mint(),
            AccountWithMetadata {
                account: Account::default(),
                is_authorized: true,
                account_id: AccountId::new([3; 32]),
            },
        ];
        let _post_states = new_definition_with_metadata(
            &pre_states,
            name,
            total_supply,
            token_standard,
            metadata_standard,
            &metadata_values,
        );
    }

    #[should_panic(expected = "Holding target account must have default values")]
    #[test]
    fn test_call_new_definition_metadata_with_init_holding() {
        let name = [0xca, 0xfe, 0xca, 0xfe, 0xca, 0xfe];
        let total_supply = 15u128;
        let token_standard = 0u8;
        let metadata_standard = 0u8;
        let metadata_values: Data = Data::try_from([1u8; 450].to_vec()).unwrap();

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
            AccountForTests::holding_account_same_definition_mint(),
        ];
        let _post_states = new_definition_with_metadata(
            &pre_states,
            name,
            total_supply,
            token_standard,
            metadata_standard,
            &metadata_values,
        );
    }

    #[should_panic(expected = "Metadata values data should be 450 bytes")]
    #[test]
    fn test_call_new_definition_metadata_with_too_short_metadata_length() {
        let name = [0xca, 0xfe, 0xca, 0xfe, 0xca, 0xfe];
        let total_supply = 15u128;
        let token_standard = 0u8;
        let metadata_standard = 0u8;
        let metadata_values: Data = Data::try_from([1u8; 449].to_vec()).unwrap();

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
        let _post_states = new_definition_with_metadata(
            &pre_states,
            name,
            total_supply,
            token_standard,
            metadata_standard,
            &metadata_values,
        );
    }

    #[should_panic(expected = "Metadata values data should be 450 bytes")]
    #[test]
    fn test_call_new_definition_metadata_with_too_long_metadata_length() {
        let name = [0xca, 0xfe, 0xca, 0xfe, 0xca, 0xfe];
        let total_supply = 15u128;
        let token_standard = 0u8;
        let metadata_standard = 0u8;
        let metadata_values: Data = Data::try_from([1u8; 451].to_vec()).unwrap();

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
        let _post_states = new_definition_with_metadata(
            &pre_states,
            name,
            total_supply,
            token_standard,
            metadata_standard,
            &metadata_values,
        );
    }

    #[should_panic(expected = "Invalid Token Standard provided")]
    #[test]
    fn test_call_new_definition_metadata_with_invalid_token_standard() {
        let name = [0xca, 0xfe, 0xca, 0xfe, 0xca, 0xfe];
        let total_supply = 15u128;
        let token_standard = 14u8;
        let metadata_standard = 0u8;
        let metadata_values: Data = Data::try_from([1u8; 450].to_vec()).unwrap();

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
        let _post_states = new_definition_with_metadata(
            &pre_states,
            name,
            total_supply,
            token_standard,
            metadata_standard,
            &metadata_values,
        );
    }

    #[should_panic(expected = "Invalid Metadata Standadard provided")]
    #[test]
    fn test_call_new_definition_metadata_with_invalid_metadata_standard() {
        let name = [0xca, 0xfe, 0xca, 0xfe, 0xca, 0xfe];
        let total_supply = 15u128;
        let token_standard = 0u8;
        let metadata_standard = 14u8;
        let metadata_values: Data = Data::try_from([1u8; 450].to_vec()).unwrap();

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
        let _post_states = new_definition_with_metadata(
            &pre_states,
            name,
            total_supply,
            token_standard,
            metadata_standard,
            &metadata_values,
        );
    }

    #[should_panic(expected = "Invalid total supply for the specified token supply")]
    #[test]
    fn test_call_new_definition_metadata_invalid_supply_for_nonfungible() {
        let name = [0xca, 0xfe, 0xca, 0xfe, 0xca, 0xfe];
        let total_supply = 15u128;
        let token_standard = TOKEN_STANDARD_NONFUNGIBLE;
        let metadata_standard = 0u8;
        let metadata_values: Data = Data::try_from([1u8; 450].to_vec()).unwrap();

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
        let _post_states = new_definition_with_metadata(
            &pre_states,
            name,
            total_supply,
            token_standard,
            metadata_standard,
            &metadata_values,
        );
    }

    #[should_panic(expected = "Invalid number of accounts")]
    #[test]
    fn test_print_nft_invalid_number_of_accounts_1() {
        let pre_states = vec![
            AccountForTests::holding_account_master_nft()
            ];
        let _post_states = print_nft(&pre_states);
    }

    #[should_panic(expected = "Invalid number of accounts")]
    #[test]
    fn test_print_nft_invalid_number_of_accounts_2() {
        let pre_states = vec![
            AccountForTests::holding_account_master_nft(),
            AccountForTests::definition_account_auth(),
            AccountForTests::holding_account_uninit(),
        ];
        let _post_states = print_nft(&pre_states);
    }

    #[should_panic(expected = "Master NFT Account must be authorized")]
    #[test]
    fn test_print_nft_master_account_must_be_authorized() {
        let pre_states = vec![
            AccountForTests::holding_account_uninit(),
            AccountForTests::holding_account_uninit(),
        ];
        let _post_states = print_nft(&pre_states);
    }

    #[should_panic(expected = "Printed Account must be uninitialized")]
    #[test]
    fn test_print_nft_print_account_initialized() {
        let pre_states = vec![
            AccountForTests::holding_account_master_nft(),
            AccountForTests::holding_account_init(),
        ];
        let _post_states = print_nft(&pre_states);
    }

    #[should_panic(expected = "Invalid Token Holding data")]
    #[test]
    fn test_print_nft_master_nft_invalid_token_holding() {
        let pre_states = vec![
            AccountForTests::definition_account_auth(),
            AccountForTests::holding_account_uninit(),
        ];
        let _post_states = print_nft(&pre_states);
    }

    #[should_panic(expected = "Invalid Token Holding provided as NFT Master Account")]
    #[test]
    fn test_print_nft_master_nft_not_nft_master_account() {
        let pre_states = vec![
            AccountForTests::holding_account_init(),
            AccountForTests::holding_account_uninit(),
        ];
        let _post_states = print_nft(&pre_states);
    }

    #[should_panic(expected = "Insufficient balance to print another NFT copy")]
    #[test]
    fn test_print_nft_master_nft_insufficient_balance() {
        let pre_states = vec![
            AccountForTests::holding_account_master_nft_insufficient_balance(),
            AccountForTests::holding_account_uninit(),
        ];
        let _post_states = print_nft(&pre_states);
    }

    #[test]
    fn test_print_nft_success() {
        let pre_states = vec![
            AccountForTests::holding_account_master_nft(),
            AccountForTests::holding_account_uninit(),
        ];
        let post_states = print_nft(&pre_states);

        let post_master_nft = post_states[0].account();
        let post_printed = post_states[1].account();

        assert!(
            *post_master_nft
                == AccountForTests::holding_account_master_nft_after_print().account
        );
        assert!(
            *post_printed
                == AccountForTests::holding_account_printed_nft().account
        );
    }
}
