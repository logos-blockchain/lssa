use anyhow::Result;
use clap::{CommandFactory as _, Parser as _};
use tokio::runtime::Builder;
use wallet::cli::{
    Args, OverCommand, execute_continuous_run, execute_keys_restoration, execute_setup,
    execute_subcommand,
};

pub const NUM_THREADS: usize = 2;

// TODO #169: We have sample configs for sequencer, but not for wallet
// TODO #168: Why it requires config as a directory? Maybe better to deduce directory from config
// file path?
// TODO #172: Why it requires config as env var while sequencer_runner accepts as
// argument?
// TODO #171: Running pinata doesn't give output about transaction hash and etc.
fn main() -> Result<()> {
    let runtime = Builder::new_multi_thread()
        .worker_threads(NUM_THREADS)
        .enable_all()
        .build()
        .unwrap();

    let args = Args::parse();

    env_logger::init();

    runtime.block_on(async move {
        if let Some(over_command) = args.command {
            match over_command {
                OverCommand::Command(command) => {
                    let _output = execute_subcommand(command).await?;
                    Ok(())
                }
                OverCommand::RestoreKeys { password, depth } => {
                    execute_keys_restoration(password, depth).await
                }
                OverCommand::Setup { password } => execute_setup(password).await,
            }
        } else if args.continuous_run {
            execute_continuous_run().await
        } else {
            let help = Args::command().render_long_help();
            println!("{help}");
            Ok(())
        }
    })
}
