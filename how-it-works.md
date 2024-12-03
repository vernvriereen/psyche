# how it works

training round starts:

- apply prev round results
- everyone grabs samples, trains, emits broadcast messages, starts downloading from other people

opportunistic witness:

- all batches finished deserializing from previous round
- i'm done all my training data
- send it!

once witness quorum is reached, coordinator to adv. to witness round:

- set a point in time where everyone is "done" and has seen everything
- has a "slack" time for every non-witness to get their shit together, download payloads, finish witnessing, etc
- if witness_size is 0, all active clients are witnesses (refactor to enum for this)
- witness_size could be a # or a %
- witness_quorum:
  - for every witness, how many of the witnesses have to vote for the same thing to make that be the truth
  - if 0, quorum = witness_size (refactor to enum for this)

once witness round finishes:

- back to training
