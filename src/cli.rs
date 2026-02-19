// CLI argument parsing for Hayate

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "hayate")]
#[command(about = "疾風 Hayate - Swift Cardano indexer with UTxORPC", long_about = None)]
pub struct Args {
    /// Configuration file
    #[arg(short, long)]
    pub config: Option<String>,

    /// Database directory (overrides config)
    #[arg(short, long)]
    pub db_path: Option<String>,

    /// Network to index (mainnet, preprod, preview, sanchonet)
    #[arg(short, long)]
    pub network: Option<String>,

    /// UTxORPC API bind address (overrides config)
    #[arg(long)]
    pub api_bind: Option<String>,

    /// Gap limit for address discovery (overrides config)
    #[arg(long)]
    pub gap_limit: Option<u32>,

    /// Generate default config file
    #[arg(long)]
    pub generate_config: Option<String>,

    /// Start from genesis
    #[arg(long)]
    pub from_genesis: bool,

    /// Node socket path (for direct node connection)
    #[arg(short, long)]
    pub socket: Option<String>,
}
