set -e

curl -L https://risczero.com/install | bash 
/home/runner/.risc0/bin/rzup install 
source env.sh
cargo install taplo-cli --locked

cargo fmt -- --check
taplo fmt --check