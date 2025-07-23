use anyhow::Result;
use log::info;

use crate::chain_storage::NodeChainStore;

///Addres of public fund transfer account, as no such binary exists for zkVM
pub const PUBLIC_DEPOSIT_ID: [u8; 32] = [0; 32];

///Setups public states of default smart conracts as empty
pub async fn setup_empty_sc_states(node: &NodeChainStore) -> Result<()> {
    info!("Filling up public states of default smart contracts");

    let empty_state = vec![];

    let public_deposit_addr = hex::encode(PUBLIC_DEPOSIT_ID);
    node.block_store.put_sc_sc_state(
        &public_deposit_addr,
        empty_state.len(),
        empty_state.clone(),
    )?;
    info!("Public transfer state set");

    let mint_utxo_addr_bytes: Vec<u8> = zkvm::test_methods::MINT_UTXO_ID
        .iter()
        .flat_map(|num| num.to_le_bytes())
        .collect();
    let mint_utxo_addr = hex::encode(mint_utxo_addr_bytes);
    node.block_store
        .put_sc_sc_state(&mint_utxo_addr, empty_state.len(), empty_state.clone())?;
    info!("Mint UTXO state set");

    let single_utxo_transfer_addr_bytes: Vec<u8> = zkvm::test_methods::SEND_UTXO_ID
        .iter()
        .flat_map(|num| num.to_le_bytes())
        .collect();
    let single_utxo_transfer_addr = hex::encode(single_utxo_transfer_addr_bytes);
    node.block_store.put_sc_sc_state(
        &single_utxo_transfer_addr,
        empty_state.len(),
        empty_state.clone(),
    )?;
    info!("Single UTXO transfer state set");

    let mint_utxo_multiple_assets_addr_bytes: Vec<u8> =
        zkvm::test_methods::MINT_UTXO_MULTIPLE_ASSETS_ID
            .iter()
            .flat_map(|num| num.to_le_bytes())
            .collect();
    let mint_utxo_multiple_assets_addr = hex::encode(mint_utxo_multiple_assets_addr_bytes);
    node.block_store.put_sc_sc_state(
        &mint_utxo_multiple_assets_addr,
        empty_state.len(),
        empty_state.clone(),
    )?;
    info!("Mint UTXO multiple assets state set");

    let multiple_assets_utxo_transfer_addr_bytes: Vec<u8> =
        zkvm::test_methods::SEND_UTXO_MULTIPLE_ASSETS_ID
            .iter()
            .flat_map(|num| num.to_le_bytes())
            .collect();
    let multiple_assets_utxo_transfer_addr = hex::encode(multiple_assets_utxo_transfer_addr_bytes);
    node.block_store.put_sc_sc_state(
        &multiple_assets_utxo_transfer_addr,
        empty_state.len(),
        empty_state.clone(),
    )?;
    info!("Multiple_assets UTXO transfer state set");

    Ok(())
}
