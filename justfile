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
	cargo run -p local-testnet -- {{args}}

# run integration tests
integration-test test_name="":
    if [ "{{test_name}}" = "" ]; then \
        cargo test --test integration_tests; \
    else \
        cargo test --test integration_tests -- --nocapture "{{test_name}}"; \
    fi

# build solana coordinator
deploy-local-solana-coordinator:
    cd architectures/decentralized/solana-coordinator && anchor build && anchor deploy
