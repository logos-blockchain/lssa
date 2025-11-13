# Nescience

Nescience State Separation Architecture (NSSA) is a programmable blockchain system that introduces a clean separation between public and private states, while keeping them fully interoperable. It lets developers build apps that can operate across both transparent and privacy-preserving accounts without changing how they write or deploy programs. Privacy is handled automatically by the protocol through zero-knowledge proofs (ZKPs). The result is a fully composable blockchain where privacy comes built-in.

## Background

Typically, public blockchains maintain a fully transparent state, where the mapping from addresses to account values is entirely visible. In NSSA, we introduce a parallel *private state*, a new layer of accounts that coexists with the public one. The public and private states can be viewed as a partition of the address space: accounts with public addresses are openly visible, while private accounts are accessible only to holders of the corresponding viewing keys. Consistency across both states is enforced through zero-knowledge proofs (ZKPs).

Public accounts are represented on-chain as a visible map from addresses to account states and are modified in-place when their values change. Private accounts, by contrast, are never stored in raw form on-chain. Each update creates a new commitment, which cryptographically binds the current value of the account while preserving privacy. Commitments of previous valid versions remain on-chain, but a nullifier set is maintained to mark old versions as spent, ensuring that only the most up-to-date version of each private account can be used in any execution.

### Programmability and selective privacy

Our goal is to enable full programmability within this hybrid model, matching the flexibility and composability of public blockchains. Developers write and deploy programs in NSSA just as they would on any other blockchain. Privacy, along with the ability to execute programs involving any combination of public and private accounts, is handled entirely at the protocol level and available out of the box for all programs. From the program’s perspective, all accounts are indistinguishable. This abstraction allows developers to focus purely on business logic, while the system transparently enforces privacy and consistency guarantees.

To the best of our knowledge, this approach is unique to Nescience. Other programmable blockchains with a focus on privacy typically adopt a developer-driven model for private execution, meaning that dApp logic must explicitly handle private inputs correctly. In contrast, Nescience handles privacy at the protocol level, so developers do not need to modify their programs—private and public accounts are treated uniformly, and privacy-preserving execution is available out of the box.

### Example: creating and transferring tokens across states

1. Token creation (public execution):
   - Alice submits a transaction to execute the token program `Create` function on-chain.
   - A new public token account is created, representing the token.
   - The minted tokens are recorded on-chain and fully visible on Alice's public account.
2. Transfer from public to private (local / privacy-preserving execution)
   - Alice executes the token program `Transfer` function locally, specifying a Bob’s private account as recipient.
   - A ZKP of correct execution is generated.
   - The proof is submitted to the blockchain, and validator nodes verify it.
   - Alice's public account balance is modified accordingly.
   - Bob’s private account and balance remain hidden, while the transfer is provably valid.
3. Transferring private to public (local / privacy-preserving execution)
   - Bob executes the token program `Transfer` function locally, specifying a Charlie’s public account as recipient.
   - A ZKP of correct execution is generated.
   - Bob’s private account and balance still remain hidden.
   - Charlie's public account is modified with the new tokens added.
4. Transferring public to public (public execution):
   - Alice submits a transaction to execute the token program `Transfer` function on-chain, specifying Charlie's public account as recipient.
   - The execution is handled on-chain without ZKPs involved.
   - Alice's and Charlie's accounts are modified according to the transaction.
   
#### Key points:
- The same token program is used in all executions.
- The difference lies in execution mode: public executions update visible accounts on-chain, while private executions rely on ZKPs.
- Validators only need to verify proofs for privacy-preserving transactions, keeping processing efficient.

### The account’s model

To achieve both state separation and full programmability, NSSA adopts a stateless program model. Programs do not hold internal state. Instead, all persistent data resides in accounts explicitly passed to the program during execution. This design enables fine-grained control over access and visibility while maintaining composability across public and private states.

### Execution types

Execution is divided into two fundamentally distinct types based on how they are processed: public execution, which is executed transparently on-chain, and private execution, which occurs off-chain. For private execution, the blockchain relies on ZKPs to verify the correctness of execution and ensure that all system invariants are preserved.

Both public and private executions of the same program are enforced to use the same Risc0 VM bytecode. For public transactions, programs are executed directly on-chain like any standard RISC-V VM execution, without generating or verifying proofs. For privacy-preserving transactions, users generate Risc0 ZKPs of correct execution, and validator nodes only verify these proofs rather than re-executing the program. This design ensures that from a validator’s perspective, public transactions are processed as quickly as any RISC-V–based VM, while verification of ZKPs keeps privacy-preserving transactions efficient as well. Additionally, the system naturally supports parallel execution similar to Solana, further increasing throughput. The main computational bottleneck for privacy-preserving transactions lies on the user side, in generating zk proofs.

### Resources
- [IFT Research call](https://forum.vac.dev/t/ift-research-call-september-10th-2025-updates-on-the-development-of-nescience/566)
- [NSSA v0.2 specs](https://www.notion.so/NSSA-v0-2-specifications-2848f96fb65c800c9818e6f66d9be8f2)
- [Choice of VM/zkVM](https://www.notion.so/Conclusion-on-the-chosen-VM-and-zkVM-for-NSSA-2318f96fb65c806a810ed1300f56992d)
- [NSSA vs other privacy projects](https://www.notion.so/Privacy-projects-comparison-2688f96fb65c8096b694ecf7e4deca30)
- [NSSA state model](https://www.notion.so/Public-state-model-decision-2388f96fb65c80758b20c76de07b1fcc)
- [NSSA sequencer specs](https://www.notion.so/Sequencer-specs-2428f96fb65c802da2bfea7b0b214ecb)
- [NSSA sequencer code](https://www.notion.so/NSSA-sequencer-pseudocode-2508f96fb65c805e8859e047dffd6785)
- [NSSA Token program desing](https://www.notion.so/Token-program-design-2538f96fb65c80a1b4bdc4fd9dd162d7)
- [NSSA cross program calls](https://www.notion.so/NSSA-cross-program-calls-Tail-call-model-proposal-extended-version-2838f96fb65c8096b3a2d390444193b6)


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

# Run tests

The NSSA repository includes both unit and integration test suites.

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

# Run the sequencer

The sequencer can be run locally:

```bash
cd sequencer_runner
RUST_LOG=info cargo run --release configs/debug
```

If everything went well you should see an output similar to this:
```bash
[2025-11-13T19:50:29Z INFO  sequencer_runner] Sequencer core set up
[2025-11-13T19:50:29Z INFO  network] Starting http server at 0.0.0.0:3040
[2025-11-13T19:50:29Z INFO  actix_server::builder] starting 8 workers
[2025-11-13T19:50:29Z INFO  sequencer_runner] HTTP server started
[2025-11-13T19:50:29Z INFO  sequencer_runner] Starting main sequencer loop
[2025-11-13T19:50:29Z INFO  actix_server::server] Tokio runtime found; starting in existing Tokio runtime
[2025-11-13T19:50:29Z INFO  actix_server::server] starting service: "actix-web-service-0.0.0.0:3040", workers: 8, listening on: 0.0.0.0:3040
[2025-11-13T19:50:39Z INFO  sequencer_runner] Collecting transactions from mempool, block creation
[2025-11-13T19:50:39Z INFO  sequencer_core] Created block with 0 transactions in 0 seconds
[2025-11-13T19:50:39Z INFO  sequencer_runner] Block with id 2 created
[2025-11-13T19:50:39Z INFO  sequencer_runner] Waiting for new transactions
```

# Try the Wallet CLI

## Install
This repo contains a CLI to interact with the Nescience sequencer. To install it run the following from the root directory of the repository.

```bash
cargo install --path wallet --force
```

To use it the environment variable `NSSA_WALLET_HOME_DIR` needs to be set to the path where the wallet configuration file is.
There is one configuration file in `integration_tests/configs/debug/wallet/` that can be used. For that, from the root directory of this repository run:
```bash
export NSSA_WALLET_HOME_DIR=$(pwd)/configs/debug/wallet/
```

## Tutorial

### Health-check

Check that the node is running and the wallet can connect to it with the following command

```bash
wallet check-health
```

You should see `✅All looks good!`.

### The commands

The wallet comes with a variety of commands to interact and fetch information from the node. Run `wallet help` to see the available commands.

```bash
Commands:
  auth-transfer  Authenticated transfer subcommand
  chain-info     Generic chain info subcommand
  account        Account view and sync subcommand
  pinata         Pinata program interaction subcommand
  token          Token program interaction subcommand
  check-health   Check the wallet can connect to the node and builtin local programs match the remote versions
```

### Accounts
Every piece of state in NSSA is encoded in an account. Public and private accounts can be created with the CLI.

#### Create a new public account
```bash
wallet account new public

# Output:
Generated new account with addr Public/9ypzv6GGr3fwsgxY7EZezg5rz6zj52DPCkmf1vVujEiJ
```

The address is the identifier of the account needed when executing programs that involve it.

##### Account initialization
To see the current status of the newly generated account run

```bash
# Replace the address with yours
wallet account get --addr Public/9ypzv6GGr3fwsgxY7EZezg5rz6zj52DPCkmf1vVujEiJ

# Output:
Account is Uninitialized
```

Every new account is uninitialized. That means that it is not yet associated with any program. Programs can claim uninitialized accounts. Once a program claims an account, it will be owned by that program. This process is irreversible.

How to do that depends on each program. In this section we'll initialize the account for the **Authenticated transfers program**. It is a program that safely handles native token transfers by requiring authentication to debit funds.

To initialize the account under the ownership of the Authenticated transfer program run:

```bash
# This command will submit a public transaction to execute the `init` function of
# the Authenticated-transfer program. The wallet will poll the sequencer to check
# that the transaction was accepted in a block. That may take some seconds.
wallet auth-transfer init --addr Public/9ypzv6GGr3fwsgxY7EZezg5rz6zj52DPCkmf1vVujEiJ
```

Once that finishes, you can check the new status of the account with the same command as before

```bash
wallet account get --addr Public/9ypzv6GGr3fwsgxY7EZezg5rz6zj52DPCkmf1vVujEiJ

# Output:
Account owned by authenticated transfer program
{"balance":0}
```

#### Create a new private account
```bash
wallet account new private

# Output:
Generated new account with addr Private/6n9d68Q3riGyWHbcGFLigmjaaE49bpGBpwq3TYbfgLNv
With npk 5b09bc16a637c7154a85d3cfce2c0152fadfcd36b38dcc00479aac3f3dd291fc
With ipk 02e12ecdabc33d207624823062e10e2d2c1246180431c476e816ffd9e634badf34
```



