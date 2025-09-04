use anyhow::Result;
use clap::Parser;
use tokio::runtime::Builder;
use wallet::{Args, execute_subcommand};

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
        execute_subcommand(args.command).await.unwrap();
    });

    Ok(())
}
