// UTxORPC Query Service implementation

use tonic::{Request, Response, Status};
use crate::indexer::{NetworkStorage, ChainTip};
use cardano_lsm::Key;
use std::sync::Arc;
use tokio::sync::RwLock;

// Include generated proto code
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
};

pub struct QueryServiceImpl {
    storage: Arc<RwLock<NetworkStorage>>,
}

impl QueryServiceImpl {
    #[allow(dead_code)]
    pub fn new(storage: NetworkStorage) -> Self {
        Self {
            storage: Arc::new(RwLock::new(storage))
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

        tracing::debug!("ReadUtxos request for {} addresses", req.addresses.len());

        let storage = self.storage.read().await;
        let mut utxos = Vec::new();

        // Convert addresses to hex for lookup
        let address_hexes: Vec<String> = req.addresses.iter()
            .map(|addr| hex::encode(addr))
            .collect();

        // Use address index to efficiently find UTxOs
        for addr_hex in &address_hexes {
            tracing::debug!("Looking up UTxOs for address: {}", addr_hex);

            // Get UTxO keys for this address from index
            let utxo_keys = storage.get_utxos_for_address(addr_hex)
                .map_err(|e| Status::internal(format!("Failed to query address index: {}", e)))?;

            tracing::debug!("Found {} UTxOs for address {}", utxo_keys.len(), addr_hex);

            // Retrieve full UTxO data for each key
            for utxo_key in utxo_keys {
                let key = Key::from(utxo_key.as_bytes());

                if let Some(utxo_data) = storage.utxo_tree.get(&key)
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

        tracing::debug!("Returning {} total UTxOs", utxos.len());

        // Get chain tip
        let tip = storage.get_chain_tip()
            .map_err(|e| Status::internal(format!("Failed to get chain tip: {}", e)))?
            .unwrap_or(ChainTip {
                height: 0,
                slot: 0,
                hash: vec![],
            });

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

        let _storage = self.storage.read().await;
        let utxos = Vec::new();

        // Pattern matching:
        // - "*" = all UTxOs
        // - hex prefix = addresses starting with prefix
        let pattern = req.pattern.to_lowercase();

        // We need to scan the address index to find matching addresses
        // This is not ideal for large databases, but works for wallet-scale data
        // TODO: Add more efficient pattern matching using LSM tree range queries

        // For now, we'll implement a simple linear scan
        // In a production system, we'd want a more efficient indexing strategy

        if pattern == "*" {
            // Return all UTxOs - this could be expensive!
            tracing::warn!("Returning all UTxOs - this may be slow for large datasets");

            // We need to iterate through all addresses in the index
            // Unfortunately, LSM trees don't have a simple iteration API
            // For now, return empty list with a message
            tracing::warn!("Full UTxO scan not yet implemented");

            return Ok(Response::new(SearchUtxosResponse {
                items: vec![],
            }));
        }

        // Check if pattern is a valid hex prefix
        if !pattern.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(Status::invalid_argument("Pattern must be hex or '*'"));
        }

        // Search for addresses matching the prefix
        // This is a stub - full implementation would require scanning the address index
        tracing::info!("Pattern-based search for addresses starting with: {}", pattern);

        Ok(Response::new(SearchUtxosResponse {
            items: utxos,
        }))
    }

    async fn read_params(
        &self,
        _request: Request<ReadParamsRequest>,
    ) -> Result<Response<ReadParamsResponse>, Status> {
        let storage = self.storage.read().await;
        let tip = storage.get_chain_tip()
            .map_err(|e| Status::internal(format!("Failed to get chain tip: {}", e)))?
            .unwrap_or(ChainTip {
                height: 0,
                slot: 0,
                hash: vec![],
            });

        Ok(Response::new(ReadParamsResponse {
            slot: tip.slot,
            hash: tip.hash,
        }))
    }

    async fn get_chain_tip(
        &self,
        _request: Request<GetChainTipRequest>,
    ) -> Result<Response<GetChainTipResponse>, Status> {
        let storage = self.storage.read().await;
        let tip = storage.get_chain_tip()
            .map_err(|e| Status::internal(format!("Failed to get chain tip: {}", e)))?
            .unwrap_or(ChainTip {
                height: 0,
                slot: 0,
                hash: vec![],
            });

        Ok(Response::new(GetChainTipResponse {
            height: tip.height,
            slot: tip.slot,
            hash: tip.hash,
        }))
    }

    async fn get_tx_history(
        &self,
        request: Request<GetTxHistoryRequest>,
    ) -> Result<Response<GetTxHistoryResponse>, Status> {
        let req = request.into_inner();

        let address_hex = hex::encode(&req.address);
        tracing::debug!("GetTxHistory for address: {}", address_hex);

        let storage = self.storage.read().await;

        // Get transaction hashes for this address
        let tx_hashes_hex = storage.get_tx_history_for_address(&address_hex)
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

        let storage = self.storage.read().await;
        let mut events = Vec::new();

        // Convert address filters to hex for comparison
        let address_filters: Vec<String> = req.addresses.iter()
            .map(|addr| hex::encode(addr))
            .collect();

        // Scan through the slot range
        'slot_loop: for slot in req.start_slot..=req.end_slot {
            // Try event indices from 0 upward
            for event_index in 0..100000u64 {
                let event_key = format!("slot#{:020}#{:010}", slot, event_index);
                let key = Key::from(event_key.as_bytes());

                // Try to get this event
                match storage.block_events_tree.get(&key) {
                    Ok(Some(event_data)) => {
                        // Parse event JSON
                        let event_json: serde_json::Value = match serde_json::from_slice(event_data.as_ref()) {
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
                    Ok(None) => {
                        // No more events for this slot
                        break;
                    }
                    Err(e) => {
                        tracing::error!("Error reading event: {}", e);
                        continue;
                    }
                }
            }
        }

        tracing::debug!("Returning {} events", events.len());

        Ok(Response::new(ReadUtxoEventsResponse {
            events,
        }))
    }
}

impl QueryServiceImpl {
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

    fn build_spent_event(&self, event_json: &serde_json::Value) -> Result<query::UtxoEvent, Status> {
        let utxo_key = event_json.get("utxo_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Status::internal("Missing utxo_key in SPENT event"))?;

        let spend_data = event_json.get("spend_data")
            .ok_or_else(|| Status::internal("Missing spend_data in SPENT event"))?;

        // Parse utxo_key to get tx_hash and output_index
        let parts: Vec<&str> = utxo_key.split('#').collect();
        if parts.len() != 2 {
            return Err(Status::internal("Invalid utxo_key format"));
        }

        let tx_hash = hex::decode(parts[0])
            .map_err(|_| Status::internal("Invalid tx_hash in utxo_key"))?;
        let output_index = parts[1].parse::<u32>()
            .map_err(|_| Status::internal("Invalid output_index in utxo_key"))?;

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

        Ok(query::UtxoEvent {
            event_type: query::utxo_event::EventType::Spent as i32,
            tx_hash,
            output_index,
            slot,
            block_hash,
            tx_index,
            block_timestamp,
            utxo: None,
            spent_by_tx_hash,
        })
    }
}
