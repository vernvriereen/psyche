# BUILDER
FROM rust:1.82 AS builder

# install Solana CLI
RUN apt update
RUN sh -c "$(curl -sSfL https://release.anza.xyz/stable/install)"
RUN . /root/.profile

# install Anchor
RUN cargo install --git https://github.com/coral-xyz/anchor --tag v0.30.1 anchor-cli

# build Solana psyche coordinator
WORKDIR /usr/src/psyche
COPY . .
ENV PATH="/root/.local/share/solana/install/active_release/bin:$PATH"
RUN cd ./architectures/decentralized/solana-coordinator && anchor build --no-idl
RUN cd -

# RUNTIME
FROM debian:bookworm-slim AS runtime

COPY --from=builder /usr/src/psyche/architectures/decentralized/solana-coordinator /usr/local/solana-coordinator
COPY --from=builder /root/.local/share/solana/install/active_release/bin/* /usr/local/bin
COPY --from=builder /usr/local/cargo/bin/anchor /usr/local/bin
COPY --from=builder /usr/src/psyche/psyche_solana_entrypoint.sh /usr/local

RUN apt update

RUN chmod a+x /usr/local/psyche_solana_entrypoint.sh
ENTRYPOINT ["/usr/local/psyche_solana_entrypoint.sh"]
