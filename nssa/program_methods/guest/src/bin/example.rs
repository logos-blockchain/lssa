use risc0_zkvm::guest::env;
fn main() {
    let a: u32 = env::read();
    env::commit(&a);
}

