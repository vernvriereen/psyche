FROM debian:bookworm-slim AS base
WORKDIR /usr/src

RUN apt-get update && apt-get install -y unzip zip libssl-dev libgomp1 curl wget build-essential && rm -rf /var/lib/apt/lists/*
RUN wget https://download.pytorch.org/libtorch/cu124/libtorch-cxx11-abi-shared-with-deps-2.4.1%2Bcu124.zip \
    && unzip libtorch-cxx11-abi-shared-with-deps-2.4.1+cu124.zip \
    && rm -rf libtorch-cxx11-abi-shared-with-deps-2.4.1+cu124.zip

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

## Chef Planner
FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef

FROM chef AS planner
WORKDIR /usr/src/psyche
COPY . .
RUN cargo chef prepare --recipe-path client-recipe.json

## Chef builder
FROM chef AS chef_builder

WORKDIR /usr/src/psyche
COPY --from=base /usr/src/libtorch /usr/home/libtorch
ENV LIBTORCH=/usr/home/libtorch
ENV LIBTORCH_INCLUDE=/usr/home/libtorch
ENV LIBTORCH_LIB=/usr/home/libtorch
ENV LD_LIBRARY_PATH=/usr/home/libtorch/lib:$LD_LIBRARY_PATH

COPY --from=planner /usr/src/psyche/client-recipe.json /usr/src/psyche/client-recipe.json
RUN cargo chef cook --release --recipe-path client-recipe.json

FROM base AS builder
WORKDIR /usr/src/psyche

COPY . .
COPY --from=chef_builder /usr/src/psyche/target ./target

ENV LIBTORCH=/usr/src/libtorch
ENV LIBTORCH_INCLUDE=/usr/src/libtorch
ENV LIBTORCH_LIB=/usr/src/libtorch
ENV LD_LIBRARY_PATH=/usr/src/libtorch/lib:$LD_LIBRARY_PATH

RUN cargo build --release
