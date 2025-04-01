#!/bin/bash

_usage() {
    echo "Usage: $0 <SOLANA_RPC> <GRANTOR_KEYPAIR_FILE> <GRANTEE_PUBKEY>"
    echo "  SOLANA_RPC: The solana RPC url or moniker to use"
    echo "  GRANTOR_KEYPAIR_FILE: The keypair file of the authority that is issuing the authorization"
    echo "  GRANTEE_PUBKEY: The pubkey that is the receiver of the authorization"
    exit 1
}

if [[ "$#" -lt 3 ]]; then
    _usage
fi

SOLANA_RPC="$1"
shift

GRANTOR_KEYPAIR_FILE="$1"
shift

if [[ ! -f "$GRANTOR_KEYPAIR_FILE" ]]; then
  echo "Error: Grantor keypair file '$GRANTOR_KEYPAIR_FILE' not found."
  _usage
fi
GRANTOR_PUBKEY=$(solana-keygen pubkey $GRANTOR_KEYPAIR_FILE)

GRANTEE_PUBKEY="$1"
shift

PSYCHE_AUTHORIZER_ID="PsyAUmhpmiUouWsnJdNGFSX8vZ6rWjXjgDPHsgqPGyw"
PSYCHE_AUTH_SCOPE="utf8:CoordinatorJoinRun"

# Make sure all is good to go
echo "SOLANA_RPC: $SOLANA_RPC"
echo "GRANTOR_KEYPAIR_FILE: $GRANTOR_KEYPAIR_FILE"
echo "GRANTOR_PUBKEY: $GRANTOR_PUBKEY"
echo "GRANTEE_PUBKEY: $GRANTEE_PUBKEY"
echo "PSYCHE_AUTHORIZER_ID: $PSYCHE_AUTHORIZER_ID"
echo "PSYCHE_AUTH_SCOPE: $PSYCHE_AUTH_SCOPE"

# Create a new authorization and save the created PDA's address
AUTHORIZATION_PDA=$(\
    solana-toolbox --rpc=$SOLANA_RPC instruction \
        $PSYCHE_AUTHORIZER_ID authorization_create \
        payer:$GRANTOR_KEYPAIR_FILE \
        grantor:$GRANTOR_KEYPAIR_FILE \
        --args=params.grantee:$GRANTEE_PUBKEY \
        --args=params.scope:$PSYCHE_AUTH_SCOPE \
        --execute \
    | jq .resolved.addresses.authorization \
)
echo "AUTHORIZATION_PDA: $AUTHORIZATION_PDA"

# Activate the new authorization we just created
echo "----"
echo "Activation of the newly created authorization..."
solana-toolbox --rpc=$SOLANA_RPC instruction \
    $PSYCHE_AUTHORIZER_ID authorization_grantor_update \
    --args=params.active:true \
    grantor:$GRANTOR_KEYPAIR_FILE \
    authorization:$AUTHORIZATION_PDA \
    --execute | jq -r .outcome.explorer
echo "----"
