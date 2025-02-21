FROM nous-base AS base
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y unzip zip libssl-dev libgomp1 curl wget build-essential && rm -rf /var/lib/apt/lists/*

COPY --from=base /usr/home/libtorch /usr/home/libtorch
COPY --chmod=755 --from=base /usr/src/psyche/docker/client_entrypoint.sh /usr/local
COPY --from=base /usr/src/psyche/target/release/psyche-solana-client /usr/local/bin/psyche-solana-client

ENV LIBTORCH=/usr/home/libtorch
ENV LIBTORCH_INCLUDE=/usr/home/libtorch
ENV LIBTORCH_LIB=/usr/home/libtorch
ENV LD_LIBRARY_PATH=/usr/home/libtorch/lib:$LD_LIBRARY_PATH

ENTRYPOINT ["/usr/local/client_entrypoint.sh"]
