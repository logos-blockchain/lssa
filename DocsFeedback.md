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

I approached this as a Web3 developer with very little Rust & Solana experience. I needed to learn the basics of the account model, instructions, & PDAs, while also managing my way around an unfamiliar langauge syntax.

Missing clickable links to resources, repos. Many opportunities to delegate documentation work to other resources - ie, if the system is based on the account model, why not link back to the original Solana documentation which is actively maintained and a great resource to get started?

I could not find information about how exactly the account model is applied to the stack - ie, is it fully compatible with Solana, are there some things that work there but not here, etc?

- For this, a comparison list between differences would be a good resource and time-saver for newcomers. This can also be done via code-first examples, showcasing differences and similarities between a LEE and Solana program with the same or similar business logic

I found it hard to explore my actions. Running the `explorer_serivce` via docker builds and runs properly, but displays a 404 error upon opening up `localhost:8080`. `cargo leptos watch` came back with an error:

```
❯ cargo leptos watch
   Compiling proc-macro2 v1.0.103
   Compiling quote v1.0.42
   Compiling unicode-ident v1.0.22
   Compiling version_check v0.9.5
   Compiling wasm-bindgen-shared v0.2.106
   Compiling libc v0.2.178
   Compiling serde_core v1.0.228
   Compiling semver v1.0.27
   Compiling cfg-if v1.0.4
error[E0463]: can't find crate for `core`
```

The reason for the explorer service

The sequencer was constantly giving a large amount of WARN lines, ie

```
[2026-02-22T20:17:32Z WARN  sequencer_runner] Failed to resubmit block with id 1 with error Internal server error: Item already in mempool
[2026-02-22T20:17:32Z WARN  sequencer_runner] Failed to resubmit block with id 2 with error Internal server error: Item already in mempool
[2026-02-22T20:17:32Z WARN  sequencer_runner] Failed to resubmit block with id 3 with error Internal server error: Item already in mempool
[2026-02-22T20:17:32Z WARN  sequencer_runner] Failed to resubmit block with id 4 with error Internal server error: Item already in mempool
[2026-02-22T20:17:32Z WARN  sequencer_runner] Failed to resubmit block with id 5 with error Internal server error: Item already in mempoo
```

even with a clean state. Not sure what is causing this.

I got no feedback from the wallet when deploying a program - had to chase sequencer logs to see if the program was deployed properly. Ideally a program ID or address or similar would be displayed, possibly also to be used for searching for the program via the explorer

## Wallet Issues

- Name could be less generic - ie `leew`, `lssew` `logoswallet`
- Naming accounts by default during account creation
- Plain-text password input should be hidden
- Some simple docs/readme would be nice:
    - How does account generation work
    - Config data
    - Which accounts are preconfigured/premined?
- If we have pre-installed programs to interact with in the wallet, some info in the CLI or docs/links would be nice as to what they are

