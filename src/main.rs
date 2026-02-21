// Hayate (疾風) - Swift Cardano Indexer with UTxORPC API

mod cli;
mod mock_types;
mod wallet;
mod gpg;
mod chain_sync;
mod keys;
mod rewards;
mod indexer;
mod api;
mod config;
mod wallet_stats;

use clap::Parser;
use tracing::info;
use std::sync::Arc;
use indexer::{HayateIndexer, Network};
use std::path::PathBuf;
use cli::Args;
use chain_sync::HayateSync;
use pallas_network::miniprotocols::chainsync::NextResponse;
use amaru_kernel::Point as AmaruPoint;
use pallas_network::miniprotocols::Point as PallasPoint;
use pallas_traverse::MultiEraBlock;

/// Run chain sync from a node socket
async fn run_chain_sync(
    indexer: Arc<HayateIndexer>,
    network: Network,
    socket_path: String,
    tokens: Vec<config::TokenConfig>,
) -> anyhow::Result<()> {
    use indexer::block_processor::BlockProcessor;

    info!("🔄 Starting chain sync from socket: {}", socket_path);

    // Get wallet IDs for per-wallet tip tracking
    let wallet_ids = indexer.account_xpubs.read().await.clone();

    // Get network storage
    let networks = indexer.networks.read().await;
    let storage = networks.get(&network)
        .ok_or_else(|| anyhow::anyhow!("Network storage not found"))?;

    // Get minimum wallet tip for resume point
    // This ensures newly added wallets are indexed from their last known tip (or origin)
    let start_point = if let Some(tip) = storage.get_min_wallet_tip(&wallet_ids)? {
        info!("Resuming from slot {} (minimum across {} wallets)", tip.slot, wallet_ids.len());
        let hash_bytes: [u8; 32] = tip.hash.try_into()
            .map_err(|_| anyhow::anyhow!("Invalid hash length"))?;
        AmaruPoint::Specific(tip.slot.into(), hash_bytes.into())
    } else {
        info!("Starting from origin (new wallets or empty database)");
        AmaruPoint::Origin
    };

    drop(networks); // Release read lock

    // Connect to node
    let magic = network.magic();
    let mut sync = HayateSync::connect_unix(&socket_path, magic, start_point).await?;
    info!("✅ Connected to Cardano node");

    // Create block processor with wallet IDs
    let mut networks = indexer.networks.write().await;
    let storage = networks.remove(&network)
        .ok_or_else(|| anyhow::anyhow!("Network storage not found"))?;
    let mut processor = BlockProcessor::new(storage);

    // Add wallet IDs to processor for per-wallet tip tracking
    for wallet_id in &wallet_ids {
        processor.add_wallet_id(wallet_id.clone());
    }

    // Add tracked tokens to processor
    for token in &tokens {
        processor.add_tracked_token(token.clone());
    }

    if !tokens.is_empty() {
        info!("Tracking {} native token(s)", tokens.len());
    }

    drop(networks); // Release write lock

    info!("🔄 Starting block processing...");

    // Process blocks - loop until shutdown signal
    let result: anyhow::Result<()> = loop {
        tokio::select! {
            // Handle Ctrl+C for graceful shutdown
            _ = tokio::signal::ctrl_c() => {
                info!("🛑 Received shutdown signal, saving tips...");
                break Ok(());
            }

            // Process next block
            next = sync.request_next() => {
                match next? {
                    NextResponse::RollForward(block_bytes, _tip) => {
                        // Parse block to get slot and hash
                        let block = MultiEraBlock::decode(&block_bytes)?;
                        let slot = block.slot();
                        let hash = block.hash();

                        // Process block
                        match processor.process_block(&block_bytes, slot, hash.as_ref()) {
                            Ok(_stats) => {
                                // Only log progress every 1000 blocks to reduce overhead
                                // The processor already logs every 100 blocks with more detail

                                // Broadcast block update
                                indexer.broadcast_block(indexer::BlockUpdate {
                                    network: network.clone(),
                                    height: 0, // TODO: track height
                                    slot,
                                    hash: hash.as_ref().to_vec(),
                                    tx_hashes: Vec::new(), // TODO: collect tx hashes
                                });
                            }
                            Err(e) => {
                                tracing::error!("Error processing block at slot {}: {}", slot, e);
                                continue;
                            }
                        }
                    }
                    NextResponse::RollBackward(point, _tip) => {
                        info!("⚠️  Rollback to {:?}", point);
                        match point {
                            PallasPoint::Specific(slot, _) => {
                                match processor.rollback_to(slot) {
                                    Ok(count) => info!("✓ Rolled back {} blocks", count),
                                    Err(e) => tracing::error!("Failed to rollback: {}", e),
                                }
                            }
                            PallasPoint::Origin => {
                                info!("Rollback to origin requested");
                                match processor.rollback_to(0) {
                                    Ok(count) => info!("✓ Rolled back {} blocks to origin", count),
                                    Err(e) => tracing::error!("Failed to rollback to origin: {}", e),
                                }
                            }
                        }
                    }
                    NextResponse::Await => {
                        info!("Caught up, waiting for new blocks...");
                        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                    }
                }
            }
        }
    };

    // Save tips before exiting (whether shutdown, error, or normal exit)
    info!("💾 Saving final chain tips...");
    processor.save_current_tips()?;

    result
}

async fn handle_wallet_command(wallet_cmd: &cli::WalletCommand, args: &Args) -> anyhow::Result<()> {
    // Determine wallet directory
    let wallet_dir = args.db_path.as_ref()
        .map(|p| PathBuf::from(p).join("wallets"))
        .unwrap_or_else(|| PathBuf::from("./hayate-wallets"));

    // Load config to get UTxORPC endpoint
    let config = if let Some(ref config_path) = args.config {
        config::HayateConfig::load(config_path)?
    } else {
        config::HayateConfig::default()
    };

    let utxorpc_endpoint = Some(format!("http://{}", config.api.bind));

    // Use the wallet CLI handler
    wallet::handle_wallet_command(wallet_cmd, wallet_dir, utxorpc_endpoint).await
}

async fn handle_config_command(config_cmd: &cli::ConfigCommand) -> anyhow::Result<()> {
    match config_cmd {
        cli::ConfigCommand::Generate { output } => {
            info!("Generating default config at: {}", output);
            config::HayateConfig::generate_default(output)?;
            info!("✅ Config file created: {}", output);
            info!("Edit the file and run: hayate sync --config {}", output);
            Ok(())
        }
    }
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

    // Handle commands
    match &args.command {
        Some(cli::Command::Wallet { wallet_cmd }) => {
            return handle_wallet_command(wallet_cmd, &args).await;
        }
        Some(cli::Command::Config { config_cmd }) => {
            return handle_config_command(config_cmd).await;
        }
        Some(cli::Command::Sync { .. }) | None => {
            // Continue with sync (default behavior)
        }
    }
    
    // Load configuration
    let mut config = if let Some(config_path) = args.config {
        info!("Loading config from: {}", config_path);
        config::HayateConfig::load(&config_path)?
    } else {
        info!("Using default configuration");
        config::HayateConfig::default()
    };
    
    // Apply CLI overrides from global args
    if let Some(db_path) = &args.db_path {
        config.data_dir = PathBuf::from(db_path);
    }

    // Apply overrides from Sync command if present
    if let Some(cli::Command::Sync { gap_limit, api_bind, .. }) = &args.command {
        if let Some(gap_limit) = gap_limit {
            config.gap_limit = *gap_limit;
        }
        if let Some(api_bind) = api_bind {
            config.api.bind = api_bind.clone();
        }
    }
    
    info!("疾風 Hayate starting...");
    info!("Database: {:?}", config.data_dir);
    info!("UTxORPC API: {}", config.api.bind);
    info!("Gap limit: {}", config.gap_limit);

    // Create indexer
    let indexer = Arc::new(HayateIndexer::new(config.data_dir.clone(), config.gap_limit)?);

    // Determine network to use
    let socket = if let Some(cli::Command::Sync { socket, .. }) = &args.command {
        socket.as_ref()
    } else {
        None
    };

    let network = if let Some(network_str) = &args.network {
        Network::from_str(network_str)
            .ok_or_else(|| anyhow::anyhow!("Invalid network: {}", network_str))?
    } else if socket.is_some() {
        return Err(anyhow::anyhow!("--network is required when using --socket"));
    } else {
        // From config file - use first enabled network
        config.networks.iter()
            .find(|(_, cfg)| cfg.enabled)
            .and_then(|(name, _)| Network::from_str(name))
            .ok_or_else(|| anyhow::anyhow!("No network enabled in config"))?
    };

    info!("Network: {}", network.as_str());
    info!("Magic: {}", network.magic());

    // Add network storage
    indexer.add_network(network.clone(), config.data_dir.clone()).await?;

    // Load wallets and tokens from config
    if config.wallets.is_empty() && config.tokens.is_empty() {
        return Err(anyhow::anyhow!(
            "Nothing configured to index. Please configure at least one of:\n\
             - Wallet xpubs (in 'wallets' array)\n\
             - Native tokens to track (in 'tokens' array)\n\
             - Smart contracts (coming soon)\n\n\
             Generate a default config with: hayate config generate config.toml"
        ));
    }

    for wallet_xpub in &config.wallets {
        info!("Loading wallet: {}", wallet_xpub);
        indexer.add_account(wallet_xpub.clone()).await?;
    }

    info!("Loaded {} wallet(s)", config.wallets.len());

    // If socket is provided, run in sync mode
    if let Some(socket_path) = socket {
        info!("Socket: {}", socket_path);
        info!("Running in sync mode (no API server)");

        // Run chain sync (this will block forever)
        run_chain_sync(indexer, network, socket_path.clone(), config.tokens.clone()).await?;

        return Ok(());
    }

    // No socket provided - run in API-only mode
    info!("Running in API-only mode (reading from existing DB)");

    // Get socket path from network config (for GetBlockByHash support)
    let socket_path = config.networks.get(network.as_str())
        .and_then(|cfg| cfg.socket_path.as_ref())
        .map(|p| p.to_string_lossy().to_string());

    if socket_path.is_none() {
        info!("⚠️  No socket_path configured for {} - GetBlockByHash will not be available", network.as_str());
        info!("💡 Add socket_path to network config to enable block-by-hash queries");
    }

    info!("🚀 Starting UTxORPC server on {}...", config.api.bind);
    api::start_utxorpc_server(indexer, config.api.bind, network, socket_path).await?;

    Ok(())
}
