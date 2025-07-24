set -e

source env.sh
cargo install taplo-cli --locked

cargo fmt -- --check
taplo fmt --check

export RISC0_SKIP_BUILD=1
cargo clippy --workspace --all-targets -- -D warnings
