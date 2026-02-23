## Documentation feedback

First, we need to ask ourselves what kind of persona is reading the docs:

- Are they a developer?
  - If yes, what kind?
    - Web3, non-Web3 (understanding of basic blockchain concepts)
    - Which language(s) do they know? Rust -> a lot easier
    - Do they come from Ethereum, Solana, Cosmos...
    - Are they familiar with the Solana tech stack, account model, etc?
- Are they looking to run a node/blockchain infra?
  - Different approach, probably not supposed to be in this repo, but another one (or another docs section) - I also assume this is not a priority right now.

We should write documentation while keeping specific target group(s) in mind, and delegate to other resources when the reader persona deviates from the target group baseline.

I approached this as a Web3 developer with very little Rust & Solana experience. I needed to learn the basics of the account model, instructions, & PDAs, while also managing my way around an unfamiliar language syntax.

I was missing clickable links to resources, repos, folders, etc. Many opportunities to delegate documentation work to other resources - ie, if the system is based on the account model, why not link back to the original Solana documentation which is actively maintained and a great resource to get started? Also, whenever new concepts/terminology is introduced, linking to a further resource is a great way to help the user understand. For example, the `witness` was introduced without any explanation or further link.

I could not find information about how exactly the account model is applied to the stack - ie, is it fully compatible with/based on Solana, are there some things that work there but not here, etc?

- For this, a comparison list between differences would be a good resource and time-saver for newcomers. This can also be done via code-first examples, showcasing differences and similarities between a LEE and Solana program with the same or similar business logic

I found it hard to explore my actions. Running the `explorer_serivce` via docker builds and runs properly, but displays a 404 error upon opening up `localhost:8080`. Managed to get `cargo leptos watch` working after a few runs. Even after that, I kept getting stuck with a "Cannot reach RPC error".

For debugging purposes, it would be good to set expectations about which service is running on which port by default, somewhere where the user can easily reference it (ie top of docs file).

The sequencer was constantly giving a large amount of WARN lines, ie

```
[2026-02-22T20:17:32Z WARN  sequencer_runner] Failed to resubmit block with id 1 with error Internal server error: Item already in mempool
[2026-02-22T20:17:32Z WARN  sequencer_runner] Failed to resubmit block with id 2 with error Internal server error: Item already in mempool
[2026-02-22T20:17:32Z WARN  sequencer_runner] Failed to resubmit block with id 3 with error Internal server error: Item already in mempool
[2026-02-22T20:17:32Z WARN  sequencer_runner] Failed to resubmit block with id 4 with error Internal server error: Item already in mempool
[2026-02-22T20:17:32Z WARN  sequencer_runner] Failed to resubmit block with id 5 with error Internal server error: Item already in mempool
```

etc, even with a clean state. Not sure what is causing this.

## Wallet Issues

- Name could be less generic - ie `leew`, `lssew` `logoswallet`
- Naming accounts should be a default during account creation
- Plain-text password input should be hidden
- Some simple docs/readme in `wallet/` would be nice:
  - How does account generation work
  - Where is configuration data stored
  - Which accounts are preconfigured/premined (if there are any)?
- If we have pre-installed programs to interact with in the wallet, some info in the CLI or docs/links would be nice as to what they are
- I got no feedback from the wallet when deploying a program - had to chase sequencer logs to see if there was a transaction on the chain and the program was deployed properly. Ideally a program ID or address or similar would be displayed, possibly also to be used for searching for the program via the explorer.

## Improvements

I believe both the root `README.md` & `examples/program_deployment/README.md` are too long and should be cut into smaller, comprehensive chunks. Currently, the documents are a bit overwhelming and it's easy to get lost in them.

Ideally, I would separate the docs into:

- Tech Stack Intro - What is Logos Blockchain/LEE, Why care?
  - What is NSSA, what this repo is. What tech it's based on, what the goal of the project is.
  - Main practical functionalities and possibilities of the tech
- Environment setup:
  - Blockchain node, indexer, sequencer, explorer service
  - Wallet
  - Health-check everything -> ready to interact with the blockchain!
- Interacting with the chain using the wallet:
  - Preconfigured wallets, how to preconfigure your own (ie is there a config file somewhere that can be edited at this stage of tech?) since there is no easy airdrop/premine process?
  - Simple transaction submission via CLI, ie a balance transfer via the wallet
  - Exploring actions/blocks on the explorer, helping the developer see what they did outside of only logs
- NSSA programs:
  - Core concepts: accounts, ownership, claiming, generating via a wallet vs a PDA, etc. Instructions, pre&post states. Diagrams would be nice (or partially delegate to Solana account model docs)
  - Almost every line should be commented, assuming the reader does not have any pre-existing knowledge on the tech - primarily about the stack, but maybe even some light Rust syntax help.
  - Compilation process explanation, local testing guide (when available)
  - Private vs public execution, ZK basics & what private execution means for the developer/user, delegating documentation about ZK for further reading
- Deploying & interacting with NSSA programs
  - This is the culmination of the items above. The user now has a properly set-up environment, knows the basics, has the right tools installed and running, and is now ready to start deploying and using the programs on a live chain.
  - `How to debug programs` OR `Common beginner errors and how to fix them`- this was a piece I was crucially missing from the docs. A lot of the times something went wrong with my program code, and all I could do was reset the whole state, recompile, push, try interacting again, only to fail a second, maybe even a third time. This process was tedious and impractical, due to multiple steps that almost always took time (ie recompilation of programs). A list of common (known) issues and their fixes will greatly improve developer experience, make people understand the system faster, and help them be less frustrated while learning.
- More advanced patterns:
  - Using PDAs
  - Tail Calls
  - Complex programs (ie AMM)
  - ...
