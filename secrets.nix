# this file contains secrets that we can store encrypted in this repo.
# they can be decrypted by the specified ssh public keys using `agenix`.
let
  keys = import ./nix/keys.nix;
in
{
  # http basic auth for our test deployments
  "secrets/docs-http-basic.age".publicKeys = keys.allKeys;

  # RPC urls for our devnet indexer
  "secrets/devnet/backend.age".publicKeys = keys.allKeys;

  # RPC urls for our mainnet indexer
  "secrets/backend-mainnet.age".publicKeys = keys.allKeys;

  # a shared devnet wallet
  "secrets/devnet/keypair/wallet.age".publicKeys = keys.allDevKeys;
}
