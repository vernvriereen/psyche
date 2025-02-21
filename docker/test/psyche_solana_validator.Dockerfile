FROM nous-base AS base

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y libssl-dev libgomp1 curl wget build-essential && rm -rf /var/lib/apt/lists/*

# Install Rust to install Anchor
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# Install Solana
RUN sh -c "$(curl -sSfL https://release.anza.xyz/stable/install)"
ENV PATH="/root/.local/share/solana/install/active_release/bin:$PATH"

ENV RUST_BACKTRACE=1

# Install Anchor
RUN cargo install --git https://github.com/coral-xyz/anchor --tag v0.30.1 anchor-cli

# Copy the compiled binaries from the base image
COPY --from=base /usr/src/psyche/architectures/decentralized/solana-coordinator /usr/local/solana-coordinator

# Makes sure that the coordinator was built on the host machine before running the container
RUN [ -d "/usr/local/solana-coordinator/target" ] || (echo "The coordinator must be built on the host machine" 1>&2 && exit 1)

# Copy the entrypoint script from host machine
COPY --chmod=755 ./docker/test/psyche_solana_validator_entrypoint.sh /usr/local

ENTRYPOINT ["/usr/local/psyche_solana_validator_entrypoint.sh"]
