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
        cargo test --release -p psyche-centralized-testing --test integration_tests; \
    else \
        cargo test --release -p psyche-centralized-testing --test integration_tests -- --nocapture "{{test_name}}"; \
    fi

# run integration decentralized tests
decentralized-integration-test test_name="":
    if [ "{{test_name}}" = "" ]; then \
        cargo test --release -p psyche-decentralized-testing --test integration_tests -- --nocapture; \
    else \
        cargo test --release -p psyche-decentralized-testing --test integration_tests -- --nocapture "{{test_name}}"; \
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
	cargo install mdbook mdbook-mermaid mdbook-linkcheck

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

build_docker_test_client:
    ./scripts/coordinator-address-check.sh
    docker build -t psyche-base -f docker/psyche_base.Dockerfile .
    docker build -t psyche-test-client -f docker/test/psyche_test_client.Dockerfile .

# Setup the infrastructure for testing locally using Docker.
setup_test_infra num_clients="1":
    cd architectures/decentralized/solana-coordinator && anchor keys sync && anchor build --no-idl
    cd docker/test && docker compose build
    cd docker/test && NUM_REPLICAS={{num_clients}} docker compose up -d --force-recreate

stop_test_infra:
    cd docker/test && docker compose stop

# Setup clients assigning one available GPU to each of them.
# There's no way to do this using the replicas from docker-compose file, so we have to do it manually.
setup_clients num_clients="1": build_docker_test_client
    ./scripts/train-multiple-gpu-localnet.sh {{num_clients}}

# Build the docker psyche client
build_docker_psyche_client:
    ./scripts/coordinator-address-check.sh
    docker build -t psyche-base -f docker/psyche_base.Dockerfile .
    docker build -t psyche-client -f docker/psyche_client.Dockerfile .

clean_stale_images:
    docker rmi $(docker images -f dangling=true -q)
