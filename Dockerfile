## BUILDER
FROM rust:1.82 AS builder
WORKDIR /usr/src

RUN apt-get update && apt-get install -y unzip zip
RUN wget https://download.pytorch.org/libtorch/cu124/libtorch-cxx11-abi-shared-with-deps-2.4.1%2Bcu124.zip
RUN unzip libtorch-cxx11-abi-shared-with-deps-2.4.1+cu124.zip
RUN rm -rf libtorch-cxx11-abi-shared-with-deps-2.4.1+cu124.zip

ENV LIBTORCH=/usr/src/libtorch
ENV LIBTORCH_INCLUDE=/usr/src/libtorch
ENV LIBTORCH_LIB=/usr/src/libtorch
ENV LD_LIBRARY_PATH=/usr/src/libtorch/lib:$LD_LIBRARY_PATH

WORKDIR /usr/src/psyche

COPY . .
RUN cargo build --release

## RUNTIME
FROM debian:bookworm-slim AS runtime
COPY --from=builder /usr/src/psyche/target/release/psyche-solana-client /usr/local/bin/psyche-solana-client
COPY --from=builder /usr/src/libtorch /usr/home/libtorch
COPY --from=builder /usr/src/psyche/docker-entrypoint.sh /usr/local
RUN apt-get update && apt-get install -y libssl-dev libgomp1 curl && rm -rf /var/lib/apt/lists/*

ENV LIBTORCH=/usr/home/libtorch
ENV LIBTORCH_INCLUDE=/usr/home/libtorch
ENV LIBTORCH_LIB=/usr/home/libtorch
ENV LD_LIBRARY_PATH=/usr/home/libtorch/lib:$LD_LIBRARY_PATH

RUN chmod a+x /usr/local/docker-entrypoint.sh

ENTRYPOINT ["./usr/local/docker-entrypoint.sh"]

## TEST CLIENT
FROM debian:bookworm-slim AS test
COPY --from=builder /usr/src/psyche/target/release/psyche-solana-client /usr/local/bin/psyche-solana-client
COPY --from=builder /usr/src/libtorch /usr/home/libtorch
COPY --from=builder /usr/src/psyche/client-test-entrypoint.sh /usr/local
RUN apt-get update && apt-get install -y libssl-dev libgomp1 curl && rm -rf /var/lib/apt/lists/*

# Install solana cli
RUN sh -c "$(curl -sSfL https://release.anza.xyz/stable/install)"

ENV LIBTORCH=/usr/home/libtorch
ENV LIBTORCH_INCLUDE=/usr/home/libtorch
ENV LIBTORCH_LIB=/usr/home/libtorch
ENV LD_LIBRARY_PATH=/usr/home/libtorch/lib:$LD_LIBRARY_PATH
ENV PATH="/root/.local/share/solana/install/active_release/bin:$PATH"

RUN chmod a+x /usr/local/client-test-entrypoint.sh

ENTRYPOINT ["./usr/local/client-test-entrypoint.sh"]

## VALIDATOR
FROM rust:1.82 AS validator
COPY --from=builder /usr/src/psyche/ /usr/local/psyche
COPY --from=builder /usr/src/psyche/validator-entrypoint.sh /usr/local

# Install solana cli
RUN apt-get update && apt install -y curl
RUN sh -c "$(curl -sSfL https://release.anza.xyz/stable/install)"
RUN cargo install --git https://github.com/coral-xyz/anchor --tag v0.30.1 anchor-cli
ENV PATH="/root/.local/share/solana/install/active_release/bin:$PATH"

RUN chmod a+x /usr/local/validator-entrypoint.sh

ENTRYPOINT ["./usr/local/validator-entrypoint.sh"]
