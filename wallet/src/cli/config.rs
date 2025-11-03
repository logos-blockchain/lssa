use anyhow::Result;
use clap::Subcommand;

use crate::{SubcommandReturnValue, WalletCore, cli::WalletSubcommand};

///Represents generic config CLI subcommand
#[derive(Subcommand, Debug, Clone)]
pub enum ConfigSubcommand {
    /// Command to explicitly setup config and storage
    ///
    /// Does nothing in case if both already present
    Setup {},
    /// Getter of config fields
    Get { key: String },
    /// Setter of config fields
    Set { key: String, value: String },
}

impl WalletSubcommand for ConfigSubcommand {
    async fn handle_subcommand(
        self,
        wallet_core: &mut WalletCore,
    ) -> Result<SubcommandReturnValue> {
        match self {
            ConfigSubcommand::Setup {} => {
                let path = wallet_core.store_persistent_data().await?;

                println!("Stored persistent accounts at {path:#?}");
            }
            ConfigSubcommand::Get { key } => match key.as_str() {
                "all" => {
                    let config_str =
                        serde_json::to_string_pretty(&wallet_core.storage.wallet_config)?;

                    println!("{config_str}");
                }
                "override_rust_log" => {
                    if let Some(value) = &wallet_core.storage.wallet_config.override_rust_log {
                        println!("{value}");
                    } else {
                        println!("Not set");
                    }
                }
                "sequencer_addr" => {
                    println!("{}", wallet_core.storage.wallet_config.sequencer_addr);
                }
                "seq_poll_timeout_millis" => {
                    println!(
                        "{}",
                        wallet_core.storage.wallet_config.seq_poll_timeout_millis
                    );
                }
                "seq_poll_max_blocks" => {
                    println!("{}", wallet_core.storage.wallet_config.seq_poll_max_blocks);
                }
                "seq_poll_max_retries" => {
                    println!("{}", wallet_core.storage.wallet_config.seq_poll_max_retries);
                }
                "seq_poll_retry_delay_millis" => {
                    println!(
                        "{}",
                        wallet_core
                            .storage
                            .wallet_config
                            .seq_poll_retry_delay_millis
                    );
                }
                "initial_accounts" => {
                    println!("{:#?}", wallet_core.storage.wallet_config.initial_accounts);
                }
                _ => {
                    println!("Unknown field");
                }
            },
            ConfigSubcommand::Set { key, value } => {
                match key.as_str() {
                    "override_rust_log" => {
                        wallet_core.storage.wallet_config.override_rust_log = Some(value);
                    }
                    "sequencer_addr" => {
                        wallet_core.storage.wallet_config.sequencer_addr = value;
                    }
                    "seq_poll_timeout_millis" => {
                        wallet_core.storage.wallet_config.seq_poll_timeout_millis =
                            value.parse()?;
                    }
                    "seq_poll_max_blocks" => {
                        wallet_core.storage.wallet_config.seq_poll_max_blocks = value.parse()?;
                    }
                    "seq_poll_max_retries" => {
                        wallet_core.storage.wallet_config.seq_poll_max_retries = value.parse()?;
                    }
                    "seq_poll_retry_delay_millis" => {
                        wallet_core
                            .storage
                            .wallet_config
                            .seq_poll_retry_delay_millis = value.parse()?;
                    }
                    "initial_accounts" => {
                        anyhow::bail!("Setting this field from wallet is not supported");
                    }
                    _ => {
                        anyhow::bail!("Unknown field");
                    }
                }

                let path = wallet_core.store_config_changes().await?;

                println!("Stored changed config at {path:#?}");
            }
        }

        Ok(SubcommandReturnValue::Empty)
    }
}
