# LEE Documentation feedback

This document contains feedback on the developer onboarding process & documentation found in the LEE repo.

## 1. Target Audience & Personas

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

## 2. Using Existing Resources

While reading, I was often missing clickable links to resources, repos, folders, etc. There were many opportunities to delegate documentation work to other resources - ie, if the system is based on the account model, linking back to the Solana documentation, an actively maintained resource, is a great way to offload work and help developers onboard.

Furthermore, whenever new concepts/terminology is introduced, linking to a resource is a great way to help the user not feel confused or question themselves if they should know about the term already. It helps helps prevent confusion and self-doubt (`should I already know this?`). For instance, the concept of a `witness` was introduced without explanation or reference.

We could:

- Link to Solana account model documentation where concepts overlap
- Provide analogies or "bridging explanations” when terminology differs
- Add a comparison section: “How this model differs from Solana” where applicable

This can also be done via code-first examples, showcasing differences and similarities between a LEE and Solana program with the same or similar business logic.

## 3. Environment and Services Setup & Understanding

Ideally a brief intro into what the dev environment is comprised of should be the first thing the reader sees:

- Indexer
- Sequencer
- Node
- Explorer

Currently, it's expected that the reader to understand what fits where and why. I suggest creating a list of items needed, and giving a brief overview of what's what. This also provides the reader with context on the architecture of the system and all of the moving parts.

The explorer was not mentioned explicitly in any of the suggested docs. This was a major minus for me as I believe that being able to see and track what you did during the development process, apart from node & sequencer logs, is vital.

- Running the `explorer_serivce` via docker builds and runs properly, but displays a 404 error upon opening up `localhost:8080`. I managed to get `cargo leptos watch` working after a few runs. Even after that, I repeatedly encountered ‘Cannot reach RPC’ errors without any info on why they might be happening - in spite of the indexer port being set properly and the chain running smoothly.
- The sequencer was constantly giving a large amount of WARN lines such as `Failed to resubmit block with id X: Item already in mempool` even with a clean chain state. I am not sure what is causing this, but it was filling my logs and making them hard to read.

Apart from the brief intro to required environment components, the docs should:

- Provide a clear table of default endpoints for each service(node, sequencer, explorer, RPC, etc.)
- Add a “Health Check” section after setup:
  - Which logs should look “healthy" for each service
  - Which warnings are expected vs. problematic
- Finally, ideally include a short troubleshooting guide for common connection issues (RPC unreachable, explorer 404, etc.)

## 4. Proposed Documentation Structure

I believe both the root `README.md` & `examples/program_deployment/README.md` are too long and should be cut into smaller, comprehensive chunks. Currently, the documents are a bit overwhelming and it's easy to get lost in them.

Ideally, I would move docs into a separate folder, and partition them as follows:

### Vision & Tech Stack

- What is Logos Blockchain/LEE, Why should anyone care?
- What is NSSA, what the repo is. What tech it's based on, what the goal of the project is.
- Main practical functionalities and possibilities of the tech
- Brief overview of repo structure, as it's the main local development environment devs will use

### Local Environment Setup:

- Blockchain node, indexer, sequencer, explorer service
- Wallet
- Health-check everything -> ready to interact with the blockchain.

### First Interaction (Wallet & Chain)

- Preconfigured wallets, how to preconfigure your own (ie is there a config file somewhere that can be edited at this stage of tech?) since there is no easy airdrop/premine process?
- Simple transaction submission via CLI, ie a balance transfer via the wallet
- Exploring actions/blocks on the explorer, helping the developer see what they did outside of only logs

### Core Concepts & Programs

- Accounts, ownership & claiming, generating via a wallet vs a PDA, etc.
- Instructions, pre&post states. Diagrams would be nice (or partially delegate to Solana account model docs).
- Private vs public execution, ZK basics & what private execution means for the developer/user, delegating documentation about ZK for further reading
- Compilation process explanation, local testing guide (when it becomes an available feature)

### Deploying & interacting with NSSA programs

This is the culmination of the items above. The user now has a properly set-up environment, knows the basics, has the right tools installed and running, and is now ready to start deploying and using the programs on a live chain. Now:

- Jump into practical code examples, going from the simplest, to slowly introducing concepts talked about in the previous section (Core Concepts & Programs)

During the initial examples, almost every code line should be commented, while assuming the reader does not have any pre-existing knowledge on the tech - primarily about the stack, but maybe even some light Rust syntax help could benefit the reader.

### Debugging & Common Errors

Include a document such as `How to debug programs` OR `Common beginner errors and how to fix them`.

This was a piece I was crucially missing from the docs. A lot of the times something went wrong with my program code, and all I could do was reset the whole state, recompile, push, try interacting again, only to fail a second, maybe even a third time.

This process was tedious and impractical, due to multiple steps that almost always took time (ie recompilation of programs). A list of common (known) issues and their fixes will greatly improve developer experience, make people understand the system faster, and help them be less frustrated while learning.

### Advanced Patterns:

- Using PDAs
- Tail Calls
- Complex programs (ie AMM)
- ...

## 5. Wallet UX Issues

- The name `wallet` is quite generic; a more distinctive name might help - ie `leew`, `lssew` `logoswallet`, etc.
- Naming accounts should be a default during account creation
- Plain-text password input should be hidden
- Some simple docs or a readme in `wallet/` would be useful for developers familiar with wallet tools:
  - How does account generation work
  - Where is configuration data stored
  - Which accounts are preconfigured/premined (if there are any)?
- If there are pre-installed programs to interact with in the wallet, some info in the CLI or docs/links would be nice as to what they are
- More feedback after running actions in the wallet would be helpful - ie when deploying a program - I had to chase sequencer logs to see if there was a transaction on the chain and the program was deployed properly. Ideally a program ID or address would be displayed after deployment, possibly also to be used for searching for the program via the explorer.

## 6. Final Thoughts

Currently, the documentation seems like it's more oriented towards existing contributors that understand core concepts than to newcomers. This is understandable as the project is in an early phase, but creating easy to follow documentation will enable easier onboarding, potentially attracting more and more open-source contributors which could accelerate project development.

If we're looking to expand the contributor base, some sort of online community should be created so people can reach out with beginner questions, apart from GitHub itself. I had the privilege of being in a Signal group with the engineers behind the project, but most newcomers probably do not, which is a drawback.

Creating video content on the core concepts & development & setting up short, weekly office hours could greatly benefit the project.
