use anyhow::Result;

use integration_tests::main_tests_runner;

pub const NUM_THREADS: usize = 8;

fn main() -> Result<()> {
    actix::System::with_tokio_rt(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(NUM_THREADS)
            .enable_all()
            .build()
            .unwrap()
    })
    .block_on(main_tests_runner())
}
