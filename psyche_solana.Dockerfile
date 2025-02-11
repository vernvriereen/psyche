FROM rust:1.82

# install Solana CLI
RUN apt update
RUN sh -c "$(curl -sSfL https://release.anza.xyz/stable/install)"
RUN . /root/.profile
RUN cargo install --git https://github.com/coral-xyz/anchor --tag v0.30.1 anchor-cli

# build Solana psyche coordinator
WORKDIR /usr/src/psyche
COPY . .
ENV PATH="/root/.local/share/solana/install/active_release/bin:$PATH"
RUN cd ./architectures/decentralized/solana-coordinator && anchor build --no-idl
RUN cd -

RUN chmod a+x /usr/src/psyche/psyche_solana_entrypoint.sh
ENTRYPOINT ["/usr/src/psyche/psyche_solana_entrypoint.sh"]
