set -e

cargo install taplo-cli --locked

cargo fmt -- --check
taplo fmt --check

RISC0_SKIP_BUILD=1 cargo clippy --workspace --all-targets -- -D warnings
