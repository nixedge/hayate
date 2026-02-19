// Hayate (疾風) - Swift Cardano Indexer with UTxORPC API

mod cli;
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
use cli::Args;
use chain_sync::HayateSync;
use pallas_network::miniprotocols::chainsync::NextResponse;
use amaru_kernel::Point;
use pallas_traverse::MultiEraBlock;

/// Run chain sync from a node socket
async fn run_chain_sync(
    indexer: Arc<HayateIndexer>,
    network: Network,
    socket_path: String,
) -> anyhow::Result<()> {
    use indexer::block_processor::BlockProcessor;

    info!("🔄 Starting chain sync from socket: {}", socket_path);

    // Get network storage
    let networks = indexer.networks.read().await;
    let storage = networks.get(&network)
        .ok_or_else(|| anyhow::anyhow!("Network storage not found"))?;

    // Get chain tip for resume point
    let start_point = if let Some(tip) = storage.get_chain_tip()? {
        info!("Resuming from slot {}", tip.slot);
        let hash_bytes: [u8; 32] = tip.hash.try_into()
            .map_err(|_| anyhow::anyhow!("Invalid hash length"))?;
        Point::Specific(tip.slot.into(), hash_bytes.into())
    } else {
        info!("Starting from origin");
        Point::Origin
    };

    drop(networks); // Release read lock

    // Connect to node
    let magic = network.magic();
    let mut sync = HayateSync::connect_unix(&socket_path, magic, start_point).await?;
    info!("✅ Connected to Cardano node");

    // Create block processor
    let mut networks = indexer.networks.write().await;
    let storage = networks.remove(&network)
        .ok_or_else(|| anyhow::anyhow!("Network storage not found"))?;
    let mut processor = BlockProcessor::new(storage);
    drop(networks); // Release write lock

    info!("🔄 Starting block processing...");

    // Process blocks
    loop {
        match sync.request_next().await? {
            NextResponse::RollForward(block_bytes, _tip) => {
                // Parse block to get slot and hash
                let block = MultiEraBlock::decode(&block_bytes)?;
                let slot = block.slot();
                let hash = block.hash();

                // Process block
                match processor.process_block(&block_bytes, slot, hash.as_ref()) {
                    Ok(stats) => {
                        if stats.tx_count > 0 {
                            info!(
                                "Block {} at slot {}: {} txs, {} UTxOs created, {} spent",
                                hex::encode(&hash.as_ref()[..8]),
                                slot,
                                stats.tx_count,
                                stats.utxos_created,
                                stats.utxos_spent
                            );
                        }

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
                    Point::Specific(slot, _) => {
                        let target_slot: u64 = slot.into();
                        match processor.rollback_to(target_slot) {
                            Ok(count) => info!("✓ Rolled back {} blocks", count),
                            Err(e) => tracing::error!("Failed to rollback: {}", e),
                        }
                    }
                    Point::Origin => {
                        info!("Rollback to origin requested");
                        // TODO: Handle rollback to origin
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

    // Determine network to use
    let network = if let Some(network_str) = args.network {
        Network::from_str(&network_str)
            .ok_or_else(|| anyhow::anyhow!("Invalid network: {}", network_str))?
    } else if args.socket.is_some() {
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

    // If socket is provided, run in sync mode
    if let Some(socket_path) = args.socket {
        info!("Socket: {}", socket_path);
        info!("Running in sync mode (no API server)");

        // Run chain sync (this will block forever)
        run_chain_sync(indexer, network, socket_path).await?;

        return Ok(());
    }

    // No socket provided - run in API-only mode
    info!("Running in API-only mode (reading from existing DB)");
    info!("🚀 Starting UTxORPC server on {}...", config.api.bind);
    api::start_utxorpc_server(indexer, config.api.bind).await?;

    Ok(())
}
