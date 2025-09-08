set -e

curl -L https://risczero.com/install | bash 
/home/runner/.risc0/bin/rzup install 
source env.sh

RISC0_DEV_MODE=1 cargo test --release
cd integration_tests
export NSSA_WALLET_HOME_DIR=$(pwd)/configs/debug/wallet/
export RUST_LOG=info
cargo run $(pwd)/configs/debug all
