set -e

curl -L https://risczero.com/install | bash 
/home/runner/.risc0/bin/rzup install 
source env.sh

cargo test --release
cd integration_tests
export NSSA_WALLET_HOME_DIR=$(pwd)/configs/debug/wallet/
cargo run $(pwd)/configs/debug all