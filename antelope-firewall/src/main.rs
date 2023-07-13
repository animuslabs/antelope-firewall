use antelope_firewall_lib::{firewall_builder::{AntelopeFirewall, RoutingModeState, AntelopeFirewallError}, config::{from_config, Config}};

#[tokio::main]
async fn main() -> Result<(), AntelopeFirewallError> {
    let firewall = from_config(toml::from_str::<Config>(&std::fs::read_to_string("./test/example.toml").unwrap()).unwrap()).await.unwrap();
    firewall.build().run().await
}
