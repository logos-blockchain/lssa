set -e

cargo +nightly fmt -- --check

cargo install taplo-cli --locked
taplo fmt --check

RISC0_SKIP_BUILD=1 cargo clippy --workspace --all-targets -- -D warnings
# Lint all programs individually to avoid feature unification issues
RISC0_SKIP_BUILD=1 cargo clippy -p "*programs" -- -D warnings
