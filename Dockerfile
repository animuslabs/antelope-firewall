FROM rust

COPY . .

RUN cargo build --release

CMD ["./target/release/antelope-firewall", "-c", "test/example.toml"]
EXPOSE 3000
EXPOSE 3001
