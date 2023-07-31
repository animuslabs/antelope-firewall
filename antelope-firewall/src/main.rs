use antelope_firewall_lib::{firewall_builder::{AntelopeFirewall, RoutingModeState, AntelopeFirewallError}, config::{from_config, Config}};
use clap::Parser;
use command_line::Args;

mod command_line;

#[tokio::main]
async fn main() {
    let args = Args::parse();

    match &std::fs::read_to_string(args.config.clone()) {
        Ok(contents) => {
            match toml::from_str::<Config>(contents) {
                Ok(parsed) => {
                    match from_config(parsed).await {
                        Ok(firewall) => {
                            firewall.build().run().await;
                        },
                        Err(e) => {
                            println!("Error occurred while configuring firewall.");
                            println!("Received error: {}", e);
                        },
                    }
                },
                Err(e) => {
                    println!("Unable to parse config {}. Please run antelope-firewall --help for information on how to set the config path.", args.config);
                    println!("Parsing error: {}", e);
                }
            }
        },
        Err(e) => {
            println!("Unable to read config {}. Please run antelope-firewall --help for information on how to set the config path.", args.config);
            println!("Received error: {}", e);
        }
    }
    
}
