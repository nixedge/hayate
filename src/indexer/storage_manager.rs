// Storage manager - dedicated task for LSM access
// Provides thread-safe access to NetworkStorage via message passing

use super::{NetworkStorage, ChainTip};
use anyhow::Result;
use cardano_lsm::{Key, Value};
use tokio::sync::{mpsc, oneshot};

/// Commands that can be sent to the storage manager
pub enum StorageCommand {
    /// Store chain tip
    StoreChainTip {
        slot: u64,
        hash: Vec<u8>,
        timestamp: u64,
        response: oneshot::Sender<Result<()>>,
    },

    /// Get chain tip
    GetChainTip {
        response: oneshot::Sender<Result<Option<ChainTip>>>,
    },

    /// Store wallet tip
    StoreWalletTip {
        wallet_id: String,
        slot: u64,
        hash: Vec<u8>,
        timestamp: u64,
        response: oneshot::Sender<Result<()>>,
    },

    /// Get wallet tip
    GetWalletTip {
        wallet_id: String,
        response: oneshot::Sender<Result<Option<ChainTip>>>,
    },

    /// Get minimum wallet tip across all wallets
    GetMinWalletTip {
        wallet_ids: Vec<String>,
        response: oneshot::Sender<Result<Option<ChainTip>>>,
    },

    /// Get UTxO by key
    GetUtxo {
        utxo_key: String,
        response: oneshot::Sender<Result<Option<Vec<u8>>>>,
    },

    /// Get UTxOs for address
    GetUtxosForAddress {
        address_hex: String,
        response: oneshot::Sender<Result<Vec<String>>>,
    },

    /// Insert UTxO
    InsertUtxo {
        utxo_key: String,
        utxo_data: Vec<u8>,
        response: oneshot::Sender<Result<()>>,
    },

    /// Delete UTxO
    DeleteUtxo {
        utxo_key: String,
        response: oneshot::Sender<Result<()>>,
    },

    /// Get balance for address
    GetBalance {
        address_hex: String,
        response: oneshot::Sender<Result<u64>>,
    },

    /// Update balance for address
    UpdateBalance {
        address_hex: String,
        new_balance: u64,
        response: oneshot::Sender<Result<()>>,
    },

    /// Add UTxO to address index
    AddUtxoToAddressIndex {
        address_hex: String,
        utxo_key: String,
        response: oneshot::Sender<Result<()>>,
    },

    /// Remove UTxO from address index
    RemoveUtxoFromAddressIndex {
        address_hex: String,
        utxo_key: String,
        response: oneshot::Sender<Result<()>>,
    },

    /// Add transaction to address history
    AddTxToAddressHistory {
        address_hex: String,
        tx_hash_hex: String,
        response: oneshot::Sender<Result<()>>,
    },

    /// Get transaction history for address
    GetTxHistoryForAddress {
        address_hex: String,
        response: oneshot::Sender<Result<Vec<String>>>,
    },

    /// Add transaction to policy index
    AddTxToPolicyIndex {
        policy_id_hex: String,
        tx_hash_hex: String,
        response: oneshot::Sender<Result<()>>,
    },

    /// Get transactions for policy
    GetTxsForPolicy {
        policy_id_hex: String,
        response: oneshot::Sender<Result<Vec<String>>>,
    },

    /// Add transaction to asset index
    AddTxToAssetIndex {
        policy_id_hex: String,
        asset_name_hex: String,
        tx_hash_hex: String,
        response: oneshot::Sender<Result<()>>,
    },

    /// Get transactions for asset
    GetTxsForAsset {
        policy_id_hex: String,
        asset_name_hex: String,
        response: oneshot::Sender<Result<Vec<String>>>,
    },

    /// Insert spent UTxO record
    InsertSpentUtxo {
        utxo_key: String,
        spend_event: Vec<u8>,
        response: oneshot::Sender<Result<()>>,
    },

    /// Delete spent UTxO record
    DeleteSpentUtxo {
        utxo_key: String,
        response: oneshot::Sender<Result<()>>,
    },

    /// Insert block event
    InsertBlockEvent {
        event_key: String,
        event_data: Vec<u8>,
        response: oneshot::Sender<Result<()>>,
    },

    /// Delete block event
    DeleteBlockEvent {
        event_key: String,
        response: oneshot::Sender<Result<()>>,
    },

    /// Get block event
    GetBlockEvent {
        event_key: String,
        response: oneshot::Sender<Result<Option<Vec<u8>>>>,
    },

    /// Store block metadata
    StoreBlockMetadata {
        block_hash: Vec<u8>,
        slot: u64,
        timestamp: u64,
        response: oneshot::Sender<Result<()>>,
    },

    /// Get block metadata
    GetBlockMetadata {
        block_hash: Vec<u8>,
        response: oneshot::Sender<Result<Option<(u64, u64)>>>,
    },

    /// Scan block events by prefix (for range queries)
    ScanBlockEventsByPrefix {
        prefix: String,
        response: oneshot::Sender<Result<Vec<(String, Vec<u8>)>>>,
    },

    /// Get the entire NetworkStorage (for BlockProcessor)
    /// This is a special command that transfers ownership temporarily
    TakeStorage {
        response: oneshot::Sender<Option<NetworkStorage>>,
    },

    /// Return NetworkStorage after block processing
    ReturnStorage {
        storage: NetworkStorage,
    },
}

/// Handle to communicate with the storage manager
#[derive(Clone)]
pub struct StorageHandle {
    sender: mpsc::UnboundedSender<StorageCommand>,
}

impl StorageHandle {
    /// Store chain tip
    pub async fn store_chain_tip(&self, slot: u64, hash: Vec<u8>, timestamp: u64) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender.send(StorageCommand::StoreChainTip {
            slot,
            hash,
            timestamp,
            response: tx,
        })?;
        rx.await?
    }

    /// Get chain tip
    pub async fn get_chain_tip(&self) -> Result<Option<ChainTip>> {
        let (tx, rx) = oneshot::channel();
        self.sender.send(StorageCommand::GetChainTip { response: tx })?;
        rx.await?
    }

    /// Store wallet tip
    pub async fn store_wallet_tip(&self, wallet_id: String, slot: u64, hash: Vec<u8>, timestamp: u64) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender.send(StorageCommand::StoreWalletTip {
            wallet_id,
            slot,
            hash,
            timestamp,
            response: tx,
        })?;
        rx.await?
    }

    /// Get wallet tip
    pub async fn get_wallet_tip(&self, wallet_id: String) -> Result<Option<ChainTip>> {
        let (tx, rx) = oneshot::channel();
        self.sender.send(StorageCommand::GetWalletTip { wallet_id, response: tx })?;
        rx.await?
    }

    /// Get minimum wallet tip
    pub async fn get_min_wallet_tip(&self, wallet_ids: Vec<String>) -> Result<Option<ChainTip>> {
        let (tx, rx) = oneshot::channel();
        self.sender.send(StorageCommand::GetMinWalletTip { wallet_ids, response: tx })?;
        rx.await?
    }

    /// Get UTxO by key
    pub async fn get_utxo(&self, utxo_key: String) -> Result<Option<Vec<u8>>> {
        let (tx, rx) = oneshot::channel();
        self.sender.send(StorageCommand::GetUtxo { utxo_key, response: tx })?;
        rx.await?
    }

    /// Get UTxOs for address
    pub async fn get_utxos_for_address(&self, address_hex: String) -> Result<Vec<String>> {
        let (tx, rx) = oneshot::channel();
        self.sender.send(StorageCommand::GetUtxosForAddress { address_hex, response: tx })?;
        rx.await?
    }

    /// Get transaction history for address
    pub async fn get_tx_history_for_address(&self, address_hex: String) -> Result<Vec<String>> {
        let (tx, rx) = oneshot::channel();
        self.sender.send(StorageCommand::GetTxHistoryForAddress { address_hex, response: tx })?;
        rx.await?
    }

    /// Get transactions for policy
    pub async fn get_txs_for_policy(&self, policy_id_hex: String) -> Result<Vec<String>> {
        let (tx, rx) = oneshot::channel();
        self.sender.send(StorageCommand::GetTxsForPolicy { policy_id_hex, response: tx })?;
        rx.await?
    }

    /// Get transactions for asset
    pub async fn get_txs_for_asset(&self, policy_id_hex: String, asset_name_hex: String) -> Result<Vec<String>> {
        let (tx, rx) = oneshot::channel();
        self.sender.send(StorageCommand::GetTxsForAsset { policy_id_hex, asset_name_hex, response: tx })?;
        rx.await?
    }

    /// Get block event
    pub async fn get_block_event(&self, event_key: String) -> Result<Option<Vec<u8>>> {
        let (tx, rx) = oneshot::channel();
        self.sender.send(StorageCommand::GetBlockEvent { event_key, response: tx })?;
        rx.await?
    }

    /// Get block metadata
    pub async fn get_block_metadata(&self, block_hash: Vec<u8>) -> Result<Option<(u64, u64)>> {
        let (tx, rx) = oneshot::channel();
        self.sender.send(StorageCommand::GetBlockMetadata { block_hash, response: tx })?;
        rx.await?
    }

    /// Scan block events by prefix
    pub async fn scan_block_events_by_prefix(&self, prefix: String) -> Result<Vec<(String, Vec<u8>)>> {
        let (tx, rx) = oneshot::channel();
        self.sender.send(StorageCommand::ScanBlockEventsByPrefix { prefix, response: tx })?;
        rx.await?
    }

    /// Take storage for block processing
    /// Returns None if storage is already taken
    pub async fn take_storage(&self) -> Option<NetworkStorage> {
        let (tx, rx) = oneshot::channel();
        self.sender.send(StorageCommand::TakeStorage { response: tx }).ok()?;
        rx.await.ok()?
    }

    /// Return storage after block processing
    pub async fn return_storage(&self, storage: NetworkStorage) -> Result<()> {
        self.sender.send(StorageCommand::ReturnStorage { storage })?;
        Ok(())
    }

    /// Insert UTxO
    pub async fn insert_utxo(&self, utxo_key: String, utxo_data: Vec<u8>) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender.send(StorageCommand::InsertUtxo { utxo_key, utxo_data, response: tx })?;
        rx.await?
    }

    /// Delete UTxO
    pub async fn delete_utxo(&self, utxo_key: String) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender.send(StorageCommand::DeleteUtxo { utxo_key, response: tx })?;
        rx.await?
    }

    /// Get balance
    pub async fn get_balance(&self, address_hex: String) -> Result<u64> {
        let (tx, rx) = oneshot::channel();
        self.sender.send(StorageCommand::GetBalance { address_hex, response: tx })?;
        rx.await?
    }

    /// Update balance
    pub async fn update_balance(&self, address_hex: String, new_balance: u64) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender.send(StorageCommand::UpdateBalance { address_hex, new_balance, response: tx })?;
        rx.await?
    }

    /// Add UTxO to address index
    pub async fn add_utxo_to_address_index(&self, address_hex: String, utxo_key: String) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender.send(StorageCommand::AddUtxoToAddressIndex { address_hex, utxo_key, response: tx })?;
        rx.await?
    }

    /// Remove UTxO from address index
    pub async fn remove_utxo_from_address_index(&self, address_hex: String, utxo_key: String) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender.send(StorageCommand::RemoveUtxoFromAddressIndex { address_hex, utxo_key, response: tx })?;
        rx.await?
    }

    /// Add transaction to address history
    pub async fn add_tx_to_address_history(&self, address_hex: String, tx_hash_hex: String) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender.send(StorageCommand::AddTxToAddressHistory { address_hex, tx_hash_hex, response: tx })?;
        rx.await?
    }

    /// Add transaction to policy index
    pub async fn add_tx_to_policy_index(&self, policy_id_hex: String, tx_hash_hex: String) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender.send(StorageCommand::AddTxToPolicyIndex { policy_id_hex, tx_hash_hex, response: tx })?;
        rx.await?
    }

    /// Add transaction to asset index
    pub async fn add_tx_to_asset_index(&self, policy_id_hex: String, asset_name_hex: String, tx_hash_hex: String) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender.send(StorageCommand::AddTxToAssetIndex { policy_id_hex, asset_name_hex, tx_hash_hex, response: tx })?;
        rx.await?
    }

    /// Insert spent UTxO record
    pub async fn insert_spent_utxo(&self, utxo_key: String, spend_event: Vec<u8>) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender.send(StorageCommand::InsertSpentUtxo { utxo_key, spend_event, response: tx })?;
        rx.await?
    }

    /// Delete spent UTxO record
    pub async fn delete_spent_utxo(&self, utxo_key: String) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender.send(StorageCommand::DeleteSpentUtxo { utxo_key, response: tx })?;
        rx.await?
    }

    /// Insert block event
    pub async fn insert_block_event(&self, event_key: String, event_data: Vec<u8>) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender.send(StorageCommand::InsertBlockEvent { event_key, event_data, response: tx })?;
        rx.await?
    }

    /// Delete block event
    pub async fn delete_block_event(&self, event_key: String) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender.send(StorageCommand::DeleteBlockEvent { event_key, response: tx })?;
        rx.await?
    }

    /// Store block metadata
    pub async fn store_block_metadata(&self, block_hash: Vec<u8>, slot: u64, timestamp: u64) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.sender.send(StorageCommand::StoreBlockMetadata { block_hash, slot, timestamp, response: tx })?;
        rx.await?
    }
}

/// Storage manager task
pub struct StorageManager {
    storage: Option<NetworkStorage>,
    receiver: mpsc::UnboundedReceiver<StorageCommand>,
}

impl StorageManager {
    /// Create a new storage manager and its handle
    pub fn new(storage: NetworkStorage) -> (Self, StorageHandle) {
        let (sender, receiver) = mpsc::unbounded_channel();

        let manager = Self {
            storage: Some(storage),
            receiver,
        };

        let handle = StorageHandle { sender };

        (manager, handle)
    }

    /// Run the storage manager event loop
    pub async fn run(mut self) {
        while let Some(command) = self.receiver.recv().await {
            self.handle_command(command);
        }

        tracing::info!("Storage manager shutting down");
    }

    fn handle_command(&mut self, command: StorageCommand) {
        match command {
            StorageCommand::StoreChainTip { slot, hash, timestamp, response } => {
                let result = if let Some(ref mut storage) = self.storage {
                    storage.store_chain_tip(slot, &hash, timestamp)
                } else {
                    Err(anyhow::anyhow!("Storage not available"))
                };
                let _ = response.send(result);
            }

            StorageCommand::GetChainTip { response } => {
                let result = if let Some(ref storage) = self.storage {
                    storage.get_chain_tip()
                } else {
                    Err(anyhow::anyhow!("Storage not available"))
                };
                let _ = response.send(result);
            }

            StorageCommand::StoreWalletTip { wallet_id, slot, hash, timestamp, response } => {
                let result = if let Some(ref mut storage) = self.storage {
                    storage.store_wallet_tip(&wallet_id, slot, &hash, timestamp)
                } else {
                    Err(anyhow::anyhow!("Storage not available"))
                };
                let _ = response.send(result);
            }

            StorageCommand::GetWalletTip { wallet_id, response } => {
                let result = if let Some(ref storage) = self.storage {
                    storage.get_wallet_tip(&wallet_id)
                } else {
                    Err(anyhow::anyhow!("Storage not available"))
                };
                let _ = response.send(result);
            }

            StorageCommand::GetMinWalletTip { wallet_ids, response } => {
                let result = if let Some(ref storage) = self.storage {
                    storage.get_min_wallet_tip(&wallet_ids)
                } else {
                    Err(anyhow::anyhow!("Storage not available"))
                };
                let _ = response.send(result);
            }

            StorageCommand::GetUtxo { utxo_key, response } => {
                let result = if let Some(ref storage) = self.storage {
                    let key = Key::from(utxo_key.as_bytes());
                    storage.utxo_tree.get(&key)
                        .map(|opt| opt.map(|v| v.as_ref().to_vec()))
                        .map_err(|e| e.into())
                } else {
                    Err(anyhow::anyhow!("Storage not available"))
                };
                let _ = response.send(result);
            }

            StorageCommand::GetUtxosForAddress { address_hex, response } => {
                let result = if let Some(ref storage) = self.storage {
                    storage.get_utxos_for_address(&address_hex)
                } else {
                    Err(anyhow::anyhow!("Storage not available"))
                };
                let _ = response.send(result);
            }

            StorageCommand::InsertUtxo { utxo_key, utxo_data, response } => {
                let result = if let Some(ref mut storage) = self.storage {
                    let key = Key::from(utxo_key.as_bytes());
                    let value = Value::from(utxo_data.as_slice());
                    storage.utxo_tree.insert(&key, &value).map_err(|e| e.into())
                } else {
                    Err(anyhow::anyhow!("Storage not available"))
                };
                let _ = response.send(result);
            }

            StorageCommand::DeleteUtxo { utxo_key, response } => {
                let result = if let Some(ref mut storage) = self.storage {
                    let key = Key::from(utxo_key.as_bytes());
                    storage.utxo_tree.delete(&key).map_err(|e| e.into())
                } else {
                    Err(anyhow::anyhow!("Storage not available"))
                };
                let _ = response.send(result);
            }

            StorageCommand::GetBalance { address_hex, response } => {
                let result = if let Some(ref storage) = self.storage {
                    let key = Key::from(address_hex.as_bytes());
                    storage.balance_tree.get(&key).map_err(|e| e.into())
                } else {
                    Err(anyhow::anyhow!("Storage not available"))
                };
                let _ = response.send(result);
            }

            StorageCommand::UpdateBalance { address_hex, new_balance, response } => {
                let result = if let Some(ref mut storage) = self.storage {
                    let key = Key::from(address_hex.as_bytes());
                    storage.balance_tree.insert(&key, &new_balance).map_err(|e| e.into())
                } else {
                    Err(anyhow::anyhow!("Storage not available"))
                };
                let _ = response.send(result);
            }

            StorageCommand::AddUtxoToAddressIndex { address_hex, utxo_key, response } => {
                let result = if let Some(ref mut storage) = self.storage {
                    storage.add_utxo_to_address_index(&address_hex, &utxo_key)
                } else {
                    Err(anyhow::anyhow!("Storage not available"))
                };
                let _ = response.send(result);
            }

            StorageCommand::RemoveUtxoFromAddressIndex { address_hex, utxo_key, response } => {
                let result = if let Some(ref mut storage) = self.storage {
                    storage.remove_utxo_from_address_index(&address_hex, &utxo_key)
                } else {
                    Err(anyhow::anyhow!("Storage not available"))
                };
                let _ = response.send(result);
            }

            StorageCommand::AddTxToAddressHistory { address_hex, tx_hash_hex, response } => {
                let result = if let Some(ref mut storage) = self.storage {
                    storage.add_tx_to_address_history(&address_hex, &tx_hash_hex)
                } else {
                    Err(anyhow::anyhow!("Storage not available"))
                };
                let _ = response.send(result);
            }

            StorageCommand::GetTxHistoryForAddress { address_hex, response } => {
                let result = if let Some(ref storage) = self.storage {
                    storage.get_tx_history_for_address(&address_hex)
                } else {
                    Err(anyhow::anyhow!("Storage not available"))
                };
                let _ = response.send(result);
            }

            StorageCommand::AddTxToPolicyIndex { policy_id_hex, tx_hash_hex, response } => {
                let result = if let Some(ref mut storage) = self.storage {
                    storage.add_tx_to_policy_index(&policy_id_hex, &tx_hash_hex)
                } else {
                    Err(anyhow::anyhow!("Storage not available"))
                };
                let _ = response.send(result);
            }

            StorageCommand::GetTxsForPolicy { policy_id_hex, response } => {
                let result = if let Some(ref storage) = self.storage {
                    storage.get_txs_for_policy(&policy_id_hex)
                } else {
                    Err(anyhow::anyhow!("Storage not available"))
                };
                let _ = response.send(result);
            }

            StorageCommand::AddTxToAssetIndex { policy_id_hex, asset_name_hex, tx_hash_hex, response } => {
                let result = if let Some(ref mut storage) = self.storage {
                    storage.add_tx_to_asset_index(&policy_id_hex, &asset_name_hex, &tx_hash_hex)
                } else {
                    Err(anyhow::anyhow!("Storage not available"))
                };
                let _ = response.send(result);
            }

            StorageCommand::GetTxsForAsset { policy_id_hex, asset_name_hex, response } => {
                let result = if let Some(ref storage) = self.storage {
                    storage.get_txs_for_asset(&policy_id_hex, &asset_name_hex)
                } else {
                    Err(anyhow::anyhow!("Storage not available"))
                };
                let _ = response.send(result);
            }

            StorageCommand::InsertSpentUtxo { utxo_key, spend_event, response } => {
                let result = if let Some(ref mut storage) = self.storage {
                    let key = Key::from(utxo_key.as_bytes());
                    let value = Value::from(spend_event.as_slice());
                    storage.spent_utxo_index.insert(&key, &value).map_err(|e| e.into())
                } else {
                    Err(anyhow::anyhow!("Storage not available"))
                };
                let _ = response.send(result);
            }

            StorageCommand::DeleteSpentUtxo { utxo_key, response } => {
                let result = if let Some(ref mut storage) = self.storage {
                    let key = Key::from(utxo_key.as_bytes());
                    storage.spent_utxo_index.delete(&key).map_err(|e| e.into())
                } else {
                    Err(anyhow::anyhow!("Storage not available"))
                };
                let _ = response.send(result);
            }

            StorageCommand::InsertBlockEvent { event_key, event_data, response } => {
                let result = if let Some(ref mut storage) = self.storage {
                    let key = Key::from(event_key.as_bytes());
                    let value = Value::from(event_data.as_slice());
                    storage.block_events_tree.insert(&key, &value).map_err(|e| e.into())
                } else {
                    Err(anyhow::anyhow!("Storage not available"))
                };
                let _ = response.send(result);
            }

            StorageCommand::DeleteBlockEvent { event_key, response } => {
                let result = if let Some(ref mut storage) = self.storage {
                    let key = Key::from(event_key.as_bytes());
                    storage.block_events_tree.delete(&key).map_err(|e| e.into())
                } else {
                    Err(anyhow::anyhow!("Storage not available"))
                };
                let _ = response.send(result);
            }

            StorageCommand::GetBlockEvent { event_key, response } => {
                let result = if let Some(ref storage) = self.storage {
                    let key = Key::from(event_key.as_bytes());
                    storage.block_events_tree.get(&key)
                        .map(|opt| opt.map(|v| v.as_ref().to_vec()))
                        .map_err(|e| e.into())
                } else {
                    Err(anyhow::anyhow!("Storage not available"))
                };
                let _ = response.send(result);
            }

            StorageCommand::StoreBlockMetadata { block_hash, slot, timestamp, response } => {
                let result = if let Some(ref mut storage) = self.storage {
                    storage.store_block_metadata(&block_hash, slot, timestamp)
                } else {
                    Err(anyhow::anyhow!("Storage not available"))
                };
                let _ = response.send(result);
            }

            StorageCommand::GetBlockMetadata { block_hash, response } => {
                let result = if let Some(ref storage) = self.storage {
                    storage.get_block_metadata(&block_hash)
                } else {
                    Err(anyhow::anyhow!("Storage not available"))
                };
                let _ = response.send(result);
            }

            StorageCommand::ScanBlockEventsByPrefix { prefix, response } => {
                let result = if let Some(ref storage) = self.storage {
                    let mut events = Vec::new();
                    for (key, value) in storage.block_events_tree.scan_prefix(prefix.as_bytes()) {
                        if let Ok(key_str) = std::str::from_utf8(key.as_ref()) {
                            events.push((key_str.to_string(), value.as_ref().to_vec()));
                        }
                    }
                    Ok(events)
                } else {
                    Err(anyhow::anyhow!("Storage not available"))
                };
                let _ = response.send(result);
            }

            StorageCommand::TakeStorage { response } => {
                let storage = self.storage.take();
                let _ = response.send(storage);
            }

            StorageCommand::ReturnStorage { storage } => {
                self.storage = Some(storage);
            }
        }
    }
}
