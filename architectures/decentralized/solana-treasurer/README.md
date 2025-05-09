# Psyche Solana Treasurer

This smart contract provides an Incentive layer on top of the Psyche's coordinator program.

## How it works

The `Treasurer` account can be created by an authority, and creating the `Treasurer` will automatically creates a training `Run` owned by the `Treasurer` smart contract.

The underlying `Run` can then be configured as normal indirectly, through using the `Treasurer` directly.

The coordinator's `Run` can then be interacted with normally for computing and training and used normally by the compute providers.

A set of reward tokens can then be deposited inside of the `Treasurer` for fair distribution during the training of the underlying `Run`.

Once a client has earned compute points in the underlying `Run`, that same client can then claim to have participated in the run by creating a `Participant` account on the `Treasurer`.

Once that client has earned enough points and once the reward token has been deposited into the treasury, the user can the directly withdraw the reward tokens to its wallet. (The reward rate can be configured in the `Run` itself)

## Solana Instructions

To achieve this the smart contract provides the following capabilities:

- `run_create`, Create a normal `Run` owned by the `Treasurer`
- `run_top_up`, Deposit reward tokens to be distributed later to compute providers
- `run_update`, Configure the underlying `Run`'s Psyche coordinator
- `participant_create`, Must be called before a user can claim reward tokens
- `participant_claim`, Once a user earned points on the `Run`'s coordinator, the user can withdraw the reward tokens proportional share of the `Run`'s treasury
