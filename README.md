
# antelope-firewall: A combination Ratelimiter/Firewall/Load Balancer for Antelope RPC nodes

This repo contains two crates, antelope-firewall and antelope-firewall-lib. antelope-firewall-lib is a framework that allows a developer to more easily write their own ratelimiter, and antelope-firewall is a simple cli wrapper for the basic configuration of antelope-firewall-lib.

Features:
  - Load balance to multiple get and push RPC nodes through either weighted round robin, weighted random, or weighted least connected.
  - Filter out requests by IP, or target account (allow or denylist) for transactions.
  - Ratelimit requests using [sliding window algorithm](https://medium.com/@m-elbably/rate-limiting-the-sliding-window-algorithm-daa1d91e6196#:~:text=The%20Sliding%20Window%20Algorithm%20is,rate%20limiting%20in%20various%20applications.) by request IP, or target account or authorizer for transactions.
  - Prometheus exporter for remote monitoring

Non-features:
  - Does not unwrap SSL requests. We do not replace Nginx and HAProxy solutions, we recommend you place this behind HAProxy to deal with SSL certificate, then forward requests to antelope-firewall.

# Running

## With Docker

1. Clone the repo and edit the `docker-compose.yml` file to suit your needs. If you decide to change the firewall or prometheus ports in the config you must also change which ports are exposed in the `config.toml`

2. Ensure a config file exists at `/etc/antelope-firewall/config.toml` (or whatever your setup is in the docker compose file). An example config file with documentation exists as `default_config.toml`. You cand find more info about how to edit the config in the ["Configure" section of this document.](https://github.com/animuslabs/antelope-firewall?tab=readme-ov-file#configure)

3. Build the docker image. `docker compose build`

4. Run docker. `docker compose up -d`

## Without Docker

1. Ensure you have the following dependencies installed
```
sudo apt install openssl
```

2. Go to the [Github releases page](adb) and download the most recent *.deb file. Install with `sudo dpkg -i antelope-firewall_*.deb` This will install antelope-firewall as a binary and create the systemd service `antelope-firewall`.

3. You will then need to edit the config file at `/etc/antelope-firewall/config.toml` as described in the ["Configure" section of this document.](https://github.com/animuslabs/antelope-firewall?tab=readme-ov-file#configure)

4. Once you have a config file, enable and start the service using `systemctl enable antelope-firewall` and `systemctl start antelope-firewall`

### Prometheus

This firewall runs a Prometheus exporter on a port configurable in the config. It is recommended that you limit which servers can connect to this port via an nftables rule.

# Configuring
The file `default_config.toml` contains default settings which will work for most users. It does not filter out anything, and sets a ratelimiter that will only allow a given IP to submit transactions until it sends 5 failing requests in a minute.
The most important thing to change is the list of nodes that the firewall will delegate requests to. For example purposes the following is used:

```
[[push_nodes]]
name = "push_one"
url = "http://127.0.0.1:5000"
weight = 1

[[get_nodes]]
name = "get_one"
url = "http://127.0.0.1:5001"
weight = 1

[[get_nodes]]
name = "get_two"
url = "http://127.0.0.1:5002"
weight = 1

[[get_nodes]]
name = "get_three"
url = "http://127.0.0.1:5003"
weight = 1
```

This will result in having the firewall proxy "read" requests to three urls, and "write" requests to one url. A full list of which requests are "read" or "write" is included in the comments of `default_config.toml`. You will very likely need to edit this based on your setup. For example, if you wanted to add another url that can be used as a proxy, simply duplicate the first `[[push_nodes]]` section and edit the respective entries. Note that name must be unique, and weight corresponds to how much a node will be favored when it comes to selecting a destination for a request.

# Testing
All tests can be run with `cargo test` in the root of the repository

## Building
### Dependencies
`sudo apt install openssl libssl-dev`
### Build
`cargo build --release --bin antelope-firewall`
