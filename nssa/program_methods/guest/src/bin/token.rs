use nssa_core::{
    account::{Account, AccountId, AccountWithMetadata, Data},
    program::{ProgramInput, read_nssa_inputs, write_nssa_outputs},
};

/// [type (1) || amount (16) || name (6)]
type Instruction = [u8; 23];

const TOKEN_DEFINITION_TYPE: u8 = 0;
const TOKEN_DEFINITION_SIZE: usize = 23;

const TOKEN_HOLDING_TYPE: u8 = 1;
const TOKEN_HOLDING_SIZE: usize = 49;

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
        let mut bytes = [0; TOKEN_DEFINITION_SIZE];
        bytes[0] = self.account_type;
        bytes[1..7].copy_from_slice(&self.name);
        bytes[7..].copy_from_slice(&self.total_supply.to_le_bytes());
        bytes.into()
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
        if data.len() != TOKEN_HOLDING_SIZE && data[0] != TOKEN_HOLDING_TYPE {
            None
        } else {
            let account_type = data[0];
            let definition_id = AccountId::new(data[1..33].try_into().unwrap());
            let balance = u128::from_le_bytes(data[33..].try_into().unwrap());
            Some(Self {
                definition_id,
                balance,
                account_type,
            })
        }
    }

    fn into_data(self) -> Data {
        let mut bytes = [0; TOKEN_HOLDING_SIZE];
        bytes[0] = self.account_type;
        bytes[1..33].copy_from_slice(&self.definition_id.to_bytes());
        bytes[33..].copy_from_slice(&self.balance.to_le_bytes());
        bytes.into()
    }
}

fn transfer(pre_states: Vec<AccountWithMetadata>, balance_to_move: u128) {
    let [sender, recipient] = match pre_states.try_into() {
        Ok(array) => array,
        Err(_) => return,
    };

    let mut sender_holding = TokenHolding::parse(&sender.account.data).unwrap();
    let mut recipient_holding = if recipient.account == Account::default() {
        TokenHolding::new(&sender_holding.definition_id)
    } else {
        TokenHolding::parse(&recipient.account.data).unwrap()
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
    recipient_holding.balance += balance_to_move;

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

    write_nssa_outputs(vec![sender, recipient], vec![sender_post, recipient_post]);
}

fn new_definition(pre_states: Vec<AccountWithMetadata>, name: [u8; 6], total_supply: u128) {
    let [definition_target_account, holding_target_account] = match pre_states.try_into() {
        Ok(array) => array,
        Err(_) => return,
    };

    if definition_target_account.account != Account::default() {
        panic!("Definition target account must have default values.");
    }

    if holding_target_account.account != Account::default() {
        panic!("Holding target account must have default values.");
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

    write_nssa_outputs(
        vec![definition_target_account, holding_target_account],
        vec![definition_target_account_post, holding_target_account_post],
    );
}

fn main() {
    let ProgramInput {
        pre_states,
        instruction,
    } = read_nssa_inputs::<Instruction>();

    match instruction[0] {
        0 => {
            let total_supply = u128::from_le_bytes(instruction[1..17].try_into().unwrap());
            let name: [u8; 6] = instruction[17..].try_into().unwrap();
            assert_ne!(name, [0; 6]);
            new_definition(pre_states, name, total_supply)
        }
        1 => {
            let balance_to_move = u128::from_le_bytes(instruction[1..17].try_into().unwrap());
            let name: [u8; 6] = instruction[17..].try_into().unwrap();
            assert_eq!(name, [0; 6]);
            transfer(pre_states, balance_to_move)
        }
        _ => panic!("Invalid instruction"),
    };
}
