# antelope-firewall Milestone 1

## Building
### Dependencies
`sudo apt install openssl libssl-dev`
### Build
`cargo build`

## Running

We are not providing prebuilt binaries for this project for milestone 1.
In order to run, please run either `cargo run` or `cargo build --release`, then `./target/release/antelope-firewall --config /path/to/config`.
Please ensure ensure that the config file is in toml format, following the example config file `test/example.toml`

## Prometheus

This firewall runs a Prometheus exporter on a port configurable in the config.
It is recommended that you limit which servers can connect to this port via an nftables rule.
