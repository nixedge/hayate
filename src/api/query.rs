// UTxORPC Query Service implementation

use tonic::{Request, Response, Status};
use crate::indexer::{StorageHandle, ChainTip};

// Include generated proto code
#[allow(clippy::module_inception)]
pub mod query {
    tonic::include_proto!("utxorpc.query.v1");
}

use query::{
    query_service_server::QueryService,
    ReadUtxosRequest, ReadUtxosResponse, Utxo,
    SearchUtxosRequest, SearchUtxosResponse,
    ReadParamsRequest, ReadParamsResponse,
    GetChainTipRequest, GetChainTipResponse,
    GetTxHistoryRequest, GetTxHistoryResponse,
    ReadUtxoEventsRequest, ReadUtxoEventsResponse,
    GetBlockByHashRequest, GetBlockByHashResponse,
};

pub struct QueryServiceImpl {
    storage: StorageHandle,
    socket_path: Option<String>,
    magic: u64,
}

/// Helper function to format address as bech32 for logging
/// Automatically detects mainnet (addr) vs testnet (addr_test) from the address bytes
fn format_address_bech32(addr_hex: &str) -> String {
    // Try to decode and format as bech32, fall back to hex on error
    hex::decode(addr_hex)
        .ok()
        .and_then(|bytes| {
            use pallas_addresses::Address;
            Address::from_bytes(&bytes).ok()
        })
        .and_then(|addr| addr.to_bech32().ok())
        .unwrap_or_else(|| addr_hex.to_string())
}

impl QueryServiceImpl {
    #[allow(dead_code)]
    pub fn new(storage: StorageHandle) -> Self {
        Self {
            storage,
            socket_path: None,
            magic: 0,
        }
    }

    pub fn new_with_node(storage: StorageHandle, socket_path: String, magic: u64) -> Self {
        Self {
            storage,
            socket_path: Some(socket_path),
            magic,
        }
    }
}

#[tonic::async_trait]
impl QueryService for QueryServiceImpl {
    async fn read_utxos(
        &self,
        request: Request<ReadUtxosRequest>,
    ) -> Result<Response<ReadUtxosResponse>, Status> {
        let req = request.into_inner();

        tracing::debug!(
            "ReadUtxos request for {} addresses: [{}]",
            req.addresses.len(),
            req.addresses.iter()
                .take(5)
                .map(|a| format_address_bech32(&hex::encode(a)))
                .collect::<Vec<_>>()
                .join(", ")
        );

        let mut utxos = Vec::new();

        // Convert addresses to hex for lookup
        let address_hexes: Vec<String> = req.addresses.iter()
            .map(hex::encode)
            .collect();

        // Use address index to efficiently find UTxOs
        for addr_hex in &address_hexes {
            tracing::debug!("Looking up UTxOs for address: {}", format_address_bech32(addr_hex));

            // Get UTxO keys for this address from index
            let utxo_keys = self.storage.get_utxos_for_address(addr_hex.clone()).await
                .map_err(|e| Status::internal(format!("Failed to query address index: {}", e)))?;

            tracing::debug!("Found {} UTxOs for address {}", utxo_keys.len(), format_address_bech32(addr_hex));

            // Retrieve full UTxO data for each key
            for utxo_key in utxo_keys {
                if let Some(utxo_data) = self.storage.get_utxo(utxo_key.clone()).await
                    .map_err(|e| Status::internal(format!("Failed to read UTxO: {}", e)))? {

                    // Parse UTxO JSON data
                    let utxo_json: serde_json::Value = serde_json::from_slice(utxo_data.as_ref())
                        .map_err(|e| Status::internal(format!("Failed to parse UTxO data: {}", e)))?;

                    // Extract fields
                    let tx_hash = utxo_json.get("tx_hash")
                        .and_then(|v| v.as_str())
                        .and_then(|s| hex::decode(s).ok())
                        .ok_or_else(|| Status::internal("Invalid tx_hash"))?;

                    let output_index = utxo_json.get("output_index")
                        .and_then(|v| v.as_u64())
                        .ok_or_else(|| Status::internal("Invalid output_index"))? as u32;

                    let address = utxo_json.get("address")
                        .and_then(|v| v.as_str())
                        .and_then(|s| hex::decode(s).ok())
                        .ok_or_else(|| Status::internal("Invalid address"))?;

                    let amount = utxo_json.get("amount")
                        .and_then(|v| v.as_u64())
                        .ok_or_else(|| Status::internal("Invalid amount"))?;

                    // Extract multi-assets
                    let assets = if let Some(assets_obj) = utxo_json.get("assets").and_then(|v| v.as_object()) {
                        assets_obj.iter().filter_map(|(asset_key, amount_val)| {
                            let amount = amount_val.as_u64()?;
                            // asset_key format: "policy_id.asset_name"
                            let (policy_hex, asset_name_hex) = asset_key.split_once('.')?;
                            let policy_id = hex::decode(policy_hex).ok()?;
                            let asset_name = hex::decode(asset_name_hex).ok()?;

                            Some(query::Asset {
                                policy_id,
                                asset_name,
                                amount,
                            })
                        }).collect()
                    } else {
                        vec![]
                    };

                    // Extract datum hash
                    let datum_hash = utxo_json.get("datum_hash")
                        .and_then(|v| v.as_str())
                        .and_then(|s| hex::decode(s).ok())
                        .unwrap_or_default();

                    // Extract inline datum
                    let datum = utxo_json.get("datum")
                        .and_then(|v| v.as_str())
                        .and_then(|s| hex::decode(s).ok())
                        .unwrap_or_default();

                    // Extract creation metadata
                    let created_at_slot = utxo_json.get("slot")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);

                    let created_at_block_hash = utxo_json.get("block_hash")
                        .and_then(|v| v.as_str())
                        .and_then(|s| hex::decode(s).ok())
                        .unwrap_or_default();

                    let created_at_tx_index = utxo_json.get("tx_index")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as u32;

                    let created_at_block_timestamp = utxo_json.get("block_timestamp")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);

                    // Create Utxo proto message
                    utxos.push(Utxo {
                        tx_hash,
                        output_index,
                        address,
                        amount,
                        assets,
                        datum_hash,
                        datum,
                        created_at_slot,
                        created_at_block_hash,
                        created_at_tx_index,
                        created_at_block_timestamp,
                    });
                }
            }
        }

        // Get chain tip
        let tip = self.storage.get_chain_tip().await
            .map_err(|e| Status::internal(format!("Failed to get chain tip: {}", e)))?
            .unwrap_or(ChainTip {
                height: 0,
                slot: 0,
                hash: vec![],
                timestamp: 0,
            });

        tracing::debug!(
            "ReadUtxos response: {} UTxOs, tip slot: {}",
            utxos.len(),
            tip.slot
        );

        Ok(Response::new(ReadUtxosResponse {
            items: utxos,
            ledger_tip: tip.hash,
        }))
    }
    
    async fn search_utxos(
        &self,
        request: Request<SearchUtxosRequest>,
    ) -> Result<Response<SearchUtxosResponse>, Status> {
        let req = request.into_inner();

        tracing::debug!("SearchUtxos with pattern: {}", req.pattern);

        let pattern = req.pattern.to_lowercase();

        // Check if pattern is a valid hex string (policy ID or address prefix)
        if pattern != "*" && !pattern.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(Status::invalid_argument("Pattern must be hex or '*'"));
        }

        let mut utxos = Vec::new();

        // Pattern matching:
        // - "*" = all UTxOs (not implemented)
        // - hex string = could be policy ID or address prefix

        if pattern == "*" {
            tracing::warn!("Full UTxO scan not yet implemented");
            return Ok(Response::new(SearchUtxosResponse {
                items: vec![],
            }));
        }

        // Try to interpret as policy ID (56 hex chars = 28 bytes)
        if pattern.len() == 56 {
            tracing::info!("Interpreting pattern as policy ID: {}", pattern);

            // Get all transactions containing this policy ID
            let tx_hashes = self.storage.get_txs_for_policy(pattern.clone()).await
                .map_err(|e| Status::internal(format!("Failed to query policy index: {}", e)))?;

            tracing::debug!("Found {} transactions for policy {}", tx_hashes.len(), pattern);

            // For each transaction, try to find UTxOs containing the policy
            for tx_hash in tx_hashes {
                // Try reasonable output indices (most transactions have < 100 outputs)
                for output_idx in 0..100 {
                    let utxo_key = format!("{}#{}", tx_hash, output_idx);

                    if let Some(utxo_data) = self.storage.get_utxo(utxo_key.clone()).await
                        .map_err(|e| Status::internal(format!("Failed to read UTxO: {}", e)))? {

                        // Parse UTxO JSON data
                        let utxo_json: serde_json::Value = serde_json::from_slice(utxo_data.as_ref())
                            .map_err(|e| Status::internal(format!("Failed to parse UTxO data: {}", e)))?;

                        // Check if this UTxO contains the policy ID we're looking for
                        let contains_policy = if let Some(assets_obj) = utxo_json.get("assets").and_then(|v| v.as_object()) {
                            assets_obj.keys().any(|asset_key| {
                                // asset_key format: "policy_id.asset_name"
                                asset_key.starts_with(&pattern)
                            })
                        } else {
                            false
                        };

                        if !contains_policy {
                            continue;
                        }

                        // Extract fields for UTxO response
                        let tx_hash_bytes = utxo_json.get("tx_hash")
                            .and_then(|v| v.as_str())
                            .and_then(|s| hex::decode(s).ok())
                            .ok_or_else(|| Status::internal("Invalid tx_hash"))?;

                        let output_index = utxo_json.get("output_index")
                            .and_then(|v| v.as_u64())
                            .ok_or_else(|| Status::internal("Invalid output_index"))? as u32;

                        let address = utxo_json.get("address")
                            .and_then(|v| v.as_str())
                            .and_then(|s| hex::decode(s).ok())
                            .ok_or_else(|| Status::internal("Invalid address"))?;

                        let amount = utxo_json.get("amount")
                            .and_then(|v| v.as_u64())
                            .ok_or_else(|| Status::internal("Invalid amount"))?;

                        // Extract multi-assets
                        let assets = if let Some(assets_obj) = utxo_json.get("assets").and_then(|v| v.as_object()) {
                            assets_obj.iter().filter_map(|(asset_key, amount_val)| {
                                let amount = amount_val.as_u64()?;
                                let (policy_hex, asset_name_hex) = asset_key.split_once('.')?;
                                let policy_id = hex::decode(policy_hex).ok()?;
                                let asset_name = hex::decode(asset_name_hex).ok()?;

                                Some(query::Asset {
                                    policy_id,
                                    asset_name,
                                    amount,
                                })
                            }).collect()
                        } else {
                            vec![]
                        };

                        // Extract datum fields
                        let datum_hash = utxo_json.get("datum_hash")
                            .and_then(|v| v.as_str())
                            .and_then(|s| hex::decode(s).ok())
                            .unwrap_or_default();

                        let datum = utxo_json.get("datum")
                            .and_then(|v| v.as_str())
                            .and_then(|s| hex::decode(s).ok())
                            .unwrap_or_default();

                        // Extract creation metadata
                        let created_at_slot = utxo_json.get("slot")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);

                        let created_at_block_hash = utxo_json.get("block_hash")
                            .and_then(|v| v.as_str())
                            .and_then(|s| hex::decode(s).ok())
                            .unwrap_or_default();

                        let created_at_tx_index = utxo_json.get("tx_index")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0) as u32;

                        let created_at_block_timestamp = utxo_json.get("block_timestamp")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);

                        utxos.push(query::Utxo {
                            tx_hash: tx_hash_bytes,
                            output_index,
                            address,
                            amount,
                            assets,
                            datum_hash,
                            datum,
                            created_at_slot,
                            created_at_block_hash,
                            created_at_tx_index,
                            created_at_block_timestamp,
                        });
                    } else {
                        // No more outputs for this transaction
                        break;
                    }
                }
            }

            tracing::debug!("Returning {} UTxOs for policy {}", utxos.len(), pattern);
        } else {
            // Treat as address prefix
            tracing::info!("Pattern-based search for addresses starting with: {}", pattern);
            tracing::warn!("Address prefix search not yet implemented");
        }

        Ok(Response::new(SearchUtxosResponse {
            items: utxos,
        }))
    }

    async fn read_params(
        &self,
        _request: Request<ReadParamsRequest>,
    ) -> Result<Response<ReadParamsResponse>, Status> {
        tracing::debug!("ReadParams request");

        let tip = self.storage.get_chain_tip().await
            .map_err(|e| Status::internal(format!("Failed to get chain tip: {}", e)))?
            .unwrap_or(ChainTip {
                height: 0,
                slot: 0,
                hash: vec![],
                timestamp: 0,
            });

        // Query protocol parameters
        let protocol_params = if let Some(ref socket_path) = self.socket_path {
            let mut query = crate::node::ProtocolParamQuery::new(socket_path.clone(), self.magic);
            match query.query_current_params().await {
                Ok(params) => {
                    // Convert to proto format
                    Some(query::read_params_response::ProtocolParams {
                        min_fee_a: params.min_fee_a,
                        min_fee_b: params.min_fee_b,
                        max_tx_size: params.max_tx_size,
                        max_block_body_size: params.max_block_body_size,
                        utxo_cost_per_byte: params.utxo_cost_per_byte,
                        min_utxo_lovelace: params.min_utxo_lovelace.unwrap_or(0),
                        price_memory: params.price_memory.map(|r| {
                            query::read_params_response::protocol_params::Rational {
                                numerator: r.numerator,
                                denominator: r.denominator,
                            }
                        }),
                        price_steps: params.price_steps.map(|r| {
                            query::read_params_response::protocol_params::Rational {
                                numerator: r.numerator,
                                denominator: r.denominator,
                            }
                        }),
                        max_tx_execution_units: params.max_tx_execution_units.map(|u| {
                            query::read_params_response::protocol_params::ExUnits {
                                mem: u.mem,
                                steps: u.steps,
                            }
                        }),
                        max_block_execution_units: params.max_block_execution_units.map(|u| {
                            query::read_params_response::protocol_params::ExUnits {
                                mem: u.mem,
                                steps: u.steps,
                            }
                        }),
                        key_deposit: params.key_deposit,
                        pool_deposit: params.pool_deposit,
                        min_pool_cost: params.min_pool_cost,
                        epoch: params.epoch,
                    })
                }
                Err(e) => {
                    tracing::warn!("Failed to query protocol parameters: {}, returning None", e);
                    None
                }
            }
        } else {
            tracing::warn!("No socket_path configured, returning None for protocol params");
            None
        };

        tracing::debug!("ReadParams response: slot={}, hash={}", tip.slot, hex::encode(&tip.hash));

        Ok(Response::new(ReadParamsResponse {
            slot: tip.slot,
            hash: tip.hash,
            protocol_params,
        }))
    }

    async fn get_chain_tip(
        &self,
        _request: Request<GetChainTipRequest>,
    ) -> Result<Response<GetChainTipResponse>, Status> {
        tracing::debug!("GetChainTip request");

        let tip = self.storage.get_chain_tip().await
            .map_err(|e| Status::internal(format!("Failed to get chain tip: {}", e)))?
            .unwrap_or(ChainTip {
                height: 0,
                slot: 0,
                hash: vec![],
                timestamp: 0,
            });

        tracing::debug!(
            "GetChainTip response: height={}, slot={}, hash={}",
            tip.height,
            tip.slot,
            hex::encode(&tip.hash)
        );

        Ok(Response::new(GetChainTipResponse {
            height: tip.height,
            slot: tip.slot,
            hash: tip.hash,
            timestamp: tip.timestamp,
        }))
    }

    async fn get_tx_history(
        &self,
        request: Request<GetTxHistoryRequest>,
    ) -> Result<Response<GetTxHistoryResponse>, Status> {
        let req = request.into_inner();

        let address_hex = hex::encode(&req.address);
        tracing::debug!("GetTxHistory for address: {}", format_address_bech32(&address_hex));

        // Get transaction hashes for this address
        let tx_hashes_hex = self.storage.get_tx_history_for_address(address_hex).await
            .map_err(|e| Status::internal(format!("Failed to get tx history: {}", e)))?;

        // Convert to bytes
        let mut tx_hashes = Vec::new();
        for tx_hash_hex in tx_hashes_hex {
            if let Ok(tx_hash_bytes) = hex::decode(&tx_hash_hex) {
                tx_hashes.push(tx_hash_bytes);
            }
        }

        // Apply max_txs limit if specified
        let max_txs = req.max_txs as usize;
        if max_txs > 0 && tx_hashes.len() > max_txs {
            tx_hashes.truncate(max_txs);
        }

        tracing::debug!("Returning {} transactions", tx_hashes.len());

        Ok(Response::new(GetTxHistoryResponse {
            tx_hashes,
        }))
    }

    async fn read_utxo_events(
        &self,
        request: Request<ReadUtxoEventsRequest>,
    ) -> Result<Response<ReadUtxoEventsResponse>, Status> {
        let req = request.into_inner();

        tracing::debug!(
            "ReadUtxoEvents: slots {}-{}, address_filters: {}, max: {}",
            req.start_slot,
            req.end_slot,
            req.addresses.len(),
            req.max_events
        );

        let mut events = Vec::new();

        // Convert address filters to hex for comparison
        let address_filters: Vec<String> = req.addresses.iter()
            .map(hex::encode)
            .collect();

        // Scan through the slot range
        'slot_loop: for slot in req.start_slot..=req.end_slot {
            // Try event indices from 0 upward
            for event_index in 0..100000u64 {
                let event_key = format!("slot#{:020}#{:010}", slot, event_index);

                // Try to get this event via storage handle
                let event_data_opt = self.storage.get_block_event(event_key).await
                    .map_err(|e| Status::internal(format!("Failed to get block event: {}", e)))?;

                match event_data_opt {
                    Some(event_data) => {
                        // Parse event JSON
                        let event_json: serde_json::Value = match serde_json::from_slice(&event_data) {
                            Ok(json) => json,
                            Err(e) => {
                                tracing::warn!("Failed to parse event JSON: {}", e);
                                continue;
                            }
                        };

                        let event_type_str = event_json.get("event_type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("UNKNOWN");

                        // Build protobuf event based on type
                        let proto_event = match event_type_str {
                            "CREATED" => self.build_created_event(&event_json)?,
                            "SPENT" => self.build_spent_event(&event_json)?,
                            _ => continue,
                        };

                        // Apply address filter if specified
                        if !address_filters.is_empty() {
                            let event_address = if event_type_str == "CREATED" {
                                proto_event.utxo.as_ref()
                                    .and_then(|u| hex::decode(&u.address).ok())
                                    .map(|a| hex::encode(&a))
                            } else {
                                None
                            };

                            if let Some(addr) = event_address {
                                if !address_filters.contains(&addr) {
                                    continue;
                                }
                            } else {
                                continue; // Skip if we can't extract address
                            }
                        }

                        events.push(proto_event);

                        // Check max_events limit
                        if req.max_events > 0 && events.len() >= req.max_events as usize {
                            break 'slot_loop;
                        }
                    }
                    None => {
                        // No more events for this slot
                        break;
                    }
                }
            }
        }

        tracing::debug!("Returning {} events", events.len());

        Ok(Response::new(ReadUtxoEventsResponse {
            events,
        }))
    }

    async fn get_block_by_hash(
        &self,
        request: Request<GetBlockByHashRequest>,
    ) -> Result<Response<GetBlockByHashResponse>, Status> {
        let req = request.into_inner();

        tracing::debug!("GetBlockByHash for hash: {}", hex::encode(&req.hash));

        // Look up block metadata in the index
        let metadata = self.storage.get_block_metadata(req.hash.clone()).await
            .map_err(|e| Status::internal(format!("Failed to query block index: {}", e)))?;

        let (slot, timestamp, prev_hash) = match metadata {
            Some((s, t, p)) => {
                tracing::debug!("Found block: slot={}, prev_hash={}",
                    s, p.as_ref().map(|h| hex::encode(&h[..8])).unwrap_or_else(|| "None".to_string()));
                (s, t, p)
            }
            None => {
                return Err(Status::not_found(format!(
                    "Block not found in index: {}",
                    hex::encode(&req.hash)
                )));
            }
        };

        // Get the parent block hash
        let parent_hash = prev_hash.ok_or_else(|| {
            Status::not_found("Cannot fetch genesis/epoch boundary block")
        })?;

        tracing::debug!("Fetching block at slot {} via N2C chainsync (parent: {})",
            slot, hex::encode(&parent_hash[..8]));

        // Connect to node via N2C Unix socket
        let socket_path = self.socket_path.as_ref()
            .ok_or_else(|| Status::unavailable("Node socket path not configured"))?;

        use crate::chain_sync::HayateSync;
        use pallas_network::miniprotocols::{Point, chainsync::NextResponse};

        // Get parent slot from the parent block metadata
        let parent_metadata = self.storage.get_block_metadata(parent_hash.clone()).await
            .map_err(|e| Status::internal(format!("Failed to query parent block: {}", e)))?;

        let parent_slot = parent_metadata
            .ok_or_else(|| Status::not_found("Parent block not found in index"))?
            .0;

        tracing::debug!("Parent block: slot={}, hash={}", parent_slot, hex::encode(&parent_hash));

        // Create intersection point at parent block
        let parent_point = Point::Specific(parent_slot, parent_hash);

        // Connect via N2C and find intersection at parent
        let mut sync = HayateSync::connect(socket_path, self.magic, parent_point)
            .await
            .map_err(|e| Status::unavailable(format!("Failed to connect to node: {}", e)))?;

        // Wrap the rest in a closure to ensure cleanup happens in all paths
        let result = async {
            // Request next block with timeout
            // Note: When setting an old intersection, chainsync first sends RollBackward
            // to acknowledge the intersection, then RollForward with the block on the next call
            let response = tokio::time::timeout(
                std::time::Duration::from_secs(10),
                sync.request_next()
            )
                .await
                .map_err(|_| Status::deadline_exceeded("Timeout waiting for block from node"))?
                .map_err(|e| Status::unavailable(format!("Failed to request next block: {}", e)))?;

            let block_cbor = match response {
                NextResponse::RollForward(block, _tip) => block,
                NextResponse::RollBackward(_point, _tip) => {
                    // Got rollback acknowledgment, call request_next() again to get the block
                    tracing::debug!("Got RollBackward, calling request_next() again...");
                    let response2 = tokio::time::timeout(
                        std::time::Duration::from_secs(10),
                        sync.request_next()
                    )
                        .await
                        .map_err(|_| Status::deadline_exceeded("Timeout waiting for block from node"))?
                        .map_err(|e| Status::unavailable(format!("Failed to request next block: {}", e)))?;

                    match response2 {
                        NextResponse::RollForward(block, _tip) => block,
                        NextResponse::RollBackward(point, _tip) => {
                            return Err(Status::not_found(format!(
                                "Got rollback again to {:?} when fetching block", point
                            )));
                        }
                        NextResponse::Await => {
                            return Err(Status::unavailable("Node has no block data available"));
                        }
                    }
                }
                NextResponse::Await => {
                    return Err(Status::unavailable("Node has no block data available"));
                }
            };

            tracing::debug!("Fetched block via chainsync: {} bytes", block_cbor.len());

            // Validate that the returned block matches what we requested
            // This protects against forks that may have occurred during the fetch
            use pallas_traverse::MultiEraBlock;
            let returned_block = MultiEraBlock::decode(&block_cbor)
                .map_err(|e| Status::internal(format!("Failed to decode returned block: {}", e)))?;

            let returned_slot = returned_block.slot();
            let returned_hash = returned_block.hash();

            if returned_slot != slot {
                return Err(Status::not_found(format!(
                    "Block validation failed: requested slot {} but got slot {}. Block may have been rolled back due to a fork.",
                    slot, returned_slot
                )));
            }

            if returned_hash.as_slice() != req.hash {
                return Err(Status::not_found(format!(
                    "Block validation failed: requested hash {} but got hash {}. Block may have been rolled back due to a fork.",
                    hex::encode(&req.hash),
                    hex::encode(returned_hash)
                )));
            }

            tracing::debug!("Block validation passed: slot={}, hash={}", slot, hex::encode(&req.hash));

            Ok(Response::new(GetBlockByHashResponse {
                block_cbor,
                slot,
                hash: req.hash,
                timestamp,
            }))
        }.await;

        // Explicitly shutdown the connection to prevent file descriptor leaks
        // This must happen regardless of success or failure
        sync.shutdown().await;

        result
    }
}

impl QueryServiceImpl {
    #[allow(clippy::result_large_err)]
    fn build_created_event(&self, event_json: &serde_json::Value) -> Result<query::UtxoEvent, Status> {
        let utxo_data = event_json.get("utxo_data")
            .ok_or_else(|| Status::internal("Missing utxo_data in CREATED event"))?;

        // Extract UTxO fields
        let tx_hash = utxo_data.get("tx_hash")
            .and_then(|v| v.as_str())
            .and_then(|s| hex::decode(s).ok())
            .ok_or_else(|| Status::internal("Invalid tx_hash in UTxO data"))?;

        let output_index = utxo_data.get("output_index")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| Status::internal("Invalid output_index"))? as u32;

        let address = utxo_data.get("address")
            .and_then(|v| v.as_str())
            .and_then(|s| hex::decode(s).ok())
            .ok_or_else(|| Status::internal("Invalid address"))?;

        let amount = utxo_data.get("amount")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| Status::internal("Invalid amount"))?;

        let slot = utxo_data.get("slot")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let block_hash = utxo_data.get("block_hash")
            .and_then(|v| v.as_str())
            .and_then(|s| hex::decode(s).ok())
            .unwrap_or_default();

        let tx_index = utxo_data.get("tx_index")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        let block_timestamp = utxo_data.get("block_timestamp")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        // Build Utxo message
        let utxo = query::Utxo {
            tx_hash: tx_hash.clone(),
            output_index,
            address,
            amount,
            assets: vec![], // TODO: Parse assets from utxo_data
            datum_hash: vec![],
            datum: vec![],
            created_at_slot: slot,
            created_at_block_hash: block_hash.clone(),
            created_at_tx_index: tx_index,
            created_at_block_timestamp: block_timestamp,
        };

        Ok(query::UtxoEvent {
            event_type: query::utxo_event::EventType::Created as i32,
            tx_hash,
            output_index,
            slot,
            block_hash,
            tx_index,
            block_timestamp,
            utxo: Some(utxo),
            spent_by_tx_hash: vec![],
        })
    }

    #[allow(clippy::result_large_err)]
    fn build_spent_event(&self, event_json: &serde_json::Value) -> Result<query::UtxoEvent, Status> {
        let utxo_key = event_json.get("utxo_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Status::internal("Missing utxo_key in SPENT event"))?;

        let spend_data = event_json.get("spend_data")
            .ok_or_else(|| Status::internal("Missing spend_data in SPENT event"))?;

        // Extract UTxO data to build the UTxO object
        let utxo_data = event_json.get("utxo_data")
            .ok_or_else(|| Status::internal("Missing utxo_data in SPENT event"))?;

        // Parse utxo_key to get tx_hash and output_index
        let parts: Vec<&str> = utxo_key.split('#').collect();
        if parts.len() != 2 {
            return Err(Status::internal("Invalid utxo_key format"));
        }

        let tx_hash = hex::decode(parts[0])
            .map_err(|_| Status::internal("Invalid tx_hash in utxo_key"))?;
        let output_index = parts[1].parse::<u32>()
            .map_err(|_| Status::internal("Invalid output_index in utxo_key"))?;

        // Extract spend information
        let slot = spend_data.get("spent_at_slot")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let block_hash = spend_data.get("spent_at_block_hash")
            .and_then(|v| v.as_str())
            .and_then(|s| hex::decode(s).ok())
            .unwrap_or_default();

        let tx_index = spend_data.get("spent_at_tx_index")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        let block_timestamp = spend_data.get("spent_at_block_timestamp")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let spent_by_tx_hash = spend_data.get("spent_at_tx_hash")
            .and_then(|v| v.as_str())
            .and_then(|s| hex::decode(s).ok())
            .unwrap_or_default();

        // Build UTxO object from the original UTxO data
        let address = utxo_data.get("address")
            .and_then(|v| v.as_str())
            .and_then(|s| hex::decode(s).ok())
            .ok_or_else(|| Status::internal("Invalid address in utxo_data"))?;

        let amount = utxo_data.get("amount")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| Status::internal("Invalid amount in utxo_data"))?;

        let created_at_slot = utxo_data.get("slot")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let created_at_block_hash = utxo_data.get("block_hash")
            .and_then(|v| v.as_str())
            .and_then(|s| hex::decode(s).ok())
            .unwrap_or_default();

        let created_at_tx_index = utxo_data.get("tx_index")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        let created_at_block_timestamp = utxo_data.get("block_timestamp")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let utxo = query::Utxo {
            tx_hash: tx_hash.clone(),
            output_index,
            address,
            amount,
            assets: vec![], // TODO: Parse assets from utxo_data
            datum_hash: vec![],
            datum: vec![],
            created_at_slot,
            created_at_block_hash,
            created_at_tx_index,
            created_at_block_timestamp,
        };

        Ok(query::UtxoEvent {
            event_type: query::utxo_event::EventType::Spent as i32,
            tx_hash,
            output_index,
            slot,
            block_hash,
            tx_index,
            block_timestamp,
            utxo: Some(utxo),
            spent_by_tx_hash,
        })
    }
}
