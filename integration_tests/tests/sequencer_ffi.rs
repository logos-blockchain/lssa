//! End-to-end demo of the sequencer self-join flow.

#![expect(
    clippy::tests_outside_test_module,
    reason = "Integration tests live at crate root and don't care about these lints"
)]

use std::{ffi::CString, fs::File, io::Write, time::Duration};

use anyhow::{Context as _, Result};
use integration_tests::{account_balance, get_account, new_account};
use lee::{AccountId, PrivateKey, PublicKey, program::Program};
use log::info;
use logos_blockchain_key_management_system_service::keys::{Ed25519Key, Ed25519PublicKey};
use logos_blockchain_zone_sdk::{
    CommonHttpClient,
    adapter::{Node, NodeHttpClient},
};
use sequencer_core::config::GenesisAction;
use sequencer_service_rpc::RpcClient;
use test_fixtures::{
    MultiZoneTestContextBuilder, TestContext, ZoneTestContextBuilder,
    config::{
        MultiNodeTestContextConfig, SequencerPartialConfig, UrlProtocol, addr_to_url,
        bedrock_channel_id,
    },
    setup::SequencerSetup,
};
use wallet::AccountIdentity;

#[path = "sequencer_ffi_helpers/mod.rs"]
mod sequencer_ffi_helpers;

/// Comfortably above `system_accounts::DEFAULT_MINIMUM_SEQUENCER_STAKE`.
const FUNDING_BALANCE: u128 = 2 * system_accounts::DEFAULT_MINIMUM_SEQUENCER_STAKE;

/// Bedrock signing key of the sequencer that stakes its way in.
const JOINER_SIGNING_KEY: [u8; 32] = [0x42; 32];

/// Short block cadence for the demo.
fn fast_blocks() -> SequencerPartialConfig {
    SequencerPartialConfig {
        block_create_timeout: Duration::from_secs(5),
        ..SequencerPartialConfig::default()
    }
}

#[test]
fn seq_ffi_test_1() -> Result<()> {
    let demo_sequencer_key = Ed25519Key::from_bytes(&JOINER_SIGNING_KEY).public_key();
    let demo_stake_key = sequencer_stake_core::SequencerKey::new(demo_sequencer_key.to_bytes())
        .expect("a Bedrock key is a valid Ed25519 public key");

    let funding_private_key = PrivateKey::new_os_random();
    let funding_id = AccountId::from(&PublicKey::new_from_private_key(&funding_private_key));

    let mut ctx = MultiZoneTestContextBuilder::default()
        .with_zone(
            ZoneTestContextBuilder::new(MultiNodeTestContextConfig::default())
                .with_sequencer_partial_config(fast_blocks())
                .with_genesis(vec![GenesisAction::SupplyAccount {
                    account_id: funding_id,
                    balance: FUNDING_BALANCE,
                }]),
        )
        .build_blocking()
        .context("Failed to build test context")?;

    // Import the funding key directly; it's not one of the wallet's default accounts.
    ctx.ctx_mut()
        .wallet_mut()
        .storage_mut()
        .key_chain_mut()
        .add_imported_public_account(funding_private_key);

    info!("Waiting for the genesis supply to land on the funding account");
    ctx.block_on(|ctx| {
        poll_until("genesis supply to land", 30, || async {
            Ok(account_balance(ctx, funding_id).await? == FUNDING_BALANCE)
        })
    })?;
    info!("Funded demo account {funding_id} with {FUNDING_BALANCE} native balance");

    let ownership_id = ctx.block_on_mut(|ctx| async {
        new_account(ctx, false, None)
            .await
            .context("Failed to create a fresh stake ownership account")
    })?;
    info!("Fresh stake ownership account: {ownership_id}");

    let funds_id = system_accounts::stake_funds_account_id(&ownership_id);

    let mover_instruction_data =
        Program::serialize_instruction(authenticated_transfer_core::Instruction::Transfer {
            amount: FUNDING_BALANCE,
        })
        .context("Failed to serialize mover instruction")?;
    let stake_instruction_data =
        Program::serialize_instruction(sequencer_stake_core::Instruction::Stake {
            sequencer_key: demo_stake_key,
            amount: FUNDING_BALANCE,
            mover_account_id: programs::authenticated_transfer().id().into(),
            mover_instruction_data,
        })
        .context("Failed to serialize Stake instruction")?;

    info!(
        "Submitting Stake transaction for sequencer key {}",
        hex::encode(demo_sequencer_key.to_bytes())
    );
    let config_id = system_accounts::sequencer_stake_config_account_id();
    ctx.block_on(|ctx| async {
        ctx.wallet()
            .send_pub_tx(
                vec![
                    AccountIdentity::Public(funding_id),
                    AccountIdentity::Public(ownership_id),
                    AccountIdentity::PublicNoSign(funds_id),
                    AccountIdentity::PublicNoSign(config_id),
                ],
                stake_instruction_data,
                programs::sequencer_stake().id().into(),
            )
            .await
            .map_err(|err| anyhow::anyhow!("Failed to submit Stake transaction: {err:?}"))
    })?;

    info!("Waiting for the Stake transaction's block to land");

    ctx.block_on(|ctx| {
        poll_until("stake to take ownership", 30, || async {
            Ok(get_account(ctx, ownership_id).await?.program_owner
                == programs::sequencer_stake().id().into())
        })
    })?;

    let ownership_account = ctx.block_on(|ctx| async {
        get_account(ctx, ownership_id)
            .await
            .context("Failed to read the stake ownership account")
    })?;
    assert_eq!(
        ownership_account.program_owner,
        programs::sequencer_stake().id().into(),
        "ownership account should now be owned by sequencer_stake"
    );
    let staked_balance = ctx.block_on(|ctx| account_balance(ctx, funds_id))?;
    assert_eq!(
        staked_balance, FUNDING_BALANCE,
        "the funds PDA should hold the staked balance"
    );
    let record = sequencer_stake_core::StakeRecord::from_bytes(ownership_account.data.as_ref())
        .context("ownership account data did not decode as a StakeRecord")?;
    assert_eq!(record.sequencer_key, demo_stake_key);
    info!(
        "Ownership account confirmed: {staked_balance} staked for sequencer key {}",
        hex::encode(record.sequencer_key)
    );

    let bedrock_url = addr_to_url(UrlProtocol::Http, ctx.ctx().bedrock_addr())
        .context("Failed to build the Bedrock node URL")?;
    let node = NodeHttpClient::new(CommonHttpClient::new(None), bedrock_url);

    // The committee-config update is a separate tx from the block's own
    // publish, so it may land a moment later — poll a few times before failing.
    let mut channel_state = None;

    info!("=============================== WAIT START");

    for _ in 0..50 {
        let state = ctx
            .runtime()
            .block_on(node.channel_state(bedrock_channel_id()))
            .context("Failed to read Bedrock channel state")?
            .context("Bedrock channel does not exist")?;

        info!(
            "================================  THERE IS {} ACCREDITED KEYS",
            state.accredited_keys.len()
        );

        if state
            .accredited_keys
            .iter()
            .any(|key: &Ed25519PublicKey| *key == demo_sequencer_key)
        {
            channel_state = Some(state);
            break;
        }
        std::thread::sleep(Duration::from_secs(3));

        info!("=============================== WAIT STEP");
    }
    let channel_state = channel_state.context(
        "demo sequencer key should have been discovered and accredited after the Stake
    transaction",
    )?;
    info!(
        "Bedrock channel now accredits {} key(s), including the demo sequencer key — self-join
    complete",
        channel_state.accredited_keys.len()
    );

    info!(
        "================================================= ACTUALLY IMPORTANT THINGS HAPPEN HERE"
    );

    // Only now start a node behind the key, against a channel that already has a chain.
    let setuper = SequencerSetup::new(fast_blocks(), ctx.ctx().bedrock_addr())
        .with_channel_id(bedrock_channel_id())
        .with_bedrock_signing_key(JOINER_SIGNING_KEY)
        .joining_existing_channel();

    let temp_sequencer_dir =
        tempfile::tempdir().context("Failed to create temp dir for sequencer home")?;

    let config = setuper.prepare_sequencers_config(temp_sequencer_dir.path().to_owned())?;

    // Put sequecner config at temp sequencer dir
    let config_json = serde_json::to_vec(&config)?;
    let config_path = temp_sequencer_dir.path().join("config.json");
    let mut file = File::create(config_path.as_path())?;
    file.write_all(&config_json)?;
    file.flush()?;

    let config_c_string = CString::new(config_path.to_str().unwrap())?;

    let raw_config_path = config_c_string.into_raw();

    let res = unsafe { sequencer_ffi_helpers::start_sequencer(std::ptr::null(), raw_config_path) };

    if res.error.is_error() {
        anyhow::bail!("Sequecner FFI error {:?}", res.error);
    }

    let sequencer_ffi = unsafe { std::ptr::read(res.value) };

    let joined_at = ctx.block_on(|ctx| ctx.sequencer_client().get_last_block_id())?;
    sequencer_ffi_helpers::wait_for_sequencer_ffi_block(&sequencer_ffi, joined_at)?;
    info!("Joining sequencer synced to block {joined_at}");

    // // A tip past `joined_at` under the demo key is a block this node built.
    // poll_until("the joining sequencer to build a block on its turn", 180, {
    //     let node = &node;
    //     let ctx = &ctx;
    //     move || async move {
    //         let Some(state) = node.channel_state(bedrock_channel_id()).await? else {
    //             return Ok(false);
    //         };
    //         let turn = state
    //             .accredited_keys
    //             .get(usize::from(state.tip_sequencer))
    //             .copied();
    //         Ok(turn == Some(demo_sequencer_key)
    //             && ctx.sequencer_client().get_last_block_id().await? > joined_at)
    //     }
    // })
    // .await?;
    // info!("Joining sequencer produced a block on its round-robin turn");

    // // Both nodes agree, block for block, over everything they share.
    // let leader_client = ctx.sequencer_client();
    // let common = leader_client
    //     .get_last_block_id()
    //     .await?
    //     .min(joiner_client.get_last_block_id().await?);
    // for id in 1..=common {
    //     let leader_block = leader_client
    //         .get_block(id)
    //         .await?
    //         .with_context(|| format!("Leader is missing block {id}"))?;
    //     let joiner_block = joiner_client
    //         .get_block(id)
    //         .await?
    //         .with_context(|| format!("Joining sequencer is missing block {id}"))?;
    //     anyhow::ensure!(
    //         leader_block.header.hash == joiner_block.header.hash,
    //         "Chain divergence at block {id}: leader {:?} vs joiner {:?}",
    //         leader_block.header.hash,
    //         joiner_block.header.hash
    //     );
    // }
    // info!("Leader and joining sequencer agree on all {common} shared blocks");

    // No need for unstaking in general case

    // unsafe {
    //     sequencer_ffi_helpers::stop_sequencer(&raw mut sequencer_ffi);
    // }

    Ok(())
}

/// The `sequencer_stake` config entry for `sequencer_key`, or `None` if the key
/// has nothing at stake.
async fn _stake_entry(
    ctx: &TestContext,
    config_id: AccountId,
    sequencer_key: sequencer_stake_core::SequencerKey,
) -> Result<Option<sequencer_stake_core::SequencerEntry>> {
    let config_account = get_account(ctx, config_id)
        .await
        .context("Failed to read the sequencer_stake config account")?;
    let config =
        sequencer_stake_core::SequencerStakeConfig::from_bytes(config_account.data.as_ref())
            .context("config account data did not decode as a SequencerStakeConfig")?;
    Ok(config.entries.get(&sequencer_key).copied())
}

/// Polls `check` once a second, up to `max_attempts` times, replacing fixed
/// block-wait sleeps: the accelerated devnet crosses an epoch boundary every
/// ~100 slots, so every second of wall-clock spent sleeping increases the
/// chance of straddling one.
async fn poll_until<F, Fut>(what: &str, max_attempts: u32, mut check: F) -> Result<()>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<bool>>,
{
    for _ in 0..max_attempts {
        if check().await.unwrap_or(false) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    anyhow::bail!("timed out waiting for {what}")
}
