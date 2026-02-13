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
    ReadUtxosRequest, ReadUtxosResponse, Utxo, Asset,
    SearchUtxosRequest, SearchUtxosResponse,
    ReadParamsRequest, ReadParamsResponse,
    GetChainTipRequest, GetChainTipResponse,
    GetTxHistoryRequest, GetTxHistoryResponse,
};

pub struct QueryServiceImpl {
    storage: Arc<RwLock<NetworkStorage>>,
}

impl QueryServiceImpl {
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

                    // Create Utxo proto message
                    utxos.push(Utxo {
                        tx_hash,
                        output_index,
                        address,
                        amount,
                        assets,
                        datum_hash,
                        datum,
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
}
