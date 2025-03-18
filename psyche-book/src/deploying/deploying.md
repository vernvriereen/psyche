# deploying

## devnet

if you're developing things that change the structure of the contract,
things will break terribly unless you write nice migrations,
and the indexer will be upset too if you use new IDL with an old shape.
so, you might need to deploy changes to devnet with a totally new coordinator.

to deploy a program to devnet with a new program keypair,
regenerate its devnet/localnet keypair file (checked into the repo!)

for the solana coordinator, that would be

```bash
solana-keygen new -o architectures/decentralized/solana-coordinator/target/deploy/psyche_solana_coordinator-keypair.json -f
```

you can get the new program ID by running

```bash
solana-keygen pubkey architectures/decentralized/solana-coordinator/target/deploy/psyche_solana_coordinator-keypair.json
```

since you've now got a new program id, you'll need to change this in the indexer's data.

places to change this:

- backend/package.json
- inside the agenix secret file for devnet (see [using agenix](./agenix.md))
