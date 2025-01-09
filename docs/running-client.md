# Running a client

The client is the one responsible for joining and participating in a run, training the model and sharing the results to the other peers in order to fully train a model. The client is a cli app with different configurations.

First we want to install the client cli, for this we can just run:

```bash
just install
```

If everything went well you should be able to check the diffent commands that we can use with the client

```bash
psyche-centralized-client --help

Usage: psyche-centralized-client <COMMAND>

Commands:
  show-identity
  train
  help           Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
```

The client consists basically in two diffent commands, **show-identity** and **train**.

The **show-identity** command we will tell us our personal id, that will identify us when we participate in the run with other clients.
First we have to generate a key file (TODO: how to actually do this).

Once that's done now we can use the path to that file to get our id.

```bash
psyche-centralized-client show-identity --identity-secret-key-path <path_to_key_file>
```

TODO: Show output

The real command is the **train** command, this basically allows the client to join a run of training and participate in the training of a model. To run this we basically need two things:
- `run-id` This is the id that represents a run of training, is unique for every run and allows the client to join and participate any live run.
- `server-addr` This is the address where the server hosting the run is up.

```bash
psyche-centralized-client train --run-id <RUN_ID> --server-addr <SERVER_ADDR>
```

There's also a lot of optional arguments that we can set for the client, if we want a little more control on the behavior, you can see all the optional arguments running:

```bash
psyche-centralized-client train --help
```

But let's do a quick explanation for all of them:
- `identity-secret-key-path` Just as the show-iddentity commands, this creates the client with a specific key file.
- `bind-p2p-port` The client participates in a p2p network with the other clients, this way can choose the port for that p2p network.
- `tui` A graphical interface in the terminal to navigate the different anylitics for the client.
- `data-parallelism` TODO
- `tensor-parallelism` TODO
- `micro-batch-size` TODO
- `write-gradients-dir` A directory where the clients will store all the gradients seen in the run
- `eval-tasks` tasks to test the model in the middle of the training
- `eval-fewshots` TODO
- `eval-seed` TODO
- `eval-test-max-docs` TODO
- `checkpoint-dir` TODO
- `hub-repo` A path to the huggingface repository containing data and config for the model.
- `write-log` TODO
- `optim-stats-steps` TODO
- `grad-accum-in-fp32` TODO
- `dummy-training-delay-secs` TODO

Also there's a few estra arguments related to `wandb`to actually upload the training results of the run
- `wandb-project`
- `wandb-run`
- `wandb-group`
- `wandb-entity`
