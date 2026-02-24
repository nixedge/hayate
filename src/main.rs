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
use pallas_network::miniprotocols::Point;
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

    // Get storage handle
    let networks = indexer.networks.read().await;
    let storage_handle = networks.get(&network)
        .ok_or_else(|| anyhow::anyhow!("Network storage not found"))?
        .clone();
    drop(networks); // Release read lock

    // Get minimum wallet tip for resume point
    // This ensures newly added wallets are indexed from their last known tip (or origin)
    let start_point = if let Some(tip) = storage_handle.get_min_wallet_tip(wallet_ids.clone()).await? {
        info!("Resuming from slot {} (minimum across {} wallets)", tip.slot, wallet_ids.len());
        Point::Specific(tip.slot, tip.hash)
    } else {
        info!("Starting from origin (new wallets or empty database)");
        Point::Origin
    };

    // Connect to node via Unix socket
    let magic = network.magic();
    let mut sync = HayateSync::connect(&socket_path, magic, start_point).await?;
    info!("✅ Connected to Cardano node");

    // Create block processor with storage handle
    let mut processor = BlockProcessor::new(storage_handle.clone()).await?;

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

    info!("🔄 Starting block processing...");

    // Process blocks - loop until shutdown signal
    let mut shutdown = Box::pin(tokio::signal::ctrl_c());
    let mut is_caught_up = false;
    let result: anyhow::Result<()> = loop {
        tokio::select! {
            _ = &mut shutdown => {
                info!("🛑 Received shutdown signal, saving tips...");
                break Ok(());
            }
            next_result = async {
                // Check agency and call appropriate method
                if sync.has_agency() {
                    sync.request_next().await
                } else {
                    sync.await_next().await
                }
            } => {
                let next = next_result?;

        // Process the response
        match next {
            NextResponse::RollForward(block_bytes, _tip) => {
                // Parse block to get slot and hash
                let block = MultiEraBlock::decode(&block_bytes)?;
                let slot = block.slot();
                let hash = block.hash();

                // Log new blocks only when we were previously caught up (waiting at tip)
                // This means we only log blocks that extend the chain after we've been idle
                if is_caught_up {
                    info!("📦 New block at slot {} - {}", slot, hex::encode(&hash.as_ref()[..8]));
                }

                is_caught_up = false;

                // Process block
                match processor.process_block(&block_bytes, slot, hash.as_ref()).await {
                    Ok(_stats) => {
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
                is_caught_up = false;
                info!("⚠️  Rollback to {:?}", point);
                match point {
                    Point::Specific(slot, _) => {
                        match processor.rollback_to(slot).await {
                            Ok(count) => info!("✓ Rolled back {} blocks", count),
                            Err(e) => tracing::error!("Failed to rollback: {}", e),
                        }
                    }
                    Point::Origin => {
                        info!("Rollback to origin requested");
                        match processor.rollback_to(0).await {
                            Ok(count) => info!("✓ Rolled back {} blocks to origin", count),
                            Err(e) => tracing::error!("Failed to rollback to origin: {}", e),
                        }
                    }
                }
            }
            NextResponse::Await => {
                if !is_caught_up {
                    tracing::trace!("Caught up - waiting for new blocks");
                    is_caught_up = true;
                }
                // The protocol state machine requires us to call recv_while_must_reply()
                // after receiving Await, but that call should block until a message arrives.
                // However, to avoid busy-looping if it returns immediately, add a small delay.
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
            }
            }
        }
    };

    // Save tips before exiting (whether shutdown, error, or normal exit)
    info!("💾 Saving final chain tips...");
    processor.save_current_tips().await?;

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

async fn handle_rollback_command(
    epoch: u64,
    network_str: Option<String>,
    db_path_str: Option<String>,
) -> anyhow::Result<()> {
    use indexer::block_processor::BlockProcessor;

    // Parse network
    let network = if let Some(net_str) = network_str {
        Network::from_str(&net_str)
            .ok_or_else(|| anyhow::anyhow!("Invalid network: {}", net_str))?
    } else {
        Network::Preview // Default
    };

    // Determine database path
    let db_path = if let Some(path) = db_path_str {
        PathBuf::from(path)
    } else {
        PathBuf::from("./.hayate")
    };

    info!("🔄 Rolling back {} to epoch {}", network.as_str(), epoch);

    // Calculate target slot from epoch
    let target_slot = epoch_to_slot(epoch, &network);
    info!("Target slot: {} (epoch {})", target_slot, epoch);

    // Open network storage
    let storage = indexer::NetworkStorage::open(db_path.clone(), network.clone())?;
    let (manager, handle) = indexer::StorageManager::new(storage);

    // Spawn storage manager
    tokio::spawn(async move {
        manager.run().await;
    });

    // Create block processor
    let mut processor = BlockProcessor::new(handle.clone()).await?;

    // Perform rollback
    let count = processor.rollback_to(target_slot).await?;

    info!("✅ Rolled back {} blocks", count);
    info!("💾 Saving chain tips...");

    // Save tips
    processor.save_current_tips().await?;

    info!("✅ Rollback complete");

    Ok(())
}

fn epoch_to_slot(epoch: u64, network: &Network) -> u64 {
    let epoch_length = match network {
        Network::Mainnet | Network::Preprod => 432_000,
        Network::Preview => 86_400,
        Network::SanchoNet => 86_400,
        Network::Custom(_) => 432_000,
    };
    epoch * epoch_length
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| {
                    // Default to info if RUST_LOG not set, but allow RUST_LOG to override
                    tracing_subscriber::EnvFilter::new("hayate=info,h2=warn")
                })
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
        Some(cli::Command::Rollback { epoch, network, db_path }) => {
            return handle_rollback_command(*epoch, network.clone(), db_path.clone()).await;
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

    // If socket is provided, start API in background and run chain sync in foreground
    if let Some(socket_path) = socket {
        info!("Socket: {}", socket_path);
        info!("Starting chain sync with UTxORPC API");

        // Spawn API server in background task
        let indexer_clone = Arc::clone(&indexer);
        let api_bind = config.api.bind.clone();
        let socket_clone = socket_path.clone();
        let network_clone = network.clone();
        tokio::spawn(async move {
            info!("🚀 Starting UTxORPC server on {}...", api_bind);
            if let Err(e) = api::start_utxorpc_server(indexer_clone, api_bind, network_clone, Some(socket_clone)).await {
                tracing::error!("API server error: {}", e);
            }
        });

        // Run chain sync in main task (this will block)
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
