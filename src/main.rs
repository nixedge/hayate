// Hayate (疾風) - Swift Cardano Indexer with UTxORPC API

mod mock_types;
mod wallet;
mod chain_sync;
mod keys;
mod rewards;
mod indexer;
mod api;
mod config;

use clap::Parser;
use tracing::info;
use std::sync::Arc;
use indexer::{HayateIndexer, Network};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "hayate")]
#[command(about = "疾風 Hayate - Swift Cardano indexer with UTxORPC", long_about = None)]
struct Args {
    /// Configuration file
    #[arg(short, long)]
    config: Option<String>,
    
    /// Database directory (overrides config)
    #[arg(short, long)]
    db_path: Option<String>,
    
    /// Networks to index (comma-separated: mainnet,preprod,preview,sanchonet)
    /// Overrides config file
    #[arg(short, long)]
    networks: Option<String>,
    
    /// UTxORPC API bind address (overrides config)
    #[arg(long)]
    api_bind: Option<String>,
    
    /// Gap limit for address discovery (overrides config)
    #[arg(long)]
    gap_limit: Option<u32>,
    
    /// Generate default config file
    #[arg(long)]
    generate_config: Option<String>,
    
    /// Start from genesis
    #[arg(long)]
    from_genesis: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("hayate=info".parse()?)
                .add_directive("h2=warn".parse()?)  // Reduce gRPC noise
        )
        .init();
    
    let args = Args::parse();
    
    // Handle config generation
    if let Some(config_path) = args.generate_config {
        info!("Generating default config at: {}", config_path);
        config::HayateConfig::generate_default(&config_path)?;
        info!("✅ Config file created: {}", config_path);
        info!("Edit the file and run: hayate --config {}", config_path);
        return Ok(());
    }
    
    // Load configuration
    let mut config = if let Some(config_path) = args.config {
        info!("Loading config from: {}", config_path);
        config::HayateConfig::load(&config_path)?
    } else {
        info!("Using default configuration");
        config::HayateConfig::default()
    };
    
    // Apply CLI overrides
    if let Some(db_path) = args.db_path {
        config.data_dir = PathBuf::from(db_path);
    }
    if let Some(gap_limit) = args.gap_limit {
        config.gap_limit = gap_limit;
    }
    if let Some(api_bind) = args.api_bind {
        config.api.bind = api_bind;
    }
    
    info!("疾風 Hayate starting...");
    info!("Database: {:?}", config.data_dir);
    info!("UTxORPC API: {}", config.api.bind);
    info!("Gap limit: {}", config.gap_limit);
    
    // Create indexer
    let indexer = Arc::new(HayateIndexer::new(config.data_dir.clone(), config.gap_limit)?);
    
    // Add networks from config or CLI
    let networks_to_enable = if let Some(networks_str) = args.networks {
        // CLI override
        networks_str.split(',')
            .filter_map(|s| Network::from_str(s.trim()))
            .collect()
    } else {
        // From config file
        config.networks.iter()
            .filter(|(_, cfg)| cfg.enabled)
            .filter_map(|(name, _)| Network::from_str(name))
            .collect::<Vec<_>>()
    };
    
    for network in networks_to_enable {
        info!("Enabling network: {}", network.as_str());
        
        // Get network config
        let net_config = config.networks.get(network.as_str());
        
        if let Some(cfg) = net_config {
            info!("  Relay: {}", cfg.relay);
            info!("  Magic: {}", cfg.magic);
            
            if let Some(ref genesis) = cfg.genesis_file {
                info!("  Genesis: {:?}", genesis);
            }
        }
        
        indexer.add_network(network, config.data_dir.clone()).await?;
    }
    
    // Start UTxORPC server
    info!("🚀 Starting UTxORPC server...");
    api::start_utxorpc_server(indexer, config.api.bind).await?;
    
    Ok(())
}
