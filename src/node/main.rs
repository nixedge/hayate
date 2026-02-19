// Hayate-Node (疾風ノード) - Full Cardano node with ledger state snapshots

use clap::Parser;
use tracing::{info, error, warn};
use std::path::PathBuf;
use anyhow::Result;

// Import from lib
use hayate::node::{NodeStorage, UtxoEntry, slot_to_epoch};
use hayate::indexer::Network;
use hayate::chain_sync::HayateSync;
use amaru_kernel::Point;
use pallas_network::miniprotocols::chainsync::NextResponse;
use pallas_crypto::nonce::generate_rolling_nonce;
use amaru_kernel::Hash;

#[derive(Parser, Debug)]
#[command(name = "hayate-node")]
#[command(about = "疾風ノード Hayate-Node - Full Cardano node with ledger state snapshots", long_about = None)]
struct Args {
    /// Database directory
    #[arg(short, long, default_value = "./data")]
    db_path: String,

    /// Network (mainnet, preprod, preview, sanchonet)
    #[arg(short, long, default_value = "preview")]
    network: String,

    /// Node socket path (for syncing blocks)
    #[arg(short, long)]
    socket: Option<String>,

    /// Start from slot
    #[arg(long)]
    from_slot: Option<u64>,

    /// Magic number (network ID)
    #[arg(long)]
    magic: Option<u64>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("hayate_node=info".parse()?)
                .add_directive("hayate=info".parse()?)
        )
        .init();

    let args = Args::parse();

    info!("疾風ノード Hayate-Node starting...");

    // Parse network
    let network = Network::from_str(&args.network)
        .ok_or_else(|| anyhow::anyhow!("Invalid network: {}", args.network))?;

    info!("Network: {}", network.as_str());
    info!("Database: {}", args.db_path);

    // Open node storage
    let mut storage = NodeStorage::open(PathBuf::from(&args.db_path), network.clone())?;

    // Check for existing tip
    if let Some((tip_slot, tip_hash)) = storage.get_chain_tip()? {
        info!("Resuming from slot {}, hash: {}", tip_slot, hex::encode(&tip_hash));
    } else {
        info!("Starting fresh sync");
    }

    // Determine socket path
    let socket_path = if let Some(socket) = args.socket {
        socket
    } else {
        // Default socket paths
        match network {
            Network::Preview => {
                std::env::var("CARDANO_NODE_SOCKET_PATH")
                    .unwrap_or_else(|_| "./cardano-node/node.socket".to_string())
            },
            Network::Mainnet => {
                std::env::var("CARDANO_NODE_SOCKET_PATH")
                    .unwrap_or_else(|_| "./cardano-node/node.socket".to_string())
            },
            _ => {
                return Err(anyhow::anyhow!("Please specify --socket for network {}", network.as_str()));
            }
        }
    };

    info!("Connecting to node socket: {}", socket_path);

    // Determine magic
    let magic = args.magic.unwrap_or_else(|| network.magic());

    info!("Network magic: {}", magic);

    // Connect to node via chain sync
    let start_point = if let Some((tip_slot, tip_hash)) = storage.get_chain_tip()? {
        info!("Resuming from slot {}", tip_slot);
        let hash_bytes: [u8; 32] = tip_hash.try_into()
            .map_err(|_| anyhow::anyhow!("Invalid hash length"))?;
        Point::Specific(tip_slot.into(), hash_bytes.into())
    } else {
        info!("Starting from origin");
        Point::Origin
    };

    info!("Connecting to chain sync...");
    let mut sync = HayateSync::connect_unix(&socket_path, magic, start_point).await?;
    info!("✅ Connected to Cardano node via chain sync");

    // Start processing blocks
    let mut blocks_processed = 0u64;
    let mut current_epoch = 0u64;

    // Initialize rolling nonce with Shelley genesis nonce for Preview network
    // This is the starting point for nonce evolution
    let mut rolling_nonce: Option<Hash<32>> = None;

    info!("🔄 Starting block processing...");

    loop {
        match sync.request_next().await? {
            NextResponse::RollForward(block_bytes, _tip) => {
                // Parse block using pallas
                match parse_block_with_nonce(&block_bytes) {
                    Ok((slot, block_hash, tx_count, vrf_output_opt)) => {
                        // Update rolling nonce if VRF is present
                        if let Some(vrf_output) = vrf_output_opt {
                            // VRF output should be at least 32 bytes
                            if vrf_output.len() >= 32 {
                                match rolling_nonce {
                                    Some(prev_nonce) => {
                                        // generate_rolling_nonce(prev: Hash<32>, vrf: &[u8]) -> Hash<32>
                                        rolling_nonce = Some(generate_rolling_nonce(prev_nonce, &vrf_output[..32]));
                                    }
                                    None => {
                                        // Initialize with Preview network genesis nonce
                                        // For now, using zero hash - TODO: use actual genesis nonce
                                        let init_nonce = Hash::<32>::from([0u8; 32]);
                                        rolling_nonce = Some(generate_rolling_nonce(init_nonce, &vrf_output[..32]));
                                        info!("🔐 Initializing rolling nonce from first VRF at slot {}", slot);
                                    }
                                }
                            } else {
                                warn!("VRF output too short: {} bytes", vrf_output.len());
                            }
                        }

                        // Process block
                        if let Err(e) = process_block_simple(&mut storage, slot, &block_hash, &block_bytes).await {
                            error!("Error processing block at slot {}: {}", slot, e);
                            continue;
                        }

                        blocks_processed += 1;

                        // Check for epoch transition
                        let epoch = slot_to_epoch(slot, &network);

                        // When we transition to a new epoch, snapshot the previous epoch
                        if epoch > current_epoch {
                            info!("🎯 Epoch transition detected: epoch {} → {} at slot {}",
                                current_epoch, epoch, slot);

                            // Store epoch nonce for the new epoch
                            if let Some(nonce) = &rolling_nonce {
                                let nonce_slice: &[u8] = nonce.as_ref();
                                if nonce_slice.len() == 32 {
                                    let mut nonce_bytes = [0u8; 32];
                                    nonce_bytes.copy_from_slice(nonce_slice);
                                    match storage.store_nonce(epoch, &nonce_bytes) {
                                        Ok(()) => {
                                            info!("🔐 Stored epoch nonce for epoch {}: {}",
                                                epoch, hex::encode(&nonce_bytes[..8]));
                                        }
                                        Err(e) => {
                                            error!("Failed to store epoch nonce: {}", e);
                                        }
                                    }
                                } else {
                                    error!("Invalid nonce length: {}", nonce_slice.len());
                                }
                            } else {
                                warn!("No rolling nonce available at epoch boundary");
                            }

                            // Snapshot stake distribution for the epoch we just entered
                            // This represents the active stake for epoch N
                            info!("📸 Creating stake distribution snapshot for epoch {}...", epoch);
                            match storage.snapshot_stake_distribution(epoch) {
                                Ok(stake_map) => {
                                    let total_stake: u64 = stake_map.values().sum();
                                    info!("✅ Stake snapshot complete: {} stake keys, {} total lovelace",
                                        stake_map.len(), total_stake);

                                    // Log top 10 stakes
                                    let mut stakes: Vec<_> = stake_map.iter().collect();
                                    stakes.sort_by(|a, b| b.1.cmp(a.1));
                                    info!("Top 10 stakes:");
                                    for (i, (cred, amount)) in stakes.iter().take(10).enumerate() {
                                        info!("  {}. {}: {} lovelace",
                                            i + 1, hex::encode(&cred[..8]), amount);
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to create stake snapshot: {}", e);
                                }
                            }

                            current_epoch = epoch;
                        }

                        // Update chain tip
                        storage.store_chain_tip(slot, &block_hash)?;

                        // Log progress
                        if blocks_processed % 1000 == 0 {
                            info!("Processed {} blocks, slot: {}, epoch: {}, txs: {}",
                                blocks_processed, slot, epoch, tx_count);
                        }
                    }
                    Err(e) => {
                        error!("Failed to parse block: {}", e);
                        continue;
                    }
                }
            }
            NextResponse::RollBackward(point, _tip) => {
                info!("⚠️  Rollback to {:?}", point);
                // TODO: Implement rollback logic
            }
            NextResponse::Await => {
                info!("Caught up, waiting for new blocks...");
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            }
        }
    }
}

/// Parse block and extract VRF nonce for epoch nonce calculation
fn parse_block_with_nonce(block_bytes: &[u8]) -> Result<(u64, Vec<u8>, usize, Option<Vec<u8>>)> {
    use pallas_traverse::MultiEraBlock;

    let block = MultiEraBlock::decode(block_bytes)
        .map_err(|e| anyhow::anyhow!("Failed to decode block: {}", e))?;

    let slot = block.slot();
    let hash = block.hash().to_vec();
    let tx_count = block.txs().len();

    // Extract VRF output from block header
    let vrf_output: Option<Vec<u8>> = if let Some(babbage_block) = block.as_babbage() {
        // Babbage era blocks have vrf_result in header_body
        let vrf_cert = &babbage_block.header.header_body.vrf_result;
        // VrfCert is (output, proof) - we need the output (field .0)
        Some(vrf_cert.0.to_vec())
    } else if let Some(alonzo_block) = block.as_alonzo() {
        // Alonzo era blocks have nonce_vrf field
        let vrf_cert = &alonzo_block.header.header_body.nonce_vrf;
        Some(vrf_cert.0.to_vec())
    } else {
        // Byron and epoch boundary blocks don't have VRF
        None
    };

    Ok((slot, hash, tx_count, vrf_output))
}

async fn process_block_simple(
    storage: &mut NodeStorage,
    _slot: u64,
    _block_hash: &[u8],
    block_bytes: &[u8],
) -> Result<()> {
    use pallas_traverse::MultiEraBlock;

    let block = MultiEraBlock::decode(block_bytes)?;

    // Process each transaction
    for tx in block.txs() {
        let tx_hash = tx.hash();

        // Process inputs (remove UTxOs)
        for input in tx.inputs() {
            let input_hash = input.hash();
            let input_index = input.index();

            storage.remove_utxo(input_hash.as_ref(), input_index as u32)?;
        }

        // Process outputs (add UTxOs)
        for (output_index, output) in tx.outputs().into_iter().enumerate() {
            let address_bytes = output.address()?.to_vec();
            let amount = output.value().coin();

            // Extract stake credential from address
            let stake_credential = extract_stake_credential(&address_bytes)?;

            // Parse multi-assets
            let mut assets = std::collections::HashMap::new();
            for policy_assets in output.value().assets() {
                let policy_id = hex::encode(policy_assets.policy().as_ref());

                for asset in policy_assets.assets() {
                    let asset_name = hex::encode(asset.name());
                    let asset_key = format!("{}.{}", policy_id, asset_name);
                    let amount = match &asset {
                        pallas_traverse::MultiEraAsset::AlonzoCompatibleOutput(_, _, amt) => *amt as u64,
                        pallas_traverse::MultiEraAsset::ConwayOutput(_, _, amt) => {
                            
                            u64::from(*amt)
                        }
                        _ => 0, // Shouldn't happen for outputs
                    };
                    assets.insert(asset_key, amount);
                }
            }

            // Parse datum and datum hash
            let (datum_hash, datum) = match output.datum() {
                Some(datum_option) => {
                    use pallas_primitives::conway::MintedDatumOption;
                    match datum_option {
                        MintedDatumOption::Hash(hash) => {
                            // Datum hash only (datum is stored separately on-chain)
                            (Some(hash.to_vec()), None)
                        }
                        MintedDatumOption::Data(inline_datum) => {
                            // Inline datum (post-Babbage)
                            
                            

                            // Get the raw bytes from the KeepRaw wrapper
                            let datum_bytes = inline_datum.raw_cbor().to_vec();

                            // Compute datum hash using Blake2b256
                            let mut hasher = pallas_crypto::hash::Hasher::<256>::new();
                            hasher.input(&datum_bytes);
                            let hash = hasher.finalize();

                            (Some(hash.to_vec()), Some(datum_bytes))
                        }
                    }
                }
                None => (None, None),
            };

            let utxo_entry = UtxoEntry {
                address: address_bytes,
                amount,
                assets,
                datum_hash,
                datum,
                script_ref: None, // TODO: Parse script ref
                stake_credential,
            };

            storage.insert_utxo(tx_hash.as_ref(), output_index as u32, &utxo_entry)?;
        }

        // TODO: Process certificates for delegations
    }

    Ok(())
}

fn extract_stake_credential(address: &[u8]) -> Result<Option<Vec<u8>>> {
    use pallas_addresses::{Address, ShelleyDelegationPart};

    let addr = Address::from_bytes(address)
        .map_err(|e| anyhow::anyhow!("Failed to parse address: {}", e))?;

    match addr {
        Address::Shelley(shelley_addr) => {
            match shelley_addr.delegation() {
                ShelleyDelegationPart::Key(key_hash) => {
                    Ok(Some(key_hash.to_vec()))
                }
                ShelleyDelegationPart::Script(script_hash) => {
                    Ok(Some(script_hash.to_vec()))
                }
                ShelleyDelegationPart::Null => {
                    Ok(None)
                }
                _ => Ok(None),
            }
        }
        _ => Ok(None), // Byron addresses don't have stake credentials
    }
}

// TODO: Implement block processing
// This will be added when chain_sync integration is complete
//
// async fn process_block(
//     storage: &mut NodeStorage,
//     block: &BlockType,
//     network: &Network,
// ) -> Result<()> {
//     // Process transactions, update UTxO set, track delegations, etc.
//     Ok(())
// }
