FROM rustlang/rust:nightly as builder
WORKDIR /usr/src/psyche
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
COPY --from=builder /usr/src/psyche/target/release/psyche /usr/local/bin/psyche
RUN apt-get update && apt-get install -y libssl-dev && rm -rf /var/lib/apt/lists/*

CMD ["psyche", "aetygvefqftnweig6sygd7ib6u7u3rkck6l3sfouknhiztrxoadkgajdnb2hi4dthixs65ltmuys2mjoojswyylzfzuxe33ifzxgk5dxn5zgwlrpamaaufaaa3fzkaqarlczwtglsubablaraaa4xfic"]