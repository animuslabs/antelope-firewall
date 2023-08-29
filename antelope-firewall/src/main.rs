use antelope_firewall_lib::{firewall_builder::{AntelopeFirewall, RoutingModeState, AntelopeFirewallError}, config::{from_config, Config}};
use clap::Parser;
use command_line::Args;

use log::error;

mod command_line;

#[tokio::main]
async fn main() {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info")
    }
    env_logger::init();
    let args = Args::parse();

    match &std::fs::read_to_string(args.config.clone()) {
        Ok(contents) => {
            match toml::from_str::<Config>(contents) {
                Ok(parsed) => {
                    match from_config(parsed).await {
                        Ok(firewall) => {
                            let res = firewall.build().run().await;
                            if let Err(e) = res {
                                error!("Error occurred while running firewall. Received error: {}", e);
                            }
                        },
                        Err(e) => {
                            error!("Error occurred while configuring firewall. Received error: {}", e);
                        },
                    }
                },
                Err(e) => {
                    error!("Unable to parse config {}. Please run antelope-firewall --help for information on how to set the config path. Parsing error: {}", args.config, e);
                }
            }
        },
        Err(e) => {
            error!("Unable to read config {}. Please run antelope-firewall --help for information on how to set the config path. Received error: {}", args.config, e);
        }
    }
    
}
