// Core Hayate indexer implementation

#![allow(dead_code)]

pub mod block_processor;
pub mod storage_manager;

use cardano_lsm::{LsmTree, LsmConfig, MonoidalLsmTree, IncrementalMerkleTree, Key, Value};
use std::path::PathBuf;
use std::collections::HashMap;
use tokio::sync::{RwLock, broadcast};
use anyhow::Result;

#[allow(unused_imports)]
pub use block_processor::{BlockProcessor, WalletFilter, BlockStats};
pub use storage_manager::{StorageHandle, StorageManager};

// Import RewardsTracker from the rewards module
use crate::rewards::RewardsTracker;

/// Block update broadcast to subscribers
#[derive(Debug, Clone)]
pub struct BlockUpdate {
    pub network: Network,
    pub height: u64,
    pub slot: u64,
    pub hash: Vec<u8>,
    pub tx_hashes: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Network {
    Mainnet,
    Preprod,
    Preview,
    SanchoNet,
    Custom(String),
}

impl Network {
    pub fn magic(&self) -> u64 {
        match self {
            Network::Mainnet => 764824073,
            Network::Preprod => 1,
            Network::Preview => 2,
            Network::SanchoNet => 4,
            Network::Custom(_) => 0,
        }
    }
    
    pub fn as_str(&self) -> &str {
        match self {
            Network::Mainnet => "mainnet",
            Network::Preprod => "preprod",
            Network::Preview => "preview",
            Network::SanchoNet => "sanchonet",
            Network::Custom(name) => name,
        }
    }
    
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "mainnet" => Some(Network::Mainnet),
            "preprod" => Some(Network::Preprod),
            "preview" => Some(Network::Preview),
            "sanchonet" => Some(Network::SanchoNet),
            other => Some(Network::Custom(other.to_string())),
        }
    }
}

/// Storage for a single network
pub struct NetworkStorage {
    pub network: Network,
    pub utxo_tree: LsmTree,
    pub balance_tree: MonoidalLsmTree<u64>,
    pub governance_tree: LsmTree,
    pub governance_merkle: IncrementalMerkleTree,

    // Staking and rewards (cardano-wallet pattern)
    pub rewards_tracker: RewardsTracker,

    // Epoch nonce for leader schedule
    pub nonce_tree: LsmTree,

    // Chain tip tracking
    pub chain_tip_tree: LsmTree,

    // Address -> UTxO keys index for efficient queries
    // Key: address_hex, Value: JSON array of UTxO keys
    pub address_utxo_index: LsmTree,

    // Transaction history: address -> tx hashes
    // Key: address_hex, Value: JSON array of tx hashes
    pub address_tx_index: LsmTree,

    // Token transaction indexes
    // Policy -> tx hashes: policy_id_hex -> JSON array of tx hashes
    pub policy_tx_index: LsmTree,

    // Asset -> tx hashes: "policy_id_hex:asset_name_hex" -> JSON array of tx hashes
    pub asset_tx_index: LsmTree,

    // Spent UTxO tracking: utxo_key -> spend event JSON
    // Key: "tx_hash#index", Value: { spent_at_slot, spent_at_block_hash, spent_at_tx_index, spent_at_tx_hash, spent_at_block_timestamp, spent_by_address }
    pub spent_utxo_index: LsmTree,

    // Block events for range queries (CREATE and SPEND events)
    // Key: "slot#{slot:020}#{event_index:010}", Value: JSON event data
    // This enables efficient slot range queries for ReadUtxoEvents RPC
    pub block_events_tree: LsmTree,

    // Block hash index for block-by-hash queries
    // Key: block_hash (32 bytes), Value: JSON { slot, timestamp }
    pub block_hash_index: LsmTree,

    // Track which epoch we started indexing from
    pub indexing_start_epoch: u64,
}

impl NetworkStorage {
    pub fn open(base_path: PathBuf, network: Network) -> Result<Self> {
        let network_path = base_path.join(network.as_str());

        tracing::info!("Opening storage for {} at {:?}", network.as_str(), network_path);

        let utxo_tree = LsmTree::open(network_path.join("utxos"), LsmConfig::default())?;
        let balance_tree = MonoidalLsmTree::open(network_path.join("balances"), LsmConfig::default())?;
        let governance_tree = LsmTree::open(network_path.join("governance"), LsmConfig::default())?;
        let governance_merkle = IncrementalMerkleTree::new(32);
        let nonce_tree = LsmTree::open(network_path.join("nonces"), LsmConfig::default())?;
        let chain_tip_tree = LsmTree::open(network_path.join("chain_tip"), LsmConfig::default())?;
        let address_utxo_index = LsmTree::open(network_path.join("address_utxos"), LsmConfig::default())?;
        let address_tx_index = LsmTree::open(network_path.join("address_txs"), LsmConfig::default())?;
        let policy_tx_index = LsmTree::open(network_path.join("policy_txs"), LsmConfig::default())?;
        let asset_tx_index = LsmTree::open(network_path.join("asset_txs"), LsmConfig::default())?;
        let spent_utxo_index = LsmTree::open(network_path.join("spent_utxos"), LsmConfig::default())?;
        let block_events_tree = LsmTree::open(network_path.join("block_events"), LsmConfig::default())?;
        let block_hash_index = LsmTree::open(network_path.join("block_hash_index"), LsmConfig::default())?;

        let start_epoch = 0;
        let rewards_tracker = RewardsTracker::open(network_path.join("rewards"), start_epoch)?;

        Ok(Self {
            network,
            utxo_tree,
            balance_tree,
            governance_tree,
            governance_merkle,
            rewards_tracker,
            nonce_tree,
            chain_tip_tree,
            address_utxo_index,
            address_tx_index,
            policy_tx_index,
            asset_tx_index,
            spent_utxo_index,
            block_events_tree,
            block_hash_index,
            indexing_start_epoch: start_epoch,
        })
    }
    
    pub fn store_nonce(&mut self, epoch: u64, nonce: &[u8]) -> Result<()> {
        let key = format!("nonce:{}", epoch);
        self.nonce_tree.insert(
            &Key::from(key.as_bytes()),
            &Value::from(nonce)
        )?;
        
        tracing::debug!("Stored nonce for epoch {}", epoch);
        Ok(())
    }
    
    pub fn get_nonce(&self, epoch: u64) -> Result<Option<Vec<u8>>> {
        let key = format!("nonce:{}", epoch);

        if let Some(value) = self.nonce_tree.get(&Key::from(key.as_bytes()))? {
            Ok(Some(value.as_ref().to_vec()))
        } else {
            Ok(None)
        }
    }

    /// Store chain tip
    pub fn store_chain_tip(&mut self, slot: u64, hash: &[u8], timestamp: u64) -> Result<()> {
        let tip_data = serde_json::json!({
            "slot": slot,
            "hash": hex::encode(hash),
            "timestamp": timestamp,
        });

        self.chain_tip_tree.insert(
            &Key::from(b"current_tip"),
            &Value::from(&serde_json::to_vec(&tip_data)?),
        )?;

        Ok(())
    }

    /// Get current chain tip
    pub fn get_chain_tip(&self) -> Result<Option<ChainTip>> {
        if let Some(value) = self.chain_tip_tree.get(&Key::from(b"current_tip"))? {
            let tip_data: serde_json::Value = serde_json::from_slice(value.as_ref())?;

            Ok(Some(ChainTip {
                height: 0, // TODO: track height
                slot: tip_data["slot"].as_u64().unwrap_or(0),
                hash: hex::decode(tip_data["hash"].as_str().unwrap_or("")).unwrap_or_default(),
                timestamp: tip_data["timestamp"].as_u64().unwrap_or(0),
            }))
        } else {
            Ok(None)
        }
    }

    /// Store wallet-specific chain tip
    /// Uses wallet identifier (e.g., xpub hash or stake key) as key
    pub fn store_wallet_tip(&mut self, wallet_id: &str, slot: u64, hash: &[u8], timestamp: u64) -> Result<()> {
        let tip_data = serde_json::json!({
            "slot": slot,
            "hash": hex::encode(hash),
            "timestamp": timestamp,
        });

        let key = format!("wallet_tip:{}", wallet_id);
        self.chain_tip_tree.insert(
            &Key::from(key.as_bytes()),
            &Value::from(&serde_json::to_vec(&tip_data)?),
        )?;

        Ok(())
    }

    /// Get wallet-specific chain tip
    pub fn get_wallet_tip(&self, wallet_id: &str) -> Result<Option<ChainTip>> {
        let key = format!("wallet_tip:{}", wallet_id);

        if let Some(value) = self.chain_tip_tree.get(&Key::from(key.as_bytes()))? {
            let tip_data: serde_json::Value = serde_json::from_slice(value.as_ref())?;

            Ok(Some(ChainTip {
                height: 0,
                slot: tip_data["slot"].as_u64().unwrap_or(0),
                hash: hex::decode(tip_data["hash"].as_str().unwrap_or("")).unwrap_or_default(),
                timestamp: tip_data["timestamp"].as_u64().unwrap_or(0),
            }))
        } else {
            Ok(None)
        }
    }

    /// Get the minimum wallet tip across all wallets
    /// This determines where to resume chain sync from
    pub fn get_min_wallet_tip(&self, wallet_ids: &[String]) -> Result<Option<ChainTip>> {
        if wallet_ids.is_empty() {
            // No wallets configured, use global chain tip
            return self.get_chain_tip();
        }

        let mut min_tip: Option<ChainTip> = None;

        for wallet_id in wallet_ids {
            match self.get_wallet_tip(wallet_id)? {
                Some(tip) => {
                    match &min_tip {
                        Some(current_min) => {
                            if tip.slot < current_min.slot {
                                min_tip = Some(tip);
                            }
                        }
                        None => min_tip = Some(tip),
                    }
                }
                None => {
                    // If any wallet has no tip, start from origin
                    return Ok(None);
                }
            }
        }

        Ok(min_tip)
    }

    /// Store block metadata in the block hash index
    pub fn store_block_metadata(&mut self, block_hash: &[u8], slot: u64, timestamp: u64) -> Result<()> {
        let metadata = serde_json::json!({
            "slot": slot,
            "timestamp": timestamp,
        });

        self.block_hash_index.insert(
            &Key::from(block_hash),
            &Value::from(&serde_json::to_vec(&metadata)?),
        )?;

        Ok(())
    }

    /// Get block metadata by hash
    pub fn get_block_metadata(&self, block_hash: &[u8]) -> Result<Option<(u64, u64)>> {
        if let Some(value) = self.block_hash_index.get(&Key::from(block_hash))? {
            let metadata: serde_json::Value = serde_json::from_slice(value.as_ref())?;
            let slot = metadata["slot"].as_u64().unwrap_or(0);
            let timestamp = metadata["timestamp"].as_u64().unwrap_or(0);
            Ok(Some((slot, timestamp)))
        } else {
            Ok(None)
        }
    }

    /// Add a UTxO to the address index
    /// Uses individual keys for O(1) operations instead of JSON arrays
    pub fn add_utxo_to_address_index(&mut self, address_hex: &str, utxo_key: &str) -> Result<()> {
        // Key format: address:utxo_key
        let index_key = format!("{}:{}", address_hex, utxo_key);
        let key = Key::from(index_key.as_bytes());

        // Store a marker (just 1 byte) - the key itself contains the information
        self.address_utxo_index.insert(&key, &Value::from(&[1u8]))?;

        Ok(())
    }

    /// Remove a UTxO from the address index
    /// Uses individual keys for O(1) operations
    pub fn remove_utxo_from_address_index(&mut self, address_hex: &str, utxo_key: &str) -> Result<()> {
        // Key format: address:utxo_key
        let index_key = format!("{}:{}", address_hex, utxo_key);
        let key = Key::from(index_key.as_bytes());

        // Simply delete the key - O(1) operation
        self.address_utxo_index.delete(&key)?;

        Ok(())
    }

    /// Get all UTxO keys for an address
    /// Uses prefix scan for efficient retrieval
    pub fn get_utxos_for_address(&self, address_hex: &str) -> Result<Vec<String>> {
        let prefix = format!("{}:", address_hex);
        let mut utxo_keys = Vec::new();

        // Scan all keys with this address prefix
        for (key, _value) in self.address_utxo_index.scan_prefix(prefix.as_bytes()) {
            // Key format is "address:utxo_key", extract the utxo_key part
            if let Ok(key_str) = std::str::from_utf8(key.as_ref()) {
                if let Some(utxo_key) = key_str.strip_prefix(&prefix) {
                    utxo_keys.push(utxo_key.to_string());
                }
            }
        }

        Ok(utxo_keys)
    }

    /// Add a transaction to the address history
    /// Uses individual keys for O(1) operations
    pub fn add_tx_to_address_history(&mut self, address_hex: &str, tx_hash_hex: &str) -> Result<()> {
        // Key format: address:tx_hash
        let index_key = format!("{}:{}", address_hex, tx_hash_hex);
        let key = Key::from(index_key.as_bytes());

        // Store a marker (just 1 byte)
        self.address_tx_index.insert(&key, &Value::from(&[1u8]))?;

        Ok(())
    }

    /// Get transaction history for an address
    /// Uses prefix scan for efficient retrieval
    pub fn get_tx_history_for_address(&self, address_hex: &str) -> Result<Vec<String>> {
        let prefix = format!("{}:", address_hex);
        let mut tx_hashes = Vec::new();

        // Scan all keys with this address prefix
        for (key, _value) in self.address_tx_index.scan_prefix(prefix.as_bytes()) {
            // Key format is "address:tx_hash", extract the tx_hash part
            if let Ok(key_str) = std::str::from_utf8(key.as_ref()) {
                if let Some(tx_hash) = key_str.strip_prefix(&prefix) {
                    tx_hashes.push(tx_hash.to_string());
                }
            }
        }

        Ok(tx_hashes)
    }

    /// Add a transaction to the policy index
    /// Tracks all transactions that include tokens from this policy
    pub fn add_tx_to_policy_index(&mut self, policy_id_hex: &str, tx_hash_hex: &str) -> Result<()> {
        // Key format: policy_id:tx_hash
        let index_key = format!("{}:{}", policy_id_hex, tx_hash_hex);
        let key = Key::from(index_key.as_bytes());

        // Store a marker (just 1 byte)
        self.policy_tx_index.insert(&key, &Value::from(&[1u8]))?;

        Ok(())
    }

    /// Get all transactions for a policy ID
    pub fn get_txs_for_policy(&self, policy_id_hex: &str) -> Result<Vec<String>> {
        let prefix = format!("{}:", policy_id_hex);
        let mut tx_hashes = Vec::new();

        // Scan all keys with this policy prefix
        for (key, _value) in self.policy_tx_index.scan_prefix(prefix.as_bytes()) {
            if let Ok(key_str) = std::str::from_utf8(key.as_ref()) {
                if let Some(tx_hash) = key_str.strip_prefix(&prefix) {
                    tx_hashes.push(tx_hash.to_string());
                }
            }
        }

        Ok(tx_hashes)
    }

    /// Add a transaction to the asset index
    /// Tracks all transactions that include a specific asset (policy + name)
    pub fn add_tx_to_asset_index(&mut self, policy_id_hex: &str, asset_name_hex: &str, tx_hash_hex: &str) -> Result<()> {
        // Key format: policy_id:asset_name:tx_hash
        let index_key = format!("{}:{}:{}", policy_id_hex, asset_name_hex, tx_hash_hex);
        let key = Key::from(index_key.as_bytes());

        // Store a marker (just 1 byte)
        self.asset_tx_index.insert(&key, &Value::from(&[1u8]))?;

        Ok(())
    }

    /// Get all transactions for a specific asset (policy + name)
    pub fn get_txs_for_asset(&self, policy_id_hex: &str, asset_name_hex: &str) -> Result<Vec<String>> {
        let prefix = format!("{}:{}:", policy_id_hex, asset_name_hex);
        let mut tx_hashes = Vec::new();

        // Scan all keys with this asset prefix
        for (key, _value) in self.asset_tx_index.scan_prefix(prefix.as_bytes()) {
            if let Ok(key_str) = std::str::from_utf8(key.as_ref()) {
                if let Some(tx_hash) = key_str.strip_prefix(&prefix) {
                    tx_hashes.push(tx_hash.to_string());
                }
            }
        }

        Ok(tx_hashes)
    }
}

#[derive(Debug, Clone)]
pub struct ChainTip {
    pub height: u64,
    pub slot: u64,
    pub hash: Vec<u8>,
    pub timestamp: u64,  // Unix milliseconds
}

pub struct HayateIndexer {
    pub networks: RwLock<HashMap<Network, StorageHandle>>,
    pub account_xpubs: RwLock<Vec<String>>,
    pub gap_limit: u32,
    block_updates: broadcast::Sender<BlockUpdate>,
}

impl HayateIndexer {
    pub fn new(_base_path: PathBuf, gap_limit: u32) -> Result<Self> {
        // Create broadcast channel with capacity for 1000 block updates
        let (block_updates, _) = broadcast::channel(1000);

        Ok(Self {
            networks: RwLock::new(HashMap::new()),
            account_xpubs: RwLock::new(Vec::new()),
            gap_limit,
            block_updates,
        })
    }

    /// Subscribe to block updates
    pub fn subscribe_blocks(&self) -> broadcast::Receiver<BlockUpdate> {
        self.block_updates.subscribe()
    }

    /// Broadcast a new block update to all subscribers
    pub fn broadcast_block(&self, update: BlockUpdate) {
        // Ignore send errors (no subscribers is okay)
        let _ = self.block_updates.send(update);
    }
    
    pub async fn add_network(&self, network: Network, base_path: PathBuf) -> Result<()> {
        let storage = NetworkStorage::open(base_path, network.clone())?;

        // Create storage manager and spawn its task
        let (manager, handle) = StorageManager::new(storage);
        tokio::spawn(async move {
            manager.run().await;
        });

        self.networks.write().await.insert(network, handle);
        Ok(())
    }
    
    pub async fn add_account(&self, xpub: String) -> Result<()> {
        self.account_xpubs.write().await.push(xpub);
        Ok(())
    }
    
    pub async fn get_chain_tip(&self) -> Result<ChainTip> {
        Ok(ChainTip {
            height: 0,
            slot: 0,
            hash: vec![],
            timestamp: 0,
        })
    }
}
