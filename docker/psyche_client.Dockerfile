FROM psyche-base AS base
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y libssl-dev libgomp1 curl wget build-essential && rm -rf /var/lib/apt/lists/*

# Copy and set libtorch from base
COPY --from=base /usr/home/libtorch /usr/home/libtorch
ENV LIBTORCH=/usr/home/libtorch
ENV LIBTORCH_INCLUDE=/usr/home/libtorch
ENV LIBTORCH_LIB=/usr/home/libtorch
ENV LD_LIBRARY_PATH=/usr/home/libtorch/lib

# Copy the psyche client binary from base
COPY --from=base /usr/src/psyche/target/release/psyche-solana-client /usr/local/bin/psyche-solana-client

# Copy the entrypoint script from host machine
# COPY --chmod=755 --from=base /usr/src/psyche/docker/client_entrypoint.sh /usr/local

ENV RUST_BACKTRACE=1
ENV RUST_LOG=info,psyche=debug

ENTRYPOINT ["/usr/local/bin/psyche-solana-client"]
CMD ["--help"]
