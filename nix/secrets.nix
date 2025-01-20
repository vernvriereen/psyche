# this file contains secrets that we can store encrypted in this repo.
# they can be decrypted by the specified ssh public keys using `agenix`.
let
  # from garnix, a unique ssh key for this repo
  repoKey = "age1qjv2zfrgwdhc0pw3t2rqk0zv6zudwqjjdz8plmy9u2edyxm77crqr73uqj";

  ariLunaKey = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIIL+5IDeIKvYpQllVsU/soRu27KyPTA5FXvZM5Z8+ms7 arilotter@gmail.com";
  ariHermesKey = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIMKaPWTrDp1sp3NUXiM/JXKfivQQ6TLxMy7Fyaq59L7y arilotter@gmail.com";
  ariKeys = [ariHermesKey ariLunaKey];

  allKeys = [repoKey] ++ ariKeys;
in {
  "secrets/docs-http-basic.age".publicKeys = allKeys;
}
