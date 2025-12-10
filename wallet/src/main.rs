use anyhow::Result;
use clap::{CommandFactory as _, Parser as _};
use tokio::runtime::Builder;
use wallet::cli::{Args, execute_continuous_run_with_auth, execute_subcommand_with_auth};

pub const NUM_THREADS: usize = 2;

// TODO #169: We have sample configs for sequencer, but not for wallet
// TODO #168: Why it requires config as a directory? Maybe better to deduce directory from config
// file path?
// TODO #172: Why it requires config as env var while sequencer_runner accepts as
// argument?
fn main() -> Result<()> {
    let runtime = Builder::new_multi_thread()
        .worker_threads(NUM_THREADS)
        .enable_all()
        .build()
        .unwrap();

    let args = Args::parse();

    env_logger::init();

    runtime.block_on(async move {
        if let Some(command) = args.command {
            let _output = execute_subcommand_with_auth(command, args.auth).await?;
            Ok(())
        } else if args.continuous_run {
            execute_continuous_run_with_auth(args.auth).await
        } else {
            let help = Args::command().render_long_help();
            println!("{help}");
            Ok(())
        }
    })
}
