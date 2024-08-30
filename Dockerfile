FROM rustlang/rust:nightly as builder
WORKDIR /usr/src/psyche
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
COPY --from=builder /usr/src/psyche/target/release/psyche /usr/local/bin/psyche
RUN apt-get update && apt-get install -y libssl-dev && rm -rf /var/lib/apt/lists/*

CMD ["psyche", "--tui=false", "ae367srna6hxizjrclg43zachecxtn7vta5wuxza3ndet7ucejuz6ajdnb2hi4dthixs65ltmuys2mjoojswyylzfzuxe33ifzxgk5dxn5zgwlrpamaaufaaa3tioaqarlczwthgq4bablaraaa6nbyc"]