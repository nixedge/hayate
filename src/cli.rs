// CLI argument parsing for Hayate

use clap::{Parser, Subcommand, ValueEnum};

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

    /// Rollback to a specific epoch
    Rollback {
        /// Target epoch to rollback to
        #[arg(short, long)]
        epoch: u64,

        /// Network to rollback (preview, preprod, mainnet, sanchonet)
        #[arg(short, long)]
        network: Option<String>,

        /// Database path
        #[arg(short = 'd', long)]
        db_path: Option<String>,
    },

    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
}

#[derive(ValueEnum, Clone, Debug)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
    PowerShell,
    Elvish,
}

#[derive(Subcommand, Debug)]
pub enum WalletCommand {
    /// Initialize a new wallet with mnemonic
    Init {
        /// Wallet name
        name: String,

        /// GPG recipient for encryption (email or key ID)
        #[arg(long)]
        gpg_recipient: Option<String>,

        /// Number of mnemonic words (12, 15, 18, 21, or 24)
        #[arg(long, default_value = "24")]
        words: usize,

        /// Network (mainnet or testnet)
        #[arg(long, default_value = "testnet")]
        network: String,
    },

    /// Add existing wallet from mnemonic
    Add {
        /// Wallet name
        name: String,

        /// Mnemonic phrase (will prompt if not provided)
        #[arg(long)]
        mnemonic: Option<String>,

        /// GPG recipient for encryption (email or key ID)
        #[arg(long)]
        gpg_recipient: Option<String>,

        /// Network (mainnet or testnet)
        #[arg(long, default_value = "testnet")]
        network: String,
    },

    /// List all wallets
    List,

    /// Show wallet details and addresses
    Show {
        /// Wallet name
        name: String,

        /// Number of addresses to show
        #[arg(long, default_value = "5")]
        count: u32,
    },

    /// Export wallet mnemonic (WARNING: sensitive operation!)
    Export {
        /// Wallet name
        name: String,
    },

    /// Delete a wallet
    Delete {
        /// Wallet name
        name: String,

        /// Skip confirmation prompt
        #[arg(long)]
        yes: bool,
    },

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

    // Transaction commands
    /// Send ADA to an address
    SendTx {
        /// Wallet name
        #[arg(long)]
        wallet: String,

        /// Account index
        #[arg(long, default_value = "0")]
        account: u32,

        /// Recipient address
        #[arg(long)]
        address: String,

        /// Amount in lovelace
        #[arg(long)]
        amount: u64,

        /// Transaction fee in lovelace
        #[arg(long)]
        fee: u64,

        /// Output file for transaction
        #[arg(long)]
        out_file: String,

        /// Include native assets
        #[arg(long)]
        multiasset: bool,

        /// TTL (time to live) slot
        #[arg(long)]
        ttl: Option<u64>,

        /// Sign the transaction
        #[arg(long)]
        sign: bool,
    },

    /// Drain all funds from an account
    DrainTx {
        /// Wallet name
        #[arg(long)]
        wallet: String,

        /// Account index
        #[arg(long, default_value = "0")]
        account: u32,

        /// Destination address
        #[arg(long)]
        address: String,

        /// Transaction fee in lovelace
        #[arg(long)]
        fee: u64,

        /// Output file for transaction
        #[arg(long)]
        out_file: String,

        /// Include native assets
        #[arg(long)]
        multiasset: bool,

        /// Include staking rewards
        #[arg(long)]
        rewards: bool,

        /// TTL (time to live) slot
        #[arg(long)]
        ttl: Option<u64>,

        /// Sign the transaction
        #[arg(long)]
        sign: bool,
    },

    /// Create stake key registration transaction
    StakeRegistrationTx {
        /// Wallet name
        #[arg(long)]
        wallet: String,

        /// Account index
        #[arg(long, default_value = "0")]
        account: u32,

        /// Transaction fee in lovelace
        #[arg(long)]
        fee: u64,

        /// Output file for transaction
        #[arg(long)]
        out_file: String,

        /// Registration deposit (default: 2000000 lovelace)
        #[arg(long, default_value = "2000000")]
        deposit: u64,

        /// TTL (time to live) slot
        #[arg(long)]
        ttl: Option<u64>,

        /// Sign the transaction
        #[arg(long)]
        sign: bool,
    },

    /// Create stake pool delegation transaction
    DelegatePoolTx {
        /// Wallet name
        #[arg(long)]
        wallet: String,

        /// Account index
        #[arg(long, default_value = "0")]
        account: u32,

        /// Pool ID (bech32)
        #[arg(long)]
        pool_id: String,

        /// Transaction fee in lovelace
        #[arg(long)]
        fee: u64,

        /// Output file for transaction
        #[arg(long)]
        out_file: String,

        /// TTL (time to live) slot
        #[arg(long)]
        ttl: Option<u64>,

        /// Sign the transaction
        #[arg(long)]
        sign: bool,
    },

    /// Sign a transaction body
    SignTx {
        /// Wallet name
        #[arg(long)]
        wallet: String,

        /// Account index
        #[arg(long, default_value = "0")]
        account: u32,

        /// Transaction body file
        #[arg(long)]
        tx_body_file: String,

        /// Output file for signed transaction
        #[arg(long)]
        out_file: String,

        /// Sign with stake key as well
        #[arg(long)]
        stake: bool,
    },

    /// Create a transaction witness
    WitnessTx {
        /// Wallet name
        #[arg(long)]
        wallet: String,

        /// Account index
        #[arg(long, default_value = "0")]
        account: u32,

        /// Transaction body file
        #[arg(long)]
        tx_body_file: String,

        /// Output file for witness
        #[arg(long)]
        out_file: String,

        /// Witness type (payment or stake)
        #[arg(long, default_value = "payment")]
        role: String,
    },

    /// Sign a message (CIP-8)
    SignMsg {
        /// Wallet name
        #[arg(long)]
        wallet: String,

        /// Account index
        #[arg(long, default_value = "0")]
        account: u32,

        /// Message file to sign
        #[arg(long)]
        msg_file: String,

        /// Output file for JSON signature
        #[arg(long)]
        out_file: String,

        /// Use stake key instead of payment key
        #[arg(long)]
        stake: bool,

        /// Hash the message before signing
        #[arg(long)]
        hashed: bool,
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
