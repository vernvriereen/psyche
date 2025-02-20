FROM nous-base-test as base
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y unzip zip libssl-dev libgomp1 curl wget build-essential && rm -rf /var/lib/apt/lists/*

COPY --from=base /usr/home/libtorch /usr/home/libtorch
COPY --from=base /usr/local/bin /usr/local/bin

ENV LIBTORCH=/usr/home/libtorch
ENV LIBTORCH_INCLUDE=/usr/home/libtorch
ENV LIBTORCH_LIB=/usr/home/libtorch
ENV LD_LIBRARY_PATH=/usr/home/libtorch/lib:$LD_LIBRARY_PATH
ENV PATH="/root/.local/share/solana/install/active_release/bin:$PATH"

COPY --from=base /root/.local/share/solana/install/active_release/bin/ /usr/local/bin
COPY --from=base /root/.cargo/bin/anchor /usr/local/bin
COPY --from=base /usr/src/psyche/config/solana-test/light-config.toml /usr/local/config.toml
COPY --from=base /usr/local/bin/psyche-solana-client /usr/local/bin
COPY --chmod=755 ./docker/test/client_test_entrypoint.sh /usr/local
COPY --chmod=755 ./docker/test/run_owner_entrypoint.sh /usr/local

ENTRYPOINT ["/usr/local/client_test_entrypoint.sh"]
