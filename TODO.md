# TODO

- [ ] Stress-test
- [ ] Stress-test over bad network connections
- [ ] Allow any node that has a public IP +port to operate as a relay
- [ ] Replace iroh builtin relay with random relay selection
- [ ] data server should auth people when they connect as if they're part of the run.
- [ ] p2p / centralized coordinator backend.
- [ ] add a version byte to network messsages :)

```
solana/
  coordinator/ # bin (smart contract).
  common/ # lib. structs for data server <-> client. state reader that reads chain.
  data-server/ # bin. pulls in data-server crate, etc, and common/state reader. provides data to client.
  client/ # bin. pulls in network for p2p, common for chain state and data structs. connects to data server too.
centralized/
  state-reader/
  server/
  client/
```