set -e
curl -L https://risczero.com/install | bash 
/Users/runner/.risc0/bin/rzup install 
cargo build