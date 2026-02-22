## Documentation feedback

First, we need to ask ourselves what kind of persona is reading the docs:

- Are they a developer?
  - If yes, what kind?
    - Web3, non-Web3 (understanding of basic blockchain concepts)
    - Which langauge(s) do they know? Rust -> a lot easier
    - Do they come from Ethereum, Solana, Cosmos...
    - Are they familiar with the Solana tech stack, account model, etc?
- Are they looking to run a node/blockchain infra?
  - Different approach, probably not supposed to be in this repo, but another one (or another docs section) - I also assume this is not a priority right now.

I approached this as a Web3 developer with very little Rust & Solana experience. I needed to learn the basics of the account model, instructions, & PDAs, while also managing my way around an unfamiliar langauge syntax.

Missing clickable links to resources, repos. Many opportunities to delegate documentation work to other resources - ie, if the system is based on the account model, why not link back to the original Solana documentation which is actively maintained and a great resource to get started?

I could not find information about how exactly the account model is applied to the stack - ie, is it fully compatible with Solana, are there some things that work there but not here, etc?

- For this, a comparison list between differences would be a good resource and time-saver for newcomers. This can also be done via code-first examples, showcasing differences and similarities between a LEE and Solana program with the same or similar business logic
