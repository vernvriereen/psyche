default:
  just --list

# build & test & check format
check:
	nix flake check

# format & lint-fix code
fmt:
	cargo clippy --fix --allow-staged --all-targets
	cargo fmt
	alejandra .

# build the centralized client Docker image
docker-build-centralized-client:
	nix build .#stream-docker-psyche-centralized-client --out-link nix-results/stream-docker-psyche-centralized-client
	nix-results/stream-docker-psyche-centralized-client | docker load

# build & push the centralized client Docker image
docker-push-centralized-client: docker-build-centralized-client
	docker push docker.io/nousresearch/psyche-centralized-client

# spin up a local testnet
local-testnet +args:
	cargo run -p psyche-centralized-local-testnet -- start {{args}}

# run integration tests
integration-test test_name="":
    if [ "{{test_name}}" = "" ]; then \
        cargo test --release --test integration_tests; \
    else \
        cargo test --release --test integration_tests -- --nocapture "{{test_name}}"; \
    fi

# Deploy coordinator on localnet and create a "test" run for 1.1b model.
setup-solana-localnet-test-run run_id="test":
    RUN_ID={{run_id}} ./scripts/deploy-solana-test.sh

# Deploy coordinator on localnet and create a "test" run for 20m model.
setup-solana-localnet-light-test-run run_id="test":
    RUN_ID={{run_id}} CONFIG_FILE=./config/solana-test/light-config.toml ./scripts/deploy-solana-test.sh

# Start client for training on localnet.
start-training-localnet-client run_id="test":
    RUN_ID={{run_id}} ./scripts/train-solana-test.sh

# Start client for training on localnet without data parallelism features and using light model.
start-training-localnet-light-client run_id="test":
    RUN_ID={{run_id}} DP=1 ./scripts/train-solana-test.sh

DEVNET_RPC:="https://api.devnet.solana.com"
DEVNET_WS_RPC:="wss://api.devnet.solana.com"

# Deploy coordinator on Devnet and create a "test" run for 1.1b model.
setup-solana-devnet-test-run run_id="test":
    RUN_ID={{run_id}} RPC={{DEVNET_RPC}} WS_RPC={{DEVNET_WS_RPC}} ./scripts/deploy-solana-test.sh

# Deploy coordinator on Devnet and create a "test" run for 20m model.
setup-solana-devnet-light-test-run run_id="test":
    RUN_ID={{run_id}} RPC={{DEVNET_RPC}} WS_RPC={{DEVNET_WS_RPC}} CONFIG_FILE=./config/solana-test/light-config.toml ./scripts/deploy-solana-test.sh

# Start client for training on Devnet.
start-training-devnet-client run_id="test":
    RUN_ID={{run_id}} RPC={{DEVNET_RPC}} WS_RPC={{DEVNET_WS_RPC}} ./scripts/train-solana-test.sh

# Start client for training on localnet without data parallelism features and using light model.
start-training-devnet-light-client run_id="test":
    RUN_ID={{run_id}} RPC={{DEVNET_RPC}} WS_RPC={{DEVNET_WS_RPC}} DP=1 ./scripts/train-solana-test.sh

solana-client-tests:
	cargo test --package psyche-solana-client --features solana-localnet-tests

# install deps for building mdbook
book_deps:
	cargo install mdbook mdbook-mermaid

build_book output-dir="../book": generate_cli_docs
	mdbook build psyche-book -d {{output-dir}}

# run an interactive development server for psyche-book
serve_book: generate_cli_docs
	mdbook serve psyche-book --open

generate_cli_docs:
    echo "generating CLI --help outputs for mdbook..."
    mkdir -p psyche-book/generated/cli/
    cargo run -p psyche-centralized-client print-all-help --markdown > psyche-book/generated/cli/psyche-centralized-client.md
    cargo run -p psyche-centralized-server print-all-help --markdown > psyche-book/generated/cli/psyche-centralized-server.md
    cargo run -p psyche-centralized-local-testnet print-all-help --markdown > psyche-book/generated/cli/psyche-centralized-local-testnet.md

# Setup the infrastructure for testing locally using Docker.
setup_test_infra num_clients="1":
    cd docker/test && NUM_REPLICAS={{num_clients}} docker compose up -d --force-recreate

# Run a client to start training inside a Docker container.
run_docker_client:
    docker build -t nous-base -f docker/nous_base.Dockerfile .
    docker build -t psyche-client -f docker/Dockerfile .
    docker run --rm \
      --gpus all \
      -e NVIDIA_DRIVER_CAPABILITIES=all \
      --add-host host.docker.internal:host-gateway \
      -v ~/.config/solana/id.json:/usr/local/id.json \
      --env-file config/client/.env \
      psyche-client
