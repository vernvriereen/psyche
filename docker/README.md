# Docker Psyche

This folder contains all the docker related files and scripts.
The purpose of using docker is two-fold:
  * compartmentalize psyche client to be deployed and used in testing and production environments easily.
  * implementing end-to-end tests that are as close as possible to a production environment.

There are three concrete use-cases for the docker containers that are generated with these Dockerfiles:
  * spawning a whole dockerized network with all the components: Solana validator and varios clients. These should
  be done via `docker compose`.
  * booting a testing client in a Solana localnet: basically to join and train in a run with some local or remote
  `solana-test-validator`. In short, it solves some Solana chores such as generating a key pair and adding funds to it.
  * booting a production client: these can be used either on the Solana devnet or mainnet. There is no automatic
  management of Solana keys or funds. This should be set manually by the user.

## Dockerfiles

### Psyche base dockerfile

The `psyche_base.Dockerfile` is used for building the base image which will be used by almost all
the other docker images.
This image collects all Linux and Rust dependencies needed to build the `psyche-solana-client` binary.
It uses [cargo-chef](https://github.com/LukeMathWalker/cargo-chef), an
utility for caching Rust dependencies for Docker builds.

## Psyche Solana client

The `psyche_client.Dockerfile` is the dockerfile used to build the image for the client that would be used by
end users, in a production-like environment.
In essence, the image is built installing some basic OS dependencies and then copying the client binary from
the `psyche-base` image.
The `client_entrypoint.sh` script runs as the default entrypoint for the container, which is no more than a
call to the `psyche-solana-client` binary to start training.

## Psyche Solana test client

The `psyche_test_client.Dockerfile` is essentially the same as the `psyche_client.Dockerfile`, but adds a thin
layer of Solana dev tools like the Solana CLI to make things a little easier in testing scenarios.
The `client_test_entrypoint.sh` script runs as the default entrypoint, which basically generates a Solana key pair,
deposits some funds in it and then starts training. This is done only for convenience in testin scenarios.

## Psyche Solana test validator

The `psyche_solana_validator.Dockerfile` is meant to build a Solana validator image which will be only used for testing
when spawning a whole network using `docker compose`.
Its main task is to spawn the `solana-test-validator` and deploy the coordinator Solana program.

---

## Running a localnet using Pysche test client

If you want to running a validator in your local machine,
