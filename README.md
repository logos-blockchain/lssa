# nescience-testnet
This repo serves for Nescience testnet

For more details you can read [here](https://notes.status.im/Ya2wDpIyQquoiRiuEIM8hQ?view).

# Install dependencies

Install build dependencies
- On Linux
```sh
apt install build-essential clang libssl-dev pkg-config
```
- On Mac
```sh
xcode-select --install
brew install pkg-config openssl
```

Install Rust
```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Install Risc0

```sh
curl -L https://risczero.com/install | bash
```

Then restart your shell and run
```sh
rzup install
```
