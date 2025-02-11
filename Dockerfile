## BUILDER
FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef
WORKDIR /usr/src/psyche

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path client-recipe.json

FROM chef AS builder
WORKDIR /usr/src/

RUN apt-get update && apt-get install -y unzip zip
RUN wget https://download.pytorch.org/libtorch/cu124/libtorch-cxx11-abi-shared-with-deps-2.4.1%2Bcu124.zip \
    && unzip libtorch-cxx11-abi-shared-with-deps-2.4.1+cu124.zip \
    && rm -rf libtorch-cxx11-abi-shared-with-deps-2.4.1+cu124.zip

ENV LIBTORCH=/usr/src/libtorch
ENV LIBTORCH_INCLUDE=/usr/src/libtorch
ENV LIBTORCH_LIB=/usr/src/libtorch
ENV LD_LIBRARY_PATH=/usr/src/libtorch/lib:$LD_LIBRARY_PATH

WORKDIR /usr/src/psyche
COPY --from=planner /usr/src/psyche/client-recipe.json client-recipe.json
RUN cargo chef cook --release --recipe-path client-recipe.json
COPY . .
RUN cargo build --release

## RUNTIME
FROM debian:bookworm-slim AS runtime
COPY --from=builder /usr/src/psyche/target/release/psyche-solana-client /usr/local/bin/psyche-solana-client
COPY --from=builder /usr/src/libtorch /usr/home/libtorch
COPY --chmod=755 --from=builder /usr/src/psyche/docker-entrypoint.sh /usr/local
RUN apt-get update && apt-get install -y libssl-dev libgomp1 curl && rm -rf /var/lib/apt/lists/*

ENV LIBTORCH=/usr/home/libtorch
ENV LIBTORCH_INCLUDE=/usr/home/libtorch
ENV LIBTORCH_LIB=/usr/home/libtorch
ENV LD_LIBRARY_PATH=/usr/home/libtorch/lib:$LD_LIBRARY_PATH

ENTRYPOINT ["./usr/local/docker-entrypoint.sh"]

## TEST CLIENT
FROM debian:bookworm-slim AS test
COPY --from=builder /usr/src/psyche/target/release/psyche-solana-client /usr/local/bin/psyche-solana-client
COPY --from=builder /usr/src/libtorch /usr/home/libtorch
COPY --chmod=755 --from=builder /usr/src/psyche/client-test-entrypoint.sh /usr/local
COPY --chmod=755 --from=builder /usr/src/psyche/run-owner-entrypoint.sh /usr/local
COPY --from=builder /usr/src/psyche/config/solana-test/light-config.toml /usr/local/config.toml
RUN apt-get update && apt-get install -y libssl-dev libgomp1 curl && rm -rf /var/lib/apt/lists/*

# Install solana cli
RUN sh -c "$(curl -sSfL https://release.anza.xyz/stable/install)"

ENV LIBTORCH=/usr/home/libtorch
ENV LIBTORCH_INCLUDE=/usr/home/libtorch
ENV LIBTORCH_LIB=/usr/home/libtorch
ENV LD_LIBRARY_PATH=/usr/home/libtorch/lib:$LD_LIBRARY_PATH
ENV PATH="/root/.local/share/solana/install/active_release/bin:$PATH"

ENTRYPOINT ["./usr/local/client-test-entrypoint.sh"]
