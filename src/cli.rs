// CLI argument parsing for Hayate

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "hayate")]
#[command(about = "疾風 Hayate - Swift Cardano indexer with UTxORPC", long_about = None)]
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Configuration file
    #[arg(short, long, global = true)]
    pub config: Option<String>,

    /// Database directory (overrides config)
    #[arg(short, long, global = true)]
    pub db_path: Option<String>,

    /// Network to use (mainnet, preprod, preview, sanchonet)
    #[arg(short, long, global = true)]
    pub network: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Run the indexer and sync from the network
    Sync {
        /// UTxORPC API bind address (overrides config)
        #[arg(long)]
        api_bind: Option<String>,

        /// Gap limit for address discovery (overrides config)
        #[arg(long)]
        gap_limit: Option<u32>,

        /// Start from genesis
        #[arg(long)]
        from_genesis: bool,

        /// Node socket path (for direct node connection)
        #[arg(short, long)]
        socket: Option<String>,
    },

    /// Wallet query commands
    Wallet {
        #[command(subcommand)]
        wallet_cmd: WalletCommand,
    },

    /// Configuration commands
    Config {
        #[command(subcommand)]
        config_cmd: ConfigCommand,
    },
}

#[derive(Subcommand, Debug)]
pub enum WalletCommand {
    /// Show wallet statistics (UTxOs, balance, transactions)
    Stats {
        /// Wallet xpub or identifier (if not specified, shows all wallets)
        wallet: Option<String>,
    },

    /// List wallet UTxOs
    Utxos {
        /// Wallet xpub or identifier
        wallet: String,
    },

    /// List wallet transaction history
    Txs {
        /// Wallet xpub or identifier
        wallet: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigCommand {
    /// Generate default configuration file
    Generate {
        /// Output path for config file
        #[arg(default_value = "hayate-config.toml")]
        output: String,
    },
}
