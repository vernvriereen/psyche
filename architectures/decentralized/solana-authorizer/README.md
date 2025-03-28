# Example usages

Note: We'll use the `jq` library in this example which is an open-source JSON cli tool <https://jqlang.org/>

Note: We'll also use the `solana-toolbox` from the `cargo install solana_toolbox_cli` rust crate

## Setting up an authorized user from the authority

Here is an example workflow to create and activate an authorization

```sh
# Deployed authorizer
PSYCHE_AUTHORIZER_ID="PsyAUmhpmiUouWsnJdNGFSX8vZ6rWjXjgDPHsgqPGyw"
# We need to define the SCOPE of our authorization (what it's for)
PSYCHE_AUTH_SCOPE="{utf8:\"CoordinatorJoinRun\"}"

# Assuming we have the authority.json keypair in the current folder
# Our authority will be our authorization GRANTOR
GRANTOR_KEYPAIR=authority.json
# Assuming we know the pubkey of the authorized user somehow
# Our authorized user will be our authorization GRANTEE
GRANTEE_PUBKEY=\"$(solana-keygen pubkey user.json)\"

# Create a new authorization and save the created PDA's address:
AUTHORIZATION_PDA=$(\
    solana-toolbox --rpc=devnet instruction \
        $PSYCHE_AUTHORIZER_ID authorization_create \
        "{params:{grantee:$GRANTEE_PUBKEY,scope:$PSYCHE_AUTH_SCOPE}}" \
        grantor:$GRANTOR_KEYPAIR \
        payer:keypair --execute \
    | jq .resolved.addresses.authorization \
)

# Activate the new authorization we just created (or deactivate it by flipping the flag to false)
solana-toolbox --rpc=devnet instruction \
    $PSYCHE_AUTHORIZER_ID authorization_grantor_update \
    "{params:{active:true}}" \
    grantor:$GRANTOR_KEYPAIR \
    authorization:$AUTHORIZATION_PDA \
    --execute
```

## Setting up delegates from the authorized user

Here is an example from the user's perspective to add delegates for its own master key

```sh
# Deployed authorizer
PSYCHE_AUTHORIZER_ID="PsyAUmhpmiUouWsnJdNGFSX8vZ6rWjXjgDPHsgqPGyw"
# We need to define the SCOPE of our authorization (what it's for)
PSYCHE_AUTH_SCOPE="{utf8:\"CoordinatorJoinRun\"}"

# We must know who granted the authorization somehow (replace here)
GRANTOR_PUBKEY=\"$(solana-keygen pubkey authority.json)\"
# We must have access to our user's master key
GRANTEE_KEYPAIR=user.json
GRANTEE_PUBKEY=\"$(solana-keygen pubkey user.json)\"

# Find the authorization PDA that should have already been created for us
AUTHORIZATION_PDA=$(\
    solana-toolbox --rpc=devnet instruction \
    $PSYCHE_AUTHORIZER_ID authorization_create \
    "{params:{grantee:$GRANTEE_PUBKEY,scope:$PSYCHE_AUTH_SCOPE}}" \
    grantor:$GRANTOR_PUBKEY \
    | jq .resolved.addresses.authorization \
)

# We can then create add a new delegate keypair
solana-keygen new -o delegate.json --no-bip39-passphrase
DELEGATE_PUBKEY=\"$(solana-keygen pubkey delegate.json)\"

# Then we add the delegate key to our list of delegates
solana-toolbox --rpc=devnet instruction \
    $PSYCHE_AUTHORIZER_ID authorization_grantee_update \
    "{params:{delegates_clear:false,delegates_added:[$DELEGATE_PUBKEY]}}" \
    authorization:$AUTHORIZATION_PDA \
    grantee:$GRANTEE_KEYPAIR \
    payer:keypair --execute
```
