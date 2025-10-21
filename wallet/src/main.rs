use anyhow::Result;
use clap::{CommandFactory, Parser};
use tokio::runtime::Builder;
use wallet::{Args, execute_continious_run, execute_subcommand};

pub const NUM_THREADS: usize = 2;

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
            execute_subcommand(command).await.unwrap();
        } else if args.continious_run {
            execute_continious_run().await.unwrap();
        } else {
            let help = Args::command().render_long_help();
            println!("{help}");
        }
    });

    Ok(())
}
