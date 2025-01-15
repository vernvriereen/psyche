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
	cargo run -p psyche-local-testnet -- {{args}}

# run integration tests
integration-test test_name="":
    if [ "{{test_name}}" = "" ]; then \
        cargo test --release --test integration_tests; \
    else \
        cargo test --release --test integration_tests -- --nocapture "{{test_name}}"; \
    fi

# build solana coordinator. Some errors are happening trying to build the `idl` since we are not using it, we disabled it for now.
deploy-local-solana-coordinator:
    cd architectures/decentralized/solana-coordinator && anchor build --no-idl && anchor deploy

solana-client-tests:
	cargo test --package psyche-solana-client

install:
	cargo install --path architectures/centralized/client
	cargo install --path architectures/centralized/server
	cargo install --path architectures/centralized/local-testnet

uninstall:
	cargo uninstall psyche-centralized-client
	cargo uninstall psyche-centralized-server
	cargo uninstall psyche-local-testnet
