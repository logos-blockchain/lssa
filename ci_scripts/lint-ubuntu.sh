set -e

source env.sh
cargo install taplo-cli --locked

cargo fmt -- --check
taplo fmt --check

head -n1000 ./node_core/src/lib.rs

# export RISC0_SKIP_BUILD=1
# cargo clippy --workspace --all-targets -- -D warnings
