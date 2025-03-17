# Running Psyche on-chain

To build the Solana programs, install all required Solana tools (see [the setup](./setup.md) if you're not using Nix).

To start, you'll need to create a Solana wallet to fund your transactions.

```bash
solana-keygen new
```

## Localnet

In a new terminal, run a validator with:

```bash
solana-test-validator
```

Deploy all the required programs and create a local run using:

```bash
just setup-solana-localnet-test-run
```

And run a client to train the test model using:

```bash
just start-training-client
```

This will start a run to train a 1.1b parameter model with all the parallelism features enabled.
For a more lightweight run to avoid OOM errors, or just to use your hardware less, (we see you 8gb VRAM cards!) there's also:

```bash
just setup-solana-localnet-light-test-run
just start-training-light-client
```

## Devnet

You'll need to fund your wallet to make transactions on Devnet.
You can [request an airdrop](https://faucet.solana.com/) from the Solana foundation of up to 10 devnet sol every 8 hours. Simply run

```bash
solana-keygen pubkey
```

and paste the resulting key into the airdrop website.

You can then use the same steps for deploying the programs, creating a run, and training on localnet above, but using following `just` commands:

```bash
just setup-solana-devnet-test-run
just start-training-devnet-client
```

alongside the `-light` variants

```bash
just setup-solana-devnet-light-test-run
just start-training-devnet-light-client
```

## regenerating program keypairs

if you're developing things that change the structure of the programs,
things will break terribly unless you write nice migrations,
and the indexer will be upset too if you use new IDL with an old shape.
so, you might need to deploy changes to devnet or localnet with a totally new coordinator.

additionally, if you want to test deploying the programs yourself, and you don't have the keys required to do so, you won't be able to deploy at these addresses (since you don't have permission, and the programs are already there!)

these keypairs are only used to derive the program ID of a program.

to deploy a program to devnet or localnet _with a new program keypair_,
regenerate its devnet/localnet keypair file (checked into the repo!)

for the solana coordinator, that would be

```bash
solana-keygen new -o architectures/decentralized/solana-coordinator/target/deploy/psyche_solana_coordinator-keypair.json -f
```

you can see the newly generated program ID by running

```bash
solana-keygen pubkey architectures/decentralized/solana-coordinator/target/deploy/psyche_solana_coordinator-keypair.json
```

if you want to push these changes to the repo, you'll need to use `git add -f`, since they're normally `.gitignore`d.
