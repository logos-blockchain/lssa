# Logos Execution Zone (LEZ)

Logos Execution Zone (LEZ) is a programmable blockchain that cleanly separates public and private state while keeping them fully interoperable. Developers can build apps that operate across transparent and privacy-preserving accounts without changing their logic. Privacy is enforced by the protocol itself through zero-knowledge proofs (ZKPs), so it is always available and automatic.


## Background

These features are provided by the Logos Execution Environment (LEE). Traditional public blockchains expose a fully transparent state: the mapping from account IDs to account values is entirely visible. LEE introduces a parallel *private state* that coexists with the public one. Together, public and private accounts form a partition of the account ID space: public IDs are visible on-chain, while private accounts are accessible only to holders of the corresponding viewing keys. Consistency across both states is enforced by ZKPs.

Public accounts are stored on-chain as a visible map from IDs to account states, and their values are updated in place. Private accounts are never stored on-chain in raw form. Each update produces a new commitment that binds the current value while keeping it hidden. Previous commitments remain on-chain, but a nullifier set marks old versions as spent, ensuring that only the most recent private state can be used in execution.


### Programmability and selective privacy

LEZ aims to deliver full programmability in a hybrid public/private model, with the same flexibility and composability as public blockchains. Developers write and deploy programs in LEZ just as they would elsewhere. The protocol automatically supports executions that involve any combination of public and private accounts. From the program’s perspective, all accounts look the same, and privacy is enforced transparently. This lets developers focus on business logic while the system guarantees privacy and correctness.

To our knowledge, this design is unique to LEZ. Other privacy-focused programmable blockchains often require developers to explicitly handle private inputs inside their app logic. In LEZ, privacy is protocol-level: programs do not change, accounts are treated uniformly, and private execution works out of the box.

---

## Example: creating and transferring tokens across states

1. Token creation (public execution)
   - Alice submits a transaction that executes the token program `New` function on-chain.
   - A new public token definition account is created.
   - The minted tokens are recorded on-chain in Alice’s public account.

2. Transfer from public to private (local / privacy-preserving execution)
   - Alice runs the token program `Transfer` function locally, sending to Bob’s private account.
   - A ZKP of correct execution is generated.
   - The proof is submitted to the blockchain and verified by validators.
   - Alice’s public balance is updated on-chain.
   - Bob’s private balance remains hidden, while the transfer is provably correct.

3. Transferring private to public (local / privacy-preserving execution)
   - Bob executes the token program `Transfer` function locally, sending to Charlie’s public account.
   - A ZKP of correct execution is generated.
   - Bob’s private balance stays hidden.
   - Charlie’s public account is updated on-chain.

4. Transfer from public to public (public execution)
   - Alice submits an on-chain transaction to run `Transfer`, sending to Charlie’s public account.
   - Execution is handled fully on-chain without ZKPs.
   - Alice’s and Charlie’s public balances are updated.

   
### Key points:
- The same token program is used in every execution.
- The only difference is execution mode: public execution updates visible state on-chain, while private execution relies on ZKPs.
- Validators verify proofs only for privacy-preserving transactions, keeping processing efficient.

---

## The account’s model

To achieve both state separation and full programmability, LEZ uses a stateless program model. Programs hold no internal state. All persistent data is stored in accounts passed explicitly into each execution. This enables precise access control and visibility while preserving composability across public and private states.

### Execution types

LEZ supports two execution types:
- Public execution runs transparently on-chain.
- Private execution runs off-chain and is verified on-chain with ZKPs.

Both public and private executions use the same Risc0 VM bytecode. Public transactions are executed directly on-chain like any standard RISC-V VM call, without proof generation. Private transactions are executed locally by users, who generate Risc0 proofs that validators verify instead of re-executing the program.

This design keeps public transactions as fast as any RISC-V–based VM and makes private transactions efficient for validators. It also supports parallel execution similar to Solana, improving throughput. The main computational cost for privacy-preserving transactions is on the user side, where ZK proofs are generated.

---
---

# Install dependencies
### Install build dependencies

- On Linux
Ubuntu / Debian
```sh
apt install build-essential clang libclang-dev libssl-dev pkg-config
```

- On Fedora
```sh
sudo dnf install clang clang-devel openssl-devel pkgconf
```

- On Mac
```sh
xcode-select --install
brew install pkg-config openssl
```

### Install Rust

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Install Risc0

```sh
curl -L https://risczero.com/install | bash
```

### Then restart your shell and run
```sh
rzup install
```

# Run tests

The LEZ repository includes both unit and integration test suites.

### Unit tests

```bash
# RISC0_DEV_MODE=1 is used to skip proof generation and reduce test runtime overhead
RISC0_DEV_MODE=1 cargo test --release
```

### Integration tests

```bash
export NSSA_WALLET_HOME_DIR=$(pwd)/integration_tests/configs/debug/wallet/
cd integration_tests
# RISC0_DEV_MODE=1 skips proof generation; RUST_LOG=info enables runtime logs
RUST_LOG=info RISC0_DEV_MODE=1 cargo run $(pwd)/configs/debug all
```

# Run the sequencer and node

The sequencer and node can be run locally:

 1. On one terminal go to the ⁨`logos-blockchain/logos-blockchain`⁩ repo and run a local logos blockchain node:
      - ⁨`git checkout master; git pull`⁩
      - ⁨`cargo clean`⁩
      - ⁨`rm ~/.logos-blockchain-circuits`⁩
      - ⁨`./scripts/setup-logos-blockchain-circuits.sh`⁩
      - ⁨`cargo build --all-features`⁩
      - ⁨`./target/debug/logos-blockchain-node nodes/node/config-one-node.yaml`⁩

 2. On another terminal go to the ⁨`logos-blockchain/lssa`⁩ repo and run indexer service:
      - ⁨`git checkout schouhy/full-bedrock-integration`⁩
      - ⁨`RUST_LOG=info cargo run --release -p indexer_service $(pwd)/integration_tests/configs/indexer/indexer_config.json`⁩

 3. On another terminal go to the ⁨`logos-blockchain/lssa`⁩ repo and run the sequencer:
      - ⁨`git checkout schouhy/full-bedrock-integration`⁩
      - ⁨`RUST_LOG=info RISC0_DEV_MODE=1 cargo run --release -p sequencer_runner sequencer_runner/configs/debug`⁩

