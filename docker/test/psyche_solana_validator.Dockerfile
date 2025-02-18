FROM nous-base as base
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y unzip zip libssl-dev libgomp1 curl wget build-essential && rm -rf /var/lib/apt/lists/*

COPY --from=base /usr/local/bin/* /usr/local/bin
COPY --from=base /usr/src/psyche/architectures/decentralized/solana-coordinator /usr/local/solana-coordinator
COPY --chmod=755 --from=base /usr/src/psyche/docker/test/psyche_solana_validator_entrypoint.sh /usr/local

RUN . /root/.profile
ENTRYPOINT ["/usr/local/psyche_solana_validator_entrypoint.sh"]
