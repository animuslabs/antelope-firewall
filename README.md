
# antelope-firewall Milestones 1 and 2

This repo contains two crates, antelope-firewall and antelope-firewall-lib. antelope-firewall-lib is a framework that allows a developer to more easily write their own ratelimiter,
and antelope-firewall is a simple cli wrapper for the basic configuration of antelope-firewall-lib.

## Building
### Dependencies
`sudo apt install openssl libssl-dev`
### Build
`cargo build --release --bin antelope-firewall`

## Running

## With Docker

1. Ensure a config file exists at `/etc/antelope-firewall/config.toml` (or whatever your setup is in the docker compose file). An example config file with documentation exists in the repo `test/example.toml`. Please read before you begin, for demonstration purposes at the moment it is not configured with sensible defaults.

The firewall port must be 3000 and the prometheus port must be 3001.

2. Build the docker image. `docker compose build`

3. Run docker. `docker compose up -d`

## Without Docker

We are not providing prebuilt binaries for this project for milestone 1 and 2.
In order to run, please run either `cargo run --bin antelope-firewall -- --config /path/to/config` or `cargo build --release --bin antelope-firewall`, then `./target/release/antelope-firewall --config /path/to/config`.
Please ensure ensure that the config file is in toml format, following the example config file `test/example.toml`

## Prometheus

This firewall runs a Prometheus exporter on a port configurable in the config.
It is recommended that you limit which servers can connect to this port via an nftables rule.


# How to
Antelope Firewall is not supposed to replace Haproxy or nginx solutions but to be setup behind them.
One example of a setup would be to have HAProxy to deal with SSL certificate and then just forward the traffic to the firewall.

## 1. Setup with Docker
First Install docker and clone antelope-firewall repository.  
You will find predifined Dockerfile that will be needed to build the container.  
Next, setup your docker-compose.yml file and make it work with your local environment.  
Description of options in the docker-compose file (comments are done only to describe functionality they should not be included in the yml file):  

version: '3.8'  
services:  
  antelope-firewall:  
    build:   
      context: . /// this points to the local directory. it's important to run docker compose build in the root of the antelope-firewall repository  
      dockerfile: Dockerfile /// This is the Dockerfile that will be used to build the container  
    environment:  
      - CONFIG_PATH=/etc/antelope-firewall/config_main.toml /// configuration file for the firewall  
    volumes:  
      - /home/antelopeio/antelope-firewall/config_main.toml:/etc/antelope-firewall/config_main.toml /// configuration file for the firewall mounted from where you cloned your repository  
    ports:  
      - 3000:3000 /// default port for the firewall if you want to change this you will have to edit the Dockerfile and config_main.toml   
      - 3001:3001 /// default port for the prometheus exporter if you want to change this you will have to edit the Dockerfile and config_main.toml   
  
	  
### config_main.toml file description (comments are done only to describe functionality they should not be included in the toml file) - this is a 3 node setup

routing_mode = "round_robin" // setup how you want to distribute trxes | requests  
address = "0.0.0.0:3000" // this defines the port and that traffic can be accepted from outside  
prometheus_address = "0.0.0.0:3001" // allow stats to be collected by your prometheus instance  


// Healthcheck will run a get_info request every `interval` seconds on all nodes to determine  
// which nodes are in sync. If a node has a head_block_time more than `grace_period` seconds  
// older than the current system time, that node will not be considered as a valid node  
// to route requests to. Healthcheck will then keep making requests at the same interval  
// to determine if the node re-syncs.  
[healthcheck]  
interval = 15  
grace_period = 5  

[filter]  
block_contracts = ["eosio"] // in this example it will block any transactions sent to this account, you can enter more than one   
block_ips = []  // block requests that originate from the below IP list.  
allow_only_contracts = ["token.boid"] // only allow requests that contain an action sent to an account in the below array, you can enter more than one   

// you can have more than one ratelimiter setup  
  
// Example of a ratelimiter that limits any IP to only be able to send  
// 10 requests every 60 seconds.  
[[ratelimit]]   
name = "base" // Name of ratelimiter, used for logging and prometheus metrics.  
limit_on = "attempt" // "attempt" increments the ratelimit count on incoming request, failure increments the count when a request is forwarded to and end node and comes back with an error http status  
bucket_type = "ip" /// "ip" sets the bucket to be the request IP.  
limit = 50  
window_duration = 60  

// Example of a ratelimiter that limits any given transaction authorizer to only  
// be able to send a request if they have not had more than 2 failed transactions  
// every 60 seconds.  
[[ratelimit]]  
name = "failure" // Name of ratelimiter, used for logging and prometheus metrics.  
limit_on = "failure"  
bucket_type = "authorizer" // "authorizer" is for all accounts in an authorization list  
limit = 2  
window_duration = 60  
select_accounts = []  

// Example of a ratelimiter that limits table requests of a particular table to only 30 ever minute.  
[[ratelimit]]  
name = "table" // Name of ratelimiter, used for logging and prometheus metrics.  
limit_on = "attempt"  
bucket_type = "table"  
tables = [["eosio", "voters"]]  
limit = 30  
window_duration = 60  
// If select_accounts is specified, the ratelimiter will only apply for tables  
// on contracts in the list. If not specified, the ratelimiter will apply to all tables.  
// This applies to /get_table_rows and /get_tables_by_scope  
select_accounts = []  

#### this is where you setup your nodes  
[[push_nodes]]  
name = "Node-1"  
url = "http://192.168.0.110:8888"  
routing_weight = 1  
  
[[push_nodes]]  
name = "Node-2"  
url = "http://192.168.0.113:8888"  
routing_weight = 1  
  
[[push_nodes]]  
name = "Node-3"  
url = "http://192.168.0.114:8888"  
routing_weight = 1  
  
[[get_nodes]]  
name = "Node-1"  
url = "http://192.168.0.110:8888"  
  
[[get_nodes]]  
name = "Node-2"  
url = "http://192.168.0.113:8888"  
  
[[get_nodes]]  
name = "Node-3"  
url = "http://192.168.0.114:8888"  
  
#### build the image and run  
Build the docker image    
```
docker compose build
```
  
run docker  
```
docker compose up
```

## 2. Setup without Docker
firewall was testnet on an Ubuntu 22 instance
make sure that you have installed
```
sudo apt install openssl libssl-dev
```

clone the repository and cd into it  

### run build
```
cargo build --release --bin antelope-firewall
```

### make sure that config_main.toml file ready just like in the docker example
In order to run, please run either (remember to change your config path)
```
cargo run --bin antelope-firewall -- --config /path/to/config/config_main.toml
```
or
```
cargo build --release --bin antelope-firewall
```


**additional detailed options are described in config_main.toml example file.**
