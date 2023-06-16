use antelope_firewall_lib::firewall_builder::{AntelopeFirewall, RoutingModeState, AntelopeFirewallError};

#[tokio::main]
async fn main() -> Result<(), AntelopeFirewallError> {
    AntelopeFirewall::new(RoutingModeState::base_random())
        .build().run().await
}
