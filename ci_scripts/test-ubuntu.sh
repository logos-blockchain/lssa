set -e

curl -L https://risczero.com/install | bash 
/home/runner/.risc0/bin/rzup install 
source env.sh

RISC0_DEV_MODE=1 cargo test --release --features no_docker

cd integration_tests
export NSSA_WALLET_HOME_DIR=$(pwd)/configs/debug/wallet/
export RUST_LOG=info
cargo run $(pwd)/configs/debug all --features no_docker
echo "Try test valid proof at least once"
cargo run $(pwd)/configs/debug test_success_private_transfer_to_another_owned_account --features no_docker
echo "Continuing in dev mode"
RISC0_DEV_MODE=1 cargo run $(pwd)/configs/debug all --features no_docker
cd ..

cd nssa/program_methods/guest && cargo test --release --features no_docker
