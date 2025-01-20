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
	cargo run -p psyche-local-testnet -- start {{args}}

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

# install deps for building mdbook
book_deps:
	cargo install mdbook mdbook-mermaid

build_book output-dir="../book":
	mdbook build psyche-book -d {{output-dir}}

serve_book:
	mdbook serve psyche-book --open

generate_cli_docs:
    cargo run -p psyche-centralized-client print-all-help --markdown > docs/CommandLineHelp-client.md
    cargo run -p psyche-centralized-server print-all-help --markdown > docs/CommandLineHelp-server.md
    cargo run -p psyche-local-testnet print-all-help --markdown > docs/CommandLineHelp-local-testnet.md
