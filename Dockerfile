# Dockerfile
FROM rust

WORKDIR /usr/src/app

COPY . .

RUN cargo build --release

CMD ./target/release/antelope-firewall -c "${CONFIG_PATH}"

EXPOSE 3000
EXPOSE 3001

