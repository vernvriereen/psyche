# how it all works

i don't want to explain it rn bug @arilotter if you need to

## secrets

read about em here: https://garnix.io/docs/hosting/secrets
uses agenix!

we store caddy's http basic auth creds as an agenix secret.

### editing a secret

you must have your pubkey listed in `secrets.nix` for that secret if you want to modify the existing one!

ask someone whose key is in `secrets.nix` to be added.

e.g. `agenix -e secrets/docs-http-basic.age`
