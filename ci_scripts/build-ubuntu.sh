set -e
curl -L https://risczero.com/install | bash 
/home/runner/.risc0/bin/rzup install 
RUSTFLAGS="-D warnings" cargo build 
