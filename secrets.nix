# this file contains secrets that we can store encrypted in this repo.
# they can be decrypted by the specified ssh public keys using `agenix`.
let
  keys = import ./nix/keys.nix;
in
{
  "secrets/docs-http-basic.age".publicKeys = keys.allKeys;
  "secrets/devnet/backend.age".publicKeys = keys.allKeys;
  "secrets/backend-mainnet.age".publicKeys = keys.allKeys;

  # a shared devnet wallet
  "secrets/devnet/keypair/wallet.age".publicKeys = keys.allDevKeys;
}
