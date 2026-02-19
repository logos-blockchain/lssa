use nssa_core::{
    account::{Account, AccountWithMetadata},
    program::{AccountPostState, ProgramInput, read_nssa_inputs, write_nssa_outputs},
};

// This is a small marketplace-like program where a user can list items and someone can buy them
// Currently the "item" listed has the following structure:
// [0..16]   price: u128
// [16..48]  seller: [u8;32]
// [48..64]  unique_string > making the item "unique"
// When a sale happens, the program:
// - transfers funds to the program's escrow
// - sets the item's new owner
// - allows a withdrawal for the original seller from the escrow

// When listing an item, your provide:
// new item account (uninitialized)
// your own account_id via account
// item price
// the unique_string making the item "unique" - 16 bytes
fn list_item(
    item_account: AccountWithMetadata,
    seller: AccountWithMetadata,
    price: u128,
    unique_string: [u8; 16],
) -> AccountPostState {
    use risc0_zkvm::sha::{Impl, Sha256};

    if !seller.is_authorized {
        panic!("Seller must be authorized");
    }

    // Must be a new account provided
    if item_account.account != Account::default() {
        panic!("Item already initialized");
    }

    let mut acc = item_account.account.clone();
    let mut data: Vec<u8> = vec![0u8; 64]; // 16 price + 32 seller_id_hash + 16 unique_string

    // Create hash of seller account_id for privacy-preserving
    let seller_bytes = seller.account_id.as_ref();
    let seller_id_hashed = Impl::hash_bytes(&seller_bytes);

    // Set item data
    data[0..16].copy_from_slice(&price.to_le_bytes()); // set price
    data[16..48].copy_from_slice(&seller_id_hashed.as_bytes()); // set seller hash
    data[48..64].copy_from_slice(&unique_string); // set unique string

    acc.data = data.try_into().unwrap();
    AccountPostState::new_claimed(acc)
}

fn buy_item(
    buyer: AccountWithMetadata,
    item_account: AccountWithMetadata,
    escrow: AccountWithMetadata,
) -> Vec<AccountPostState> {
    use risc0_zkvm::sha::{Impl, Sha256};
    if !buyer.is_authorized {
        panic!("Buyer must be authorized");
    }

    // Get the data from the item account
    let mut item_acc: Account = item_account.account.clone();
    let mut item_data: Vec<u8> = item_acc.data.into_inner();

    // extract price data
    let mut price_bytes = [0u8; 16];
    price_bytes.copy_from_slice(&item_data[0..16]);
    let price = u128::from_le_bytes(price_bytes);

    // Check if buyer has enough balance
    if buyer.account.balance < price {
        panic!(
            "Insufficient funds, need {} but have {}",
            price, buyer.account.balance
        );
    }

    // Extract seller hash from item (16..48)
    let seller_hash_bytes: &[u8] = &item_data[16..48];

    // Setup per-sale escrow
    let mut escrow_post = escrow.account.clone();
    let mut escrow_data: Vec<u8> = vec![0u8; 32]; // 32 seller_id_hash 
    escrow_data.copy_from_slice(seller_hash_bytes);
    escrow_post.data = escrow_data.try_into().unwrap();

    // Transfer funds from buyer to escrow
    let mut buyer_post = buyer.account.clone();
    buyer_post.balance -= price;
    escrow_post.balance += price;

    // Set new item owner
    let buyer_id_hashed = Impl::hash_bytes(&buyer.account_id.as_ref());
    item_data[16..48].copy_from_slice(&buyer_id_hashed.as_bytes());
    item_acc.data = item_data.try_into().unwrap();

    vec![
        AccountPostState::new(buyer_post),
        AccountPostState::new(escrow_post),
        AccountPostState::new(item_acc),
    ]
}

// Withdrawing allows the correct seller (by hash of acc_id) to withdraw everything to their balance.
fn withdraw_from_escrow(
    seller: AccountWithMetadata,
    escrow_account: AccountWithMetadata,
) -> Vec<AccountPostState> {
    use risc0_zkvm::sha::{Impl, Sha256};
    if !seller.is_authorized {
        panic!("Seller must authorize withdrawal");
    }

    // Hash the seller's account ID
    let seller_hash = Impl::hash_bytes(seller.account_id.as_ref());

    // Get the seller hash stored in escrow
    let escrow_acc = escrow_account.account.clone();
    let escrow_data: Vec<u8> = escrow_acc.data.into_inner();

    // escrow_data[0..32] is assumed to hold the hashed seller ID
    if seller_hash.as_bytes() != &escrow_data[0..32] {
        panic!("Unauthorized: seller hash does not match escrow");
    }

    let mut escrow_post = escrow_account.account.clone();
    let mut seller_post = seller.account.clone();

    // Withdraw all in escrow - this is a simplification as escrows are created per-sale
    seller_post.balance += escrow_post.balance;
    escrow_post.balance = 0;

    vec![
        AccountPostState::new(escrow_post),
        AccountPostState::new(seller_post),
    ]
}

type MarketplaceInstruction = (u8, Vec<u8>);

// Selector constants
const LIST_ITEM: u8 = 0;
const BUY_ITEM: u8 = 1;
const WITHDRAW_ESCROW: u8 = 2;

fn main() {
    let (
        ProgramInput {
            pre_states,
            instruction,
        },
        instruction_words,
    ) = read_nssa_inputs::<MarketplaceInstruction>();

    let (selector, data) = instruction;

    let post_states = match (pre_states.as_slice(), selector) {
        // List item: expects [seller, item] accounts
        ([seller, item], LIST_ITEM) => {
            // data should contain: price (16 bytes) + unique_string (16 bytes)
            if data.len() != 32 {
                panic!("Invalid instruction data length for LIST_ITEM");
            }
            let price = u128::from_le_bytes(data[0..16].try_into().unwrap());
            let mut unique_string = [0u8; 16];
            unique_string.copy_from_slice(&data[16..32]);
            vec![list_item(
                item.clone(),
                seller.clone(),
                price,
                unique_string,
            )]
        }

        // Buy item: expects [buyer, item, escrow]
        ([buyer, item, escrow], BUY_ITEM) => buy_item(buyer.clone(), item.clone(), escrow.clone()),

        // Withdraw escrow: expects [seller, escrow]
        ([seller, escrow], WITHDRAW_ESCROW) => withdraw_from_escrow(seller.clone(), escrow.clone()),

        _ => panic!("Transaction response: {:#?}", instruction_words),
    };

    write_nssa_outputs(instruction_words, pre_states, post_states);
}
