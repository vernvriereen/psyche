# this file contains secrets that we can store encrypted in this repo.
# they can be decrypted by the specified ssh public keys using `agenix`.
let
  keys = import ./keys.nix;
in
{
  "secrets/docs-http-basic.age".publicKeys = keys.allKeys;
  "secrets/backend-devnet.age".publicKeys = keys.allKeys;
  "secrets/backend-mainnet.age".publicKeys = keys.allKeys;
}
