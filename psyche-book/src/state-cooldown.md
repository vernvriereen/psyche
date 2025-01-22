# Cooldown State

The Cooldown state is the last state of an epoch, during which the system waits for either the cooldown time to be over,
or a checkpoint to have happened, to transition to the next epoch.

## Deep Dive

The server transitions from Witness to Cooldown state on `tick_round_witness` in three cases:
- If we are in the last round of the epoch.
- If the clients have dropped to less than the minimum required by the config.
- If the number of witnesses for the round is less than the quorum specified by the config.

The first function to be executed will be `start_cooldown` which:
1. If the current checkpoint is of type `Checkpoint::Hub` (this will happen at the beginning, before anything has been shared):
    - Sets the checkpoint to a `Checkpoint::P2P`
2. Sets the current state to `RunState::Cooldown`

Next, in `tick_cooldown`, we tick until one of this conditions is met:
- The cooldown time has passed (only if `cooldown_time` is set to a value greater than 0 in the config).
- A checkpoint has occurred.

If either of these conditions have been met, the following happens:
1. Saves the current epoch state into `prev_epoch`
2. Advances the epoch (including updating the start data index)
3. Starts the state `WaitingForMembers`
4. Returns a `TickResult::EpochEnd(true)`.

### How does the server know a checkpoint has occurred?
#### Client
1. The client watches for state changes in the server and reacts appropriately in the state machine.
2. In the client's step machine, in the function `RunManager::apply_state`, the client will execute `CooldownStepMetadata::start` if it was in `ActiveStep::Witness` and received a transition to `RunsState::Cooldown`.
3. In `CooldownStepMetadata::start` if the `CheckpointConfig` is valid:
    1. Extracts the data from the trainer (`Trainer::extract`)
    2. Runs the evals (`EvalRunner::start`)
    3. If we have valid Hub-upload information:
        - We upload the data to the hub via `upload_model_repo_async` , we save the commit id as `revision` and we send the message including it via `tx_checkpoint`
4. In `Client::new` the client listens in `rx_checkpoint` and if has received a `Checkpoint` object it calls `Backend::send_checkpoint` which sends a `ToSend::Checkpoint` message to itself.
5. In `App::run` the client receives `ToSend::Checkpoint` and sends the `ClientToServerMessage::Checkpoint` to the server.

#### Server
When the server receives a message from client of type `ClientToServerMessage::Checkpoint` (`App::on_client_message`)  it calls `Coordinator::checkpoint`, this function returns an error if any of these things happen:
1. The `run_state` is not `Cooldown`.
2. A checkpoint for the current epoch already exists.
3. The sender is not in the list of allowed checkpointers.

If none of these conditions are met then the function continues the happy path: it sets the checkpoint for the current model and sets `epoch_state.checkpointed = true`.
In the next `tick_cooldown` it will execute the code previously described.
