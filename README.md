# antelope-firewall Milestone 1

This repo contains two crates, antelope-firewall and antelope-firewall-lib. antelope-firewall-lib is a framework that allows a developer to more easily write their own ratelimiter,
and antelope-firewall is a simple cli wrapper for the basic configuration of antelope-firewall-lib.

## Building
### Dependencies
`sudo apt install openssl libssl-dev`
### Build
`cargo build --release --bin antelope-firewall`

## Running

We are not providing prebuilt binaries for this project for milestone 1 and 2.
In order to run, please run either `cargo run --bin antelope-firewall -- --config /path/to/config` or `cargo build --release --bin antelope-firewall`, then `./target/release/antelope-firewall --config /path/to/config`.
Please ensure ensure that the config file is in toml format, following the example config file `test/example.toml`

## Prometheus

This firewall runs a Prometheus exporter on a port configurable in the config.
It is recommended that you limit which servers can connect to this port via an nftables rule.
