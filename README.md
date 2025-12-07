# Nescience

Nescience State Separation Architecture (NSSA) is a programmable blockchain system that introduces a clean separation between public and private states, while keeping them fully interoperable. It lets developers build apps that can operate across both transparent and privacy-preserving accounts. Privacy is handled automatically by the protocol through zero-knowledge proofs (ZKPs). The result is a programmable blockchain where privacy comes built-in.

## Background

Typically, public blockchains maintain a fully transparent state, where the mapping from account IDs to account values is entirely visible. In NSSA, we introduce a parallel *private state*, a new layer of accounts that coexists with the public one. The public and private states can be viewed as a partition of the account ID space: accounts with public IDs are openly visible, while private accounts are accessible only to holders of the corresponding viewing keys. Consistency across both states is enforced through zero-knowledge proofs (ZKPs).

Public accounts are represented on-chain as a visible map from IDs to account states and are modified in-place when their values change. Private accounts, by contrast, are never stored in raw form on-chain. Each update creates a new commitment, which cryptographically binds the current value of the account while preserving privacy. Commitments of previous valid versions remain on-chain, but a nullifier set is maintained to mark old versions as spent, ensuring that only the most up-to-date version of each private account can be used in any execution.

### Programmability and selective privacy

Our goal is to enable full programmability within this hybrid model, matching the flexibility and composability of public blockchains. Developers write and deploy programs in NSSA just as they would on any other blockchain. Privacy, along with the ability to execute programs involving any combination of public and private accounts, is handled entirely at the protocol level and available out of the box for all programs. From the program’s perspective, all accounts are indistinguishable. This abstraction allows developers to focus purely on business logic, while the system transparently enforces privacy and consistency guarantees.

To the best of our knowledge, this approach is unique to Nescience. Other programmable blockchains with a focus on privacy typically adopt a developer-driven model for private execution, meaning that dApp logic must explicitly handle private inputs correctly. In contrast, Nescience handles privacy at the protocol level, so developers do not need to modify their programs—private and public accounts are treated uniformly, and privacy-preserving execution is available out of the box.

### Example: creating and transferring tokens across states

1. Token creation (public execution):
   - Alice submits a transaction to execute the token program `New` function on-chain.
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
Ubuntu / Debian
```sh
apt install build-essential clang libssl-dev pkg-config
```

Fedora
```sh
sudo dnf install clang openssl-devel pkgconf llvm
```

> **Note for Fedora 41+ users:** GCC 14+ has stricter C++ standard library headers that cause build failures with the bundled RocksDB. You must set `CXXFLAGS="-include cstdint"` when running cargo commands. See the [Run tests](#run-tests) section for examples.

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

# On Fedora 41+ (GCC 14+), prefix with CXXFLAGS to fix RocksDB build:
CXXFLAGS="-include cstdint" RISC0_DEV_MODE=1 cargo test --release
```

### Integration tests

```bash
export NSSA_WALLET_HOME_DIR=$(pwd)/integration_tests/configs/debug/wallet/
cd integration_tests
# RISC0_DEV_MODE=1 skips proof generation; RUST_LOG=info enables runtime logs
RUST_LOG=info RISC0_DEV_MODE=1 cargo run $(pwd)/configs/debug all

# On Fedora 41+ (GCC 14+), prefix with CXXFLAGS to fix RocksDB build:
CXXFLAGS="-include cstdint" RUST_LOG=info RISC0_DEV_MODE=1 cargo run $(pwd)/configs/debug all
```

# Run the sequencer

The sequencer can be run locally:

```bash
cd sequencer_runner
RUST_LOG=info cargo run --release configs/debug

# On Fedora 41+ (GCC 14+), prefix with CXXFLAGS to fix RocksDB build:
CXXFLAGS="-include cstdint" RUST_LOG=info cargo run --release configs/debug
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

This repository includes a CLI for interacting with the Nescience sequencer. To install it, run the following command from the root of the repository:

```bash
cargo install --path wallet --force

# On Fedora 41+ (GCC 14+), prefix with CXXFLAGS to fix RocksDB build:
CXXFLAGS="-include cstdint" cargo install --path wallet --force
```

Run `wallet help` to check everything went well.

## Tutorial

This tutorial walks you through creating accounts and executing NSSA programs in both public and private contexts.

> [!NOTE]
> The NSSA state is split into two separate but interconnected components: the public state and the private state.
> The public state is an on-chain, publicly visible record of accounts indexed by their Account IDs
> The private state mirrors this, but the actual account values are stored locally by each account owner. On-chain, only a hidden commitment to each private account state is recorded. This allows the chain to enforce freshness (i.e., prevent the reuse of stale private states) while preserving privacy and unlinkability across executions and private accounts.
> 
> Every piece of state in NSSA is stored in an account (public or private). Accounts are either uninitialized or are owned by a program, and programs can only modify the accounts they own.
> 
> In NSSA, accounts can only be modified through program execution. A program is the sole mechanism that can change an account’s value.
> Programs run publicly when all involved accounts are public, and privately when at least one private account participates.

### Health-check

Verify that the node is running and that the wallet can connect to it:

```bash
wallet check-health
```

You should see `✅ All looks good!`.

### The commands

The wallet provides several commands to interact with the node and query state. To see the full list, run `wallet help`:

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

> [!NOTE]
> Accounts are the basic unit of state in NSSA. They essentially hold native tokens and arbitrary data managed by some program.

The CLI provides commands to manage accounts. Run `wallet account` to see the options available:
```bash
Commands:
  get           Get account data
  new           Produce new public or private account
  sync-private  Sync private accounts
  help  Print this message or the help of the given subcommand(s)
```

#### Create a new public account

You can create both public and private accounts through the CLI. For example:

```bash
wallet account new public

# Output:
Generated new account with account_id Public/9ypzv6GGr3fwsgxY7EZezg5rz6zj52DPCkmf1vVujEiJ
```

This id is required when executing any program that interacts with the account.

> [!NOTE]
> Public accounts live on-chain and are identified by a 32-byte Account ID.
> Running `wallet account new public` generates a fresh keypair for the signature scheme used in NSSA.
> The account ID is derived from the public key. The private key is used to sign transactions and to authorize the account in program executions.

#### Account initialization

To query the account’s current status, run:

```bash
# Replace the id with yours
wallet account get --account-id Public/9ypzv6GGr3fwsgxY7EZezg5rz6zj52DPCkmf1vVujEiJ

# Output:
Account is Uninitialized
```

> [!NOTE]
> New accounts begin in an uninitialized state, meaning they are not yet owned by any program. A program may claim an uninitialized account; once claimed, the account becomes owned by that program.
> Owned accounts can only be modified through executions of the owning program. The only exception is native-token credits: any program may credit native tokens to any account.
> However, debiting native tokens from an account must always be performed by its owning program.

In this example, we will initialize the account for the Authenticated transfer program, which securely manages native token transfers by requiring authentication for debits.

Initialize the account by running:

```bash
# This command submits a public transaction executing the `init` function of the
# Authenticated-transfer program. The wallet polls the sequencer until the
# transaction is included in a block, which may take several seconds.
wallet auth-transfer init --account-id Public/9ypzv6GGr3fwsgxY7EZezg5rz6zj52DPCkmf1vVujEiJ
```

After it completes, check the updated account status:

```bash
wallet account get --account-id Public/9ypzv6GGr3fwsgxY7EZezg5rz6zj52DPCkmf1vVujEiJ

# Output:
Account owned by authenticated transfer program
{"balance":0}
```

### Funding the account: executing the Piñata program

Now that we have a public account initialized by the authenticated transfer program, we need to fund it. For that, the testnet provides the Piñata program.

```bash
# Complete with your id
wallet pinata claim --to Public/9ypzv6GGr3fwsgxY7EZezg5rz6zj52DPCkmf1vVujEiJ
```

After the claim succeeds, the account will be funded with some tokens:

```bash
wallet account get --account-id Public/9ypzv6GGr3fwsgxY7EZezg5rz6zj52DPCkmf1vVujEiJ

# Output:
Account owned by authenticated transfer program
{"balance":150}
```

### Native token transfers: executing the Authenticated transfers program

NSSA comes with a program for managing and transferring native tokens. Run `wallet auth-transfer` to see the options available:
```bash
Commands:
  init  Initialize account under authenticated transfer program
  send  Send native tokens from one account to another with variable privacy
  help  Print this message or the help of the given subcommand(s)
```

We have already used the `init` command. The `send` command is used to execute the `Transfer` function of the authenticated program.
Let's try it. For that we need to create another account for the recipient of the transfer.

```bash
wallet account new public

# Output:
Generated new account with account_id Public/Ev1JprP9BmhbFVQyBcbznU8bAXcwrzwRoPTetXdQPAWS
```


> [!NOTE]
> The new account is uninitialized. The authenticated transfers program will claim any uninitialized account used in a transfer. So we don't need to manually initialize the recipient account.

Let's send 37 tokens to the new account.

```bash
wallet auth-transfer send \
    --from Public/9ypzv6GGr3fwsgxY7EZezg5rz6zj52DPCkmf1vVujEiJ \
    --to Public/Ev1JprP9BmhbFVQyBcbznU8bAXcwrzwRoPTetXdQPAWS \
    --amount 37
```

Once that succeeds we can check the states.

```bash
# Sender account
wallet account get --account-id Public/HrA8TVjBS8UVf9akV7LRhyh6k4c7F6PS7PvqgtPmKAT8

# Output:
Account owned by authenticated transfer program
{"balance":113}
```

```bash
# Recipient account
wallet account get --account-id Public/Ev1JprP9BmhbFVQyBcbznU8bAXcwrzwRoPTetXdQPAWS

# Output:
Account owned by authenticated transfer program
{"balance":37}
```

#### Create a new private account

> [!NOTE]
> Private accounts are structurally identical to public accounts; they differ only in how their state is stored off-chain and represented on-chain.
> The raw values of a private account are never stored on-chain. Instead, the chain only holds a 32-byte commitment (a hash-like binding to the actual values). Transactions include encrypted versions of the private values so that users can recover them from the blockchain. The decryption keys are known only to the user and are never shared.
> Private accounts are not managed through the usual signature mechanism used for public accounts. Instead, each private account is associated with two keypairs:
> - *Nullifier keys*, for using the corresponding private account in privacy preserving executions.
> - *Viewing keys*, used for encrypting and decrypting the values included in transactions.
>
> Private accounts also have a 32-byte identifier, derived from the nullifier public key.
>
> Just like public accounts, private accounts can only be initialized once. Any user can initialize them without knowing the owner's secret keys. However, modifying an initialized private account through an off-chain program execution requires knowledge of the owner’s secret keys.
>
> Transactions that modify the values of a private account include a commitment to the new values, which will be added to the on-chain commitment set. They also include a nullifier that marks the previous version as old.
> The nullifier is constructed so that it cannot be linked to any prior commitment, ensuring that updates to the same private account cannot be correlated.

Now let’s switch to the private state and create a private account.

```bash
wallet account new private

# Output:
Generated new account with account_id Private/HacPU3hakLYzWtSqUPw6TUr8fqoMieVWovsUR6sJf7cL
With npk e6366f79d026c8bd64ae6b3d601f0506832ec682ab54897f205fffe64ec0d951
With ipk 02ddc96d0eb56e00ce14994cfdaec5ae1f76244180a919545983156e3519940a17
```

For now, focus only on the account id. Ignore the `npk` and `ipk` values. These are the Nullifier public key and the Viewing public key. They are stored locally in the wallet and are used internally to build privacy-preserving transactions.
Also, the account id for private accounts is derived from the `npk` value. But we won't need them now.

Just like public accounts, new private accounts start out uninitialized:

```bash
wallet account get --account-id Private/HacPU3hakLYzWtSqUPw6TUr8fqoMieVWovsUR6sJf7cL

# Output:
Account is Uninitialized
```
Unlike public accounts, private accounts are never visible to the network. They exist only in your local wallet storage.

#### Sending tokens from the public account to the private account

Sending tokens to an uninitialized private account causes the Authenticated-Transfers program to claim it. Just like with public accounts.
This happens because program execution logic does not depend on whether the involved accounts are public or private.

Let’s send 17 tokens to the new private account.

The syntax is identical to the public-to-public transfer; just set the private ID as the recipient.

This command will run the Authenticated-Transfer program locally, generate a proof, and submit it to the sequencer. Depending on your machine, this can take from 30 seconds to 4 minutes.

```bash
wallet auth-transfer send \
    --from Public/Ev1JprP9BmhbFVQyBcbznU8bAXcwrzwRoPTetXdQPAWS \
    --to Private/HacPU3hakLYzWtSqUPw6TUr8fqoMieVWovsUR6sJf7cL \
    --amount 17
```

After it succeeds, check both accounts:

```bash
# Public sender account
wallet account get --account-id Public/Ev1JprP9BmhbFVQyBcbznU8bAXcwrzwRoPTetXdQPAWS

# Output:
Account owned by authenticated transfer program
{"balance":20}
```

```bash
# Private recipient account
wallet account get --account-id Private/HacPU3hakLYzWtSqUPw6TUr8fqoMieVWovsUR6sJf7cL

# Output:
Account owned by authenticated transfer program
{"balance":17}
```

> [!NOTE]
> The last command does not query the network.
> It works even offline because private account data lives only in your wallet storage. Other users cannot read your private balances.

#### Digression: modifying private accounts

As a general rule, private accounts can only be modified through a program execution performed by their owner. That is, the person who holds the private key for that account. There is one exception: an uninitialized private account may be initialized by any user, without requiring the private key. After initialization, only the owner can modify it.

This mechanism enables a common use case: transferring funds from any account (public or private) to a private account owned by someone else. For such transfers, the recipient’s private account must be uninitialized.


#### Sending tokens from the public account to a private account owned by someone else

For this tutorial, we’ll simulate that scenario by creating a new private account that we own, but we’ll treat it as if it belonged to someone else. 

Let's create a new (uninitialized) private account like before:

```bash
wallet account new private

# Output:
Generated new account with account_id Private/AukXPRBmrYVqoqEW2HTs7N3hvTn3qdNFDcxDHVr5hMm5
With npk 0c95ebc4b3830f53da77bb0b80a276a776cdcf6410932acc718dcdb3f788a00e
With ipk 039fd12a3674a880d3e917804129141e4170d419d1f9e28a3dcf979c1f2369cb72
```

Now we'll ignore the private account ID and focus on the `npk` and `ipk` values. We'll need this to send tokens to a foreign private account. Syntax is very similar.

```bash
wallet auth-transfer send \
    --from Public/Ev1JprP9BmhbFVQyBcbznU8bAXcwrzwRoPTetXdQPAWS \
    --to-npk 0c95ebc4b3830f53da77bb0b80a276a776cdcf6410932acc718dcdb3f788a00e \
    --to-ipk 039fd12a3674a880d3e917804129141e4170d419d1f9e28a3dcf979c1f2369cb72 \
    --amount 3
```

The command above produces a privacy-preserving transaction, which may take a few minutes to complete. The updated values of the private account are encrypted and included in the transaction.

Once the transaction is accepted, the recipient must run `wallet account sync-private`. This command scans the chain for encrypted values that belong to their private accounts and updates the local versions accordingly.


#### Transfers in other combinations of public and private accounts

We’ve shown how to use the authenticated-transfers program for transfers between two public accounts, and for transfers from a public sender to a private recipient. Sending tokens from a private account (whether to a public account or to another private account) works in essentially the same way.

### The token program

So far, we’ve made transfers using the authenticated-transfers program, which handles native token transfers. The Token program, on the other hand, is used for creating and managing custom tokens.

> [!NOTE]
> The token program is a single program responsible for creating and managing all tokens. There is no need to deploy new programs to introduce new tokens. All token-related operations are performed by invoking the appropriate functions of the token program.

The CLI provides commands to execute the token program. To see the options available run `wallet token`:

```bash
Commands:
  new   Produce a new token
  send  Send tokens from one account to another with variable privacy
  help  Print this message or the help of the given subcommand(s)
```


> [!NOTE]
> The Token program manages its accounts in two categories. Meaning, all accounts owned by the Token program fall into one of these types.
> - Token definition accounts: these accounts store metadata about a token, such as its name, total supply, and other identifying properties. They act as the token’s unique identifier.
> - Token holding accounts: these accounts hold actual token balances. In addition to the balance, they also record which token definition they belong to.

#### Creating a new token

To create a new token, simply run `wallet token new`. This will create a transaction to execute the `New` function of the token program.
The command expects a name, the desired total supply, and two uninitialized accounts:
- One that will be initialized as the token definition account for the new token.
- Another that will be initialized as a token holding account and receive the token’s entire initial supply.


##### New token with both definition and supply accounts set as public

For example, let's create two new (uninitialized) public accounts and then use them to create a new token.

```bash
wallet account new public

# Output:
Generated new account with account_id Public/4X9kAcnCZ1Ukkbm3nywW9xfCNPK8XaMWCk3zfs1sP4J7
```

```bash
wallet account new public

# Output:
Generated new account with account_id Public/9RRSMm3w99uCD2Jp2Mqqf6dfc8me2tkFRE9HeU2DFftw
```

Now we use them to create a new token. Let's call it the "Token A"

```bash
wallet token new \
    --name TOKENA \
    --total-supply 1337 \
    --definition-account-id Public/4X9kAcnCZ1Ukkbm3nywW9xfCNPK8XaMWCk3zfs1sP4J7 \
    --supply-account-id Public/9RRSMm3w99uCD2Jp2Mqqf6dfc8me2tkFRE9HeU2DFftw
```

After it succeeds, we can inspect the two accounts to see how they were initialized.

```bash
wallet account get --account-id Public/4X9kAcnCZ1Ukkbm3nywW9xfCNPK8XaMWCk3zfs1sP4J7

# Output:
Definition account owned by token program
{"account_type":"Token definition","name":"TOKENA","total_supply":1337}
```

```bash
wallet account get --account-id Public/9RRSMm3w99uCD2Jp2Mqqf6dfc8me2tkFRE9HeU2DFftw

# Output:
Holding account owned by token program
{"account_type":"Token holding","definition_id":"4X9kAcnCZ1Ukkbm3nywW9xfCNPK8XaMWCk3zfs1sP4J7","balance":1337}
```

##### New token with public account definition but private holding account for initial supply

Let’s create a new token, but this time using a public definition account and a private holding account to store the entire supply.

Since we can’t reuse the accounts from the previous example, we need to create fresh ones for this case.

```bash
wallet account new public

# Output:
Generated new account with account_id Public/GQ3C8rbprTtQUCvkuVBRu3v9wvUvjafCMFqoSPvTEVii
```

```bash
wallet account new private


# Output:
Generated new account with account_id Private/HMRHZdPw4pbyPVZHNGrV6K5AA95wACFsHTRST84fr3CF
With npk 6a2dfe433cf28e525aa0196d719be3c16146f7ee358ca39595323f94fde38f93
With ipk 03d59abf4bee974cc12ddb44641c19f0b5441fef39191f047c988c29a77252a577
```

And we use them to create the token.

Now we use them to create a new token. Let's call it "Token B".

```bash
wallet token new \
    --name TOKENB \
    --total-supply 7331 \
    --definition-account-id Public/GQ3C8rbprTtQUCvkuVBRu3v9wvUvjafCMFqoSPvTEVii \
    --supply-account-id Private/HMRHZdPw4pbyPVZHNGrV6K5AA95wACFsHTRST84fr3CF
```

After it succeeds, we can check their values

```bash
wallet account get --account-id Public/GQ3C8rbprTtQUCvkuVBRu3v9wvUvjafCMFqoSPvTEVii

# Output:
Definition account owned by token program
{"account_type":"Token definition","name":"TOKENB","total_supply":7331}
```

```bash
wallet account get --account-id Private/HMRHZdPw4pbyPVZHNGrV6K5AA95wACFsHTRST84fr3CF

# Output:
Holding account owned by token program
{"account_type":"Token holding","definition_id":"GQ3C8rbprTtQUCvkuVBRu3v9wvUvjafCMFqoSPvTEVii","balance":7331}
```

Like any other private account owned by us, it cannot be seen by other users.

#### Custom token transfers

The Token program has a function to move funds from one token holding account to another one. If executed with an uninitialized account as the recipient, this will be automatically claimed by the token program.

The transfer function can be executed with the `wallet token send` command.

Let's create a new public account for the recipient.

```bash
wallet account new public

# Output:
Generated new account with account_id Public/88f2zeTgiv9LUthQwPJbrmufb9SiDfmpCs47B7vw6Gd6
```

Let's send 10 B tokens to this new account. We'll debit this from the supply account used in the creation of the token.

```bash
wallet token send \
    --from Private/HMRHZdPw4pbyPVZHNGrV6K5AA95wACFsHTRST84fr3CF \
    --to Public/88f2zeTgiv9LUthQwPJbrmufb9SiDfmpCs47B7vw6Gd6 \
    --amount 10
```

Let's inspect the public account:

```bash
wallet account get --account-id Public/88f2zeTgiv9LUthQwPJbrmufb9SiDfmpCs47B7vw6Gd6

# Output:
Holding account owned by token program
{"account_type":"Token holding","definition_id":"GQ3C8rbprTtQUCvkuVBRu3v9wvUvjafCMFqoSPvTEVii","balance":10}
```

### Chain information

The wallet provides some commands to query information about the chain. These are under the `wallet chain-info` command.

```bash
Commands:
  current-block-id  Get current block id from sequencer
  block             Get block at id from sequencer
  transaction       Get transaction at hash from sequencer
```

For example, run this to find the current block id.

```bash
wallet chain-info current-block-id

# Output:
Last block id is 65537
```


