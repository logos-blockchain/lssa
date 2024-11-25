set -e

curl -L https://risczero.com/install | bash 
/home/runner/.risc0/bin/rzup install 
cargo install taplo-cli --locked

cargo fmt -- --check
cargo clippy --release -- -D warnings
taplo fmt --check