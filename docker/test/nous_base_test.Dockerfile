FROM nous-base as base
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y unzip zip libssl-dev libgomp1 curl wget build-essential && rm -rf /var/lib/apt/lists/*

RUN sh -c "$(curl -sSfL https://release.anza.xyz/stable/install)"

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

RUN cargo install --git https://github.com/coral-xyz/anchor --tag v0.30.1 anchor-cli

COPY --from=base /usr/src/libtorch /usr/home/libtorch

ENV LIBTORCH=/usr/home/libtorch
ENV LIBTORCH_INCLUDE=/usr/home/libtorch
ENV LIBTORCH_LIB=/usr/home/libtorch
ENV LD_LIBRARY_PATH=/usr/home/libtorch/lib:$LD_LIBRARY_PATH
ENV PATH="/root/.local/share/solana/install/active_release/bin:$PATH"

COPY --from=base /usr/src/psyche/target/release/psyche-solana-client /usr/local/bin

WORKDIR /usr/src/psyche
COPY . .

RUN cd architectures/decentralized/solana-coordinator && anchor keys sync && anchor build --no-idl
