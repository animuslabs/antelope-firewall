use clap::Parser;

/// Firewall for the Antelope Blockchain ecosystem.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// Path to config file.
    #[arg(short, long, default_value_t = String::from("/etc/antelope-firewall/antelope.toml"))]
    pub config: String,
}
