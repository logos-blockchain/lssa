use anyhow::Result;
use clap::Subcommand;

use crate::{
    PrivacyPreservingAccount, WalletCore,
    cli::{SubcommandReturnValue, WalletSubcommand},
    helperfunctions::parse_addr_with_privacy_prefix,
    program_facades::amm::AMM,
};

/// Represents generic CLI subcommand for a wallet working with amm program
#[derive(Subcommand, Debug, Clone)]
pub enum AmmProgramAgnosticSubcommand {
    /// Produce a new token
    ///
    /// user_holding_a and user_holding_b must be owned.
    New {
        /// amm_pool - valid 32 byte base58 string with privacy prefix
        #[arg(long)]
        amm_pool: String,
        /// vault_holding_a - valid 32 byte base58 string with privacy prefix
        #[arg(long)]
        vault_holding_a: String,
        /// vault_holding_b - valid 32 byte base58 string with privacy prefix
        #[arg(long)]
        vault_holding_b: String,
        /// pool_lp - valid 32 byte base58 string with privacy prefix
        #[arg(long)]
        pool_lp: String,
        /// user_holding_a - valid 32 byte base58 string with privacy prefix
        #[arg(long)]
        user_holding_a: String,
        /// user_holding_b - valid 32 byte base58 string with privacy prefix
        #[arg(long)]
        user_holding_b: String,
        /// user_holding_lp - valid 32 byte base58 string with privacy prefix
        #[arg(long)]
        user_holding_lp: String,
        #[arg(long)]
        balance_a: u128,
        #[arg(long)]
        balance_b: u128,
    },
    /// Swap with variable privacy
    ///
    /// The account associated with swapping token must be owned
    Swap {
        /// amm_pool - valid 32 byte base58 string with privacy prefix
        #[arg(long)]
        amm_pool: String,
        /// vault_holding_1 - valid 32 byte base58 string with privacy prefix
        #[arg(long)]
        vault_holding_1: String,
        /// vault_holding_2 - valid 32 byte base58 string with privacy prefix
        #[arg(long)]
        vault_holding_2: String,
        /// user_holding_a - valid 32 byte base58 string with privacy prefix
        #[arg(long)]
        user_holding_a: String,
        /// user_holding_b - valid 32 byte base58 string with privacy prefix
        #[arg(long)]
        user_holding_b: String,
        #[arg(long)]
        amount_in: u128,
        #[arg(long)]
        min_amount_out: u128,
        /// token_definition - valid 32 byte base58 string WITHOUT privacy prefix
        #[arg(long)]
        token_definition: String,
    },
    /// Add liquidity with variable privacy
    ///
    /// user_holding_a and user_holding_b must be owned.
    AddLiquidity {
        /// amm_pool - valid 32 byte base58 string with privacy prefix
        #[arg(long)]
        amm_pool: String,
        /// vault_holding_a - valid 32 byte base58 string with privacy prefix
        #[arg(long)]
        vault_holding_a: String,
        /// vault_holding_b - valid 32 byte base58 string with privacy prefix
        #[arg(long)]
        vault_holding_b: String,
        /// pool_lp - valid 32 byte base58 string with privacy prefix
        #[arg(long)]
        pool_lp: String,
        /// user_holding_a - valid 32 byte base58 string with privacy prefix
        #[arg(long)]
        user_holding_a: String,
        /// user_holding_b - valid 32 byte base58 string with privacy prefix
        #[arg(long)]
        user_holding_b: String,
        /// user_holding_lp - valid 32 byte base58 string with privacy prefix
        #[arg(long)]
        user_holding_lp: String,
        #[arg(long)]
        min_amount_lp: u128,
        #[arg(long)]
        max_amount_a: u128,
        #[arg(long)]
        max_amount_b: u128,
    },
    /// Remove liquidity with variable privacy
    ///
    /// user_holding_lp must be owned.
    RemoveLiquidity {
        /// amm_pool - valid 32 byte base58 string with privacy prefix
        #[arg(long)]
        amm_pool: String,
        /// vault_holding_a - valid 32 byte base58 string with privacy prefix
        #[arg(long)]
        vault_holding_a: String,
        /// vault_holding_b - valid 32 byte base58 string with privacy prefix
        #[arg(long)]
        vault_holding_b: String,
        /// pool_lp - valid 32 byte base58 string with privacy prefix
        #[arg(long)]
        pool_lp: String,
        /// user_holding_a - valid 32 byte base58 string with privacy prefix
        #[arg(long)]
        user_holding_a: String,
        /// user_holding_b - valid 32 byte base58 string with privacy prefix
        #[arg(long)]
        user_holding_b: String,
        /// user_holding_lp - valid 32 byte base58 string with privacy prefix
        #[arg(long)]
        user_holding_lp: String,
        #[arg(long)]
        balance_lp: u128,
        #[arg(long)]
        max_amount_a: u128,
        #[arg(long)]
        max_amount_b: u128,
    },
}

impl WalletSubcommand for AmmProgramAgnosticSubcommand {
    async fn handle_subcommand(
        self,
        wallet_core: &mut WalletCore,
    ) -> Result<SubcommandReturnValue> {
        match self {
            AmmProgramAgnosticSubcommand::New {
                amm_pool,
                vault_holding_a,
                vault_holding_b,
                pool_lp,
                user_holding_a,
                user_holding_b,
                user_holding_lp,
                balance_a,
                balance_b,
            } => {
                let amm_pool = PrivacyPreservingAccount::parse_with_privacy(
                    parse_addr_with_privacy_prefix(&amm_pool)?,
                )?;
                let vault_holding_a = PrivacyPreservingAccount::parse_with_privacy(
                    parse_addr_with_privacy_prefix(&vault_holding_a)?,
                )?;
                let vault_holding_b = PrivacyPreservingAccount::parse_with_privacy(
                    parse_addr_with_privacy_prefix(&vault_holding_b)?,
                )?;
                let pool_lp = PrivacyPreservingAccount::parse_with_privacy(
                    parse_addr_with_privacy_prefix(&pool_lp)?,
                )?;
                let user_holding_a = PrivacyPreservingAccount::parse_with_privacy(
                    parse_addr_with_privacy_prefix(&user_holding_a)?,
                )?;
                let user_holding_b = PrivacyPreservingAccount::parse_with_privacy(
                    parse_addr_with_privacy_prefix(&user_holding_b)?,
                )?;
                let user_holding_lp = PrivacyPreservingAccount::parse_with_privacy(
                    parse_addr_with_privacy_prefix(&user_holding_lp)?,
                )?;

                let is_public_tx = [
                    &amm_pool,
                    &vault_holding_a,
                    &vault_holding_b,
                    &pool_lp,
                    &user_holding_a,
                    &user_holding_b,
                    &user_holding_lp,
                ]
                .into_iter()
                .all(|acc| acc.is_public());

                if is_public_tx {
                    AMM(wallet_core)
                        .send_new_amm_definition(
                            amm_pool,
                            vault_holding_a,
                            vault_holding_b,
                            pool_lp,
                            user_holding_a,
                            user_holding_b,
                            user_holding_lp,
                            balance_a,
                            balance_b,
                        )
                        .await?;
                    Ok(SubcommandReturnValue::Empty)
                } else {
                    AMM(wallet_core)
                        .send_new_amm_definition_privacy_preserving(
                            amm_pool,
                            vault_holding_a,
                            vault_holding_b,
                            pool_lp,
                            user_holding_a,
                            user_holding_b,
                            user_holding_lp,
                            balance_a,
                            balance_b,
                        )
                        .await?;
                    // ToDo: change into correct return value
                    Ok(SubcommandReturnValue::Empty)
                }
            }
            AmmProgramAgnosticSubcommand::Swap {
                amm_pool,
                vault_holding_1,
                vault_holding_2,
                user_holding_a,
                user_holding_b,
                amount_in,
                min_amount_out,
                token_definition,
            } => {
                let amm_pool = PrivacyPreservingAccount::parse_with_privacy(
                    parse_addr_with_privacy_prefix(&amm_pool)?,
                )?;
                let vault_holding_1 = PrivacyPreservingAccount::parse_with_privacy(
                    parse_addr_with_privacy_prefix(&vault_holding_1)?,
                )?;
                let vault_holding_2 = PrivacyPreservingAccount::parse_with_privacy(
                    parse_addr_with_privacy_prefix(&vault_holding_2)?,
                )?;
                let user_holding_a = PrivacyPreservingAccount::parse_with_privacy(
                    parse_addr_with_privacy_prefix(&user_holding_a)?,
                )?;
                let user_holding_b = PrivacyPreservingAccount::parse_with_privacy(
                    parse_addr_with_privacy_prefix(&user_holding_b)?,
                )?;

                let is_public_tx = [
                    &amm_pool,
                    &vault_holding_1,
                    &vault_holding_2,
                    &user_holding_a,
                    &user_holding_b,
                ]
                .into_iter()
                .all(|acc| acc.is_public());

                if is_public_tx {
                    AMM(wallet_core)
                        .send_swap(
                            amm_pool,
                            vault_holding_1,
                            vault_holding_2,
                            user_holding_a,
                            user_holding_b,
                            amount_in,
                            min_amount_out,
                            token_definition.parse()?,
                        )
                        .await?;
                    Ok(SubcommandReturnValue::Empty)
                } else {
                    AMM(wallet_core)
                        .send_swap_privacy_preserving(
                            amm_pool,
                            vault_holding_1,
                            vault_holding_2,
                            user_holding_a,
                            user_holding_b,
                            amount_in,
                            min_amount_out,
                            token_definition.parse()?,
                        )
                        .await?;
                    // ToDo: change into correct return value
                    Ok(SubcommandReturnValue::Empty)
                }
            }
            AmmProgramAgnosticSubcommand::AddLiquidity {
                amm_pool,
                vault_holding_a,
                vault_holding_b,
                pool_lp,
                user_holding_a,
                user_holding_b,
                user_holding_lp,
                min_amount_lp,
                max_amount_a,
                max_amount_b,
            } => {
                let amm_pool = PrivacyPreservingAccount::parse_with_privacy(
                    parse_addr_with_privacy_prefix(&amm_pool)?,
                )?;
                let vault_holding_a = PrivacyPreservingAccount::parse_with_privacy(
                    parse_addr_with_privacy_prefix(&vault_holding_a)?,
                )?;
                let vault_holding_b = PrivacyPreservingAccount::parse_with_privacy(
                    parse_addr_with_privacy_prefix(&vault_holding_b)?,
                )?;
                let pool_lp = PrivacyPreservingAccount::parse_with_privacy(
                    parse_addr_with_privacy_prefix(&pool_lp)?,
                )?;
                let user_holding_a = PrivacyPreservingAccount::parse_with_privacy(
                    parse_addr_with_privacy_prefix(&user_holding_a)?,
                )?;
                let user_holding_b = PrivacyPreservingAccount::parse_with_privacy(
                    parse_addr_with_privacy_prefix(&user_holding_b)?,
                )?;
                let user_holding_lp = PrivacyPreservingAccount::parse_with_privacy(
                    parse_addr_with_privacy_prefix(&user_holding_lp)?,
                )?;

                let is_public_tx = [
                    &amm_pool,
                    &vault_holding_a,
                    &vault_holding_b,
                    &pool_lp,
                    &user_holding_a,
                    &user_holding_b,
                    &user_holding_lp,
                ]
                .into_iter()
                .all(|acc| acc.is_public());

                if is_public_tx {
                    AMM(wallet_core)
                        .send_add_liq(
                            amm_pool,
                            vault_holding_a,
                            vault_holding_b,
                            pool_lp,
                            user_holding_a,
                            user_holding_b,
                            user_holding_lp,
                            min_amount_lp,
                            max_amount_a,
                            max_amount_b,
                        )
                        .await?;
                    Ok(SubcommandReturnValue::Empty)
                } else {
                    AMM(wallet_core)
                        .send_add_liq_privacy_preserving(
                            amm_pool,
                            vault_holding_a,
                            vault_holding_b,
                            pool_lp,
                            user_holding_a,
                            user_holding_b,
                            user_holding_lp,
                            min_amount_lp,
                            max_amount_a,
                            max_amount_b,
                        )
                        .await?;
                    // ToDo: change into correct return value
                    Ok(SubcommandReturnValue::Empty)
                }
            }
            AmmProgramAgnosticSubcommand::RemoveLiquidity {
                amm_pool,
                vault_holding_a,
                vault_holding_b,
                pool_lp,
                user_holding_a,
                user_holding_b,
                user_holding_lp,
                balance_lp,
                max_amount_a,
                max_amount_b,
            } => {
                let amm_pool = PrivacyPreservingAccount::parse_with_privacy(
                    parse_addr_with_privacy_prefix(&amm_pool)?,
                )?;
                let vault_holding_a = PrivacyPreservingAccount::parse_with_privacy(
                    parse_addr_with_privacy_prefix(&vault_holding_a)?,
                )?;
                let vault_holding_b = PrivacyPreservingAccount::parse_with_privacy(
                    parse_addr_with_privacy_prefix(&vault_holding_b)?,
                )?;
                let pool_lp = PrivacyPreservingAccount::parse_with_privacy(
                    parse_addr_with_privacy_prefix(&pool_lp)?,
                )?;
                let user_holding_a = PrivacyPreservingAccount::parse_with_privacy(
                    parse_addr_with_privacy_prefix(&user_holding_a)?,
                )?;
                let user_holding_b = PrivacyPreservingAccount::parse_with_privacy(
                    parse_addr_with_privacy_prefix(&user_holding_b)?,
                )?;
                let user_holding_lp = PrivacyPreservingAccount::parse_with_privacy(
                    parse_addr_with_privacy_prefix(&user_holding_lp)?,
                )?;

                let is_public_tx = [
                    &amm_pool,
                    &vault_holding_a,
                    &vault_holding_b,
                    &pool_lp,
                    &user_holding_a,
                    &user_holding_b,
                    &user_holding_lp,
                ]
                .into_iter()
                .all(|acc| acc.is_public());

                if is_public_tx {
                    AMM(wallet_core)
                        .send_remove_liq(
                            amm_pool,
                            vault_holding_a,
                            vault_holding_b,
                            pool_lp,
                            user_holding_a,
                            user_holding_b,
                            user_holding_lp,
                            balance_lp,
                            max_amount_a,
                            max_amount_b,
                        )
                        .await?;
                    Ok(SubcommandReturnValue::Empty)
                } else {
                    AMM(wallet_core)
                        .send_remove_liq_privacy_preserving(
                            amm_pool,
                            vault_holding_a,
                            vault_holding_b,
                            pool_lp,
                            user_holding_a,
                            user_holding_b,
                            user_holding_lp,
                            balance_lp,
                            max_amount_a,
                            max_amount_b,
                        )
                        .await?;
                    // ToDo: change into correct return value
                    Ok(SubcommandReturnValue::Empty)
                }
            }
        }
    }
}
