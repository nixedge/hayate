// Block processor - processes blocks from chain sync and updates storage

use super::StorageHandle;
use crate::config::TokenConfig;
use crate::snapshot_manager;
use anyhow::{Context, Result};
use pallas_crypto::hash::Hash;
use pallas_traverse::{MultiEraBlock, MultiEraTx, MultiEraOutput};
use std::collections::{HashSet, VecDeque};

pub struct WalletFilter {
    tracked_payment_keys: HashSet<Hash<28>>,
    tracked_stake_keys: HashSet<Hash<28>>,
}

impl Default for WalletFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl WalletFilter {
    pub fn new() -> Self {
        Self {
            tracked_payment_keys: HashSet::new(),
            tracked_stake_keys: HashSet::new(),
        }
    }

    pub fn add_payment_key_hash(&mut self, key_hash: Hash<28>) {
        self.tracked_payment_keys.insert(key_hash);
    }

    pub fn add_stake_credential(&mut self, stake_cred: Hash<28>) {
        self.tracked_stake_keys.insert(stake_cred);
    }

    pub fn is_our_payment_key(&self, key_hash: &Hash<28>) -> bool {
        self.tracked_payment_keys.contains(key_hash)
    }

    pub fn is_our_stake_key(&self, stake_cred: &Hash<28>) -> bool {
        self.tracked_stake_keys.contains(stake_cred)
    }

    /// Check if an address involves any of our tracked keys
    pub fn is_our_address(&self, address_bytes: &[u8]) -> bool {
        // For now, if we have no filters, track everything
        if self.tracked_payment_keys.is_empty() && self.tracked_stake_keys.is_empty() {
            return true;
        }

        // Parse Shelley address
        use pallas_addresses::{Address, ShelleyDelegationPart};

        let address = match Address::from_bytes(address_bytes) {
            Ok(Address::Shelley(addr)) => addr,
            _ => return false, // Invalid or non-Shelley address
        };

        // Extract payment key hash from address
        let payment_hash = match address.payment() {
            pallas_addresses::ShelleyPaymentPart::Key(hash) => hash,
            pallas_addresses::ShelleyPaymentPart::Script(_) => return false, // Skip script addresses
        };

        // Check if this payment key is tracked
        if !self.is_our_payment_key(payment_hash) {
            return false;
        }

        // For enterprise addresses (no stake key), we accept if payment key matches
        // For full addresses (with stake key), we also check the stake key
        match address.delegation() {
            ShelleyDelegationPart::Null => {
                // Enterprise address (payment only) - matches if payment key is ours
                true
            }
            ShelleyDelegationPart::Key(stake_hash) => {
                // Full address - check if stake key is also ours
                // Accept if stake key matches OR if we're not filtering by stake keys
                self.tracked_stake_keys.is_empty() || self.is_our_stake_key(stake_hash)
            }
            ShelleyDelegationPart::Script(_) => {
                // Script stake delegation - accept if payment key is ours
                true
            }
            ShelleyDelegationPart::Pointer(_) => {
                // Pointer address - accept if payment key is ours
                true
            }
        }
    }
}

/// Rollback information for a processed block
#[derive(Debug, Clone)]
struct RollbackInfo {
    slot: u64,
    block_hash: Vec<u8>,
    timestamp: u64,
    utxos_created: Vec<String>,  // Keys that were created
    utxos_spent: Vec<(String, Vec<u8>, Vec<u8>)>,  // Keys that were spent, with their data and spend metadata
    spent_utxos_recorded: Vec<String>,  // Keys for which we recorded spend events
}

pub struct BlockProcessor {
    pub storage: StorageHandle,
    pub filter: WalletFilter,
    pub current_epoch: u64,
    pub blocks_processed: u64,
    pub current_slot: u64,

    // Wallet IDs for per-wallet tip tracking
    pub wallet_ids: Vec<String>,

    // Native tokens to track (independent of wallet addresses)
    tracked_tokens: Vec<TokenConfig>,

    // Rollback buffer - keep last K blocks for potential rollback
    rollback_buffer: VecDeque<RollbackInfo>,
    rollback_buffer_size: usize,

    // Network system start time (genesis time) in Unix milliseconds
    system_start_ms: u64,

    // Snapshot management
    snapshot_manager: snapshot_manager::SnapshotManager,
    chain_tip_slot: u64,  // Estimated chain tip for near-tip detection
}

impl BlockProcessor {
    pub async fn new(storage: StorageHandle, system_start_ms: u64) -> Result<Self> {
        Self::new_with_rollback_buffer(storage, 100, system_start_ms).await
    }

    pub async fn new_with_rollback_buffer(storage: StorageHandle, buffer_size: usize, system_start_ms: u64) -> Result<Self> {
        // Try to restore chain tip
        let current_slot = storage.get_chain_tip().await?
            .map(|tip| tip.slot)
            .unwrap_or(0);

        Ok(Self {
            storage,
            filter: WalletFilter::new(),
            current_epoch: 0,
            blocks_processed: 0,
            current_slot,
            wallet_ids: Vec::new(),
            tracked_tokens: Vec::new(),
            rollback_buffer: VecDeque::with_capacity(buffer_size),
            rollback_buffer_size: buffer_size,
            system_start_ms,
            snapshot_manager: snapshot_manager::SnapshotManager::default(),
            chain_tip_slot: current_slot, // Start with current slot as estimate
        })
    }

    /// Add a wallet ID for per-wallet tip tracking
    pub fn add_wallet_id(&mut self, wallet_id: String) {
        self.wallet_ids.push(wallet_id);
    }

    pub fn add_wallet(&mut self, payment_key: Hash<28>, stake_key: Hash<28>) {
        self.filter.add_payment_key_hash(payment_key);
        self.filter.add_stake_credential(stake_key);
    }

    /// Add a native token to track
    pub fn add_tracked_token(&mut self, token: TokenConfig) {
        self.tracked_tokens.push(token);
    }

    /// Process a block from CBOR bytes
    pub async fn process_block(&mut self, block_bytes: &[u8], slot: u64, block_hash: &[u8], chain_tip_slot: Option<u64>) -> Result<BlockStats> {
        // Check if this is a forward move
        if slot < self.current_slot {
            anyhow::bail!("Slot {} is before current slot {}. Use rollback_to() first.", slot, self.current_slot);
        }

        // Parse block using Pallas MultiEraBlock
        let block = MultiEraBlock::decode(block_bytes)
            .context("Failed to decode block")?;

        let mut stats = BlockStats {
            slot,
            block_hash: block_hash.to_vec(),
            tx_count: 0,
            utxos_created: 0,
            utxos_spent: 0,
            addresses_affected: HashSet::new(),
        };

        // Get block timestamp (Unix milliseconds)
        // For all eras, calculate timestamp from slot using network's genesis time
        let block_timestamp = self.system_start_ms + (slot * 1000);

        let mut rollback_info = RollbackInfo {
            slot,
            block_hash: block_hash.to_vec(),
            timestamp: block_timestamp,
            utxos_created: Vec::new(),
            utxos_spent: Vec::new(),
            spent_utxos_recorded: Vec::new(),
        };

        // Update epoch tracking
        let epoch = slot_to_epoch(slot);
        let epoch_boundary = epoch > self.current_epoch;
        if epoch_boundary {
            tracing::info!("📅 Epoch boundary: {} → {}", self.current_epoch, epoch);
            self.current_epoch = epoch;
        }

        // Process each transaction in the block
        for (tx_index, tx) in block.txs().into_iter().enumerate() {
            self.process_transaction(&tx, slot, block_hash, tx_index as u32, block_timestamp, &mut stats, &mut rollback_info).await?;
        }

        // Persist block events for ReadUtxoEvents RPC queries
        let mut event_index = 0u64;

        // Write CREATED events
        for utxo_key in &rollback_info.utxos_created {
            if let Some(utxo_data) = self.storage.get_utxo(utxo_key.clone()).await? {
                let event_key = format!("slot#{:020}#{:010}", slot, event_index);
                let event_data = serde_json::json!({
                    "event_type": "CREATED",
                    "utxo_key": utxo_key,
                    "utxo_data": serde_json::from_slice::<serde_json::Value>(&utxo_data).ok(),
                });
                let event_bytes = serde_json::to_vec(&event_data)?;
                self.storage.insert_block_event(event_key, event_bytes).await?;
                event_index += 1;
            }
        }

        // Write SPENT events
        for (utxo_key, utxo_data, spend_event_bytes) in &rollback_info.utxos_spent {
            // Parse UTxO data and spend metadata to include in the event
            let utxo_json: serde_json::Value = serde_json::from_slice(utxo_data)?;
            let spend_data: serde_json::Value = serde_json::from_slice(spend_event_bytes)?;

            let event_key = format!("slot#{:020}#{:010}", slot, event_index);
            let event_data = serde_json::json!({
                "event_type": "SPENT",
                "utxo_key": utxo_key,
                "utxo_data": utxo_json,
                "spend_data": spend_data,
            });
            let event_bytes = serde_json::to_vec(&event_data)?;
            self.storage.insert_block_event(event_key, event_bytes).await?;
            event_index += 1;
        }

        // Store block metadata in the block hash index for GetBlockByHash queries
        // Extract previous block hash from header (None for genesis/epoch boundary blocks)
        let prev_hash = block.header().previous_hash().map(|h| h.to_vec());
        self.storage.store_block_metadata(block_hash.to_vec(), slot, block_timestamp, prev_hash).await?;

        self.current_slot = slot;
        self.blocks_processed += 1;

        // Update chain tip estimate
        // If tip provided by node, use it; otherwise assume current slot is tip
        if let Some(tip_slot) = chain_tip_slot {
            self.chain_tip_slot = tip_slot;
        } else if slot > self.chain_tip_slot {
            self.chain_tip_slot = slot;
        }

        // Add to rollback buffer
        self.rollback_buffer.push_back(rollback_info);
        if self.rollback_buffer.len() > self.rollback_buffer_size {
            self.rollback_buffer.pop_front();
        }

        // Only update tips at epoch boundaries to minimize WAL bloat
        // Tips are also updated on rollbacks and shutdown
        if epoch_boundary {
            self.save_current_tips().await?;
        }

        // Check if we should create a snapshot
        // Strategy: epoch-based during bulk sync, time-based (5min) near tip
        let blocks_behind = self.chain_tip_slot.saturating_sub(slot);
        if self.snapshot_manager.should_snapshot(slot, epoch, self.chain_tip_slot) {
            tracing::info!("📸 Creating snapshot at slot {}, epoch {} ({} blocks behind tip)", slot, epoch, blocks_behind);
            if let Err(e) = self.storage.save_snapshots(slot).await {
                tracing::error!("Failed to save snapshots at slot {}: {}", slot, e);
                // Continue processing - next snapshot opportunity will retry
            } else {
                self.snapshot_manager.record_snapshot(slot, epoch);
                tracing::info!("✓ Saved snapshots at slot {}, epoch {}", slot, epoch);

                // Cleanup old snapshots to prevent unbounded accumulation
                // This dramatically improves snapshot creation performance
                if let Err(e) = self.storage.cleanup_snapshots(Some(10)).await {
                    tracing::warn!("Failed to cleanup old snapshots: {}", e);
                    // Non-fatal - continue processing
                }
            }
        }

        if self.blocks_processed.is_multiple_of(1000) {
            tracing::info!(
                "Progress: {} blocks, slot {}, epoch {}",
                self.blocks_processed,
                slot,
                epoch
            );
        }

        Ok(stats)
    }

    /// Roll back to a specific slot (inclusive)
    /// This will undo all blocks after the target slot
    pub async fn rollback_to(&mut self, target_slot: u64) -> Result<usize> {
        let mut blocks_rolled_back = 0;

        // Get current chain tip from database
        let current_tip = self.storage.get_chain_tip().await?;
        let current_slot = current_tip.as_ref().map(|t| t.slot).unwrap_or(0);

        tracing::warn!("⚠️  Rolling back from slot {} to slot {}", current_slot, target_slot);

        // If target slot is >= current slot, nothing to do
        if target_slot >= current_slot {
            tracing::info!("Target slot {} >= current slot {}, nothing to rollback", target_slot, current_slot);
            return Ok(0);
        }

        // Roll back blocks from the in-memory buffer first
        while let Some(rollback_info) = self.rollback_buffer.back() {
            if rollback_info.slot <= target_slot {
                break;
            }

            let rollback_info = self.rollback_buffer.pop_back().unwrap();
            self.rollback_block(&rollback_info).await?;
            blocks_rolled_back += 1;
        }

        // Update chain tip to target slot
        // Note: This is a simplified rollback that just resets the tip.
        // It doesn't undo UTxO changes, balances, etc. The node will resync from this point.
        self.storage.store_chain_tip(target_slot, vec![], 0).await?;

        // Also update wallet tips to target slot for all tracked wallets
        for wallet_id in &self.wallet_ids {
            self.storage.store_wallet_tip(wallet_id.clone(), target_slot, vec![], 0).await?;
        }

        self.current_slot = target_slot;

        // Estimate blocks rolled back from slot difference
        if blocks_rolled_back == 0 && current_slot > target_slot {
            // Rough estimate: assume 1 block per slot on average
            blocks_rolled_back = (current_slot - target_slot) as usize;
        }

        tracing::info!("✓ Rolled back to slot {} (approximately {} slots)", self.current_slot, blocks_rolled_back);

        Ok(blocks_rolled_back)
    }

    /// Save current chain tip and wallet tips
    /// Should be called at epoch boundaries, rollbacks, and shutdown
    pub async fn save_current_tips(&mut self) -> Result<()> {
        // Get the current block hash from the most recent block in rollback buffer
        if let Some(last_block) = self.rollback_buffer.back() {
            // Store chain tip
            self.storage.store_chain_tip(last_block.slot, last_block.block_hash.clone(), last_block.timestamp).await?;

            // Store per-wallet tips for all tracked wallets
            for wallet_id in &self.wallet_ids {
                self.storage.store_wallet_tip(wallet_id.clone(), last_block.slot, last_block.block_hash.clone(), last_block.timestamp).await?;
            }

            tracing::info!("💾 Saved tips at slot {}", last_block.slot);
        }

        Ok(())
    }

    /// Roll back a single block using rollback info
    async fn rollback_block(&mut self, info: &RollbackInfo) -> Result<()> {
        tracing::debug!("Rolling back block at slot {}", info.slot);

        // Delete block metadata from the block hash index
        self.storage.delete_block_metadata(info.block_hash.clone()).await?;

        // Restore spent UTxOs
        for (utxo_key, utxo_data, _spend_event_bytes) in &info.utxos_spent {
            self.storage.insert_utxo(utxo_key.clone(), utxo_data.clone()).await?;

            // Restore balance and address index
            if let Ok(utxo_json) = serde_json::from_slice::<serde_json::Value>(utxo_data) {
                if let Some(address) = utxo_json.get("address").and_then(|a| a.as_str()) {
                    if let Some(amount) = utxo_json.get("amount").and_then(|a| a.as_u64()) {
                        let current_balance = self.storage.get_balance(address.to_string()).await?;
                        self.storage.update_balance(address.to_string(), current_balance + amount).await?;
                    }

                    // Add back to address index
                    self.storage.add_utxo_to_address_index(address.to_string(), utxo_key.clone()).await?;
                }
            }
        }

        // Remove recorded spend events
        for utxo_key in &info.spent_utxos_recorded {
            self.storage.delete_spent_utxo(utxo_key.clone()).await?;
        }

        // Delete block events for this slot
        // We don't know exactly how many events there are, so we try deleting up to a reasonable limit
        for event_index in 0..100000u64 {
            let event_key = format!("slot#{:020}#{:010}", info.slot, event_index);

            // Try to get event - if it doesn't exist, we're done
            if self.storage.get_block_event(event_key.clone()).await?.is_some() {
                self.storage.delete_block_event(event_key).await?;
            } else {
                // No more events for this slot
                break;
            }
        }

        // Delete created UTxOs
        for utxo_key in &info.utxos_created {
            // Get UTxO data to update balance and index
            if let Some(utxo_data) = self.storage.get_utxo(utxo_key.clone()).await? {
                if let Ok(utxo_json) = serde_json::from_slice::<serde_json::Value>(&utxo_data) {
                    if let Some(address) = utxo_json.get("address").and_then(|a| a.as_str()) {
                        if let Some(amount) = utxo_json.get("amount").and_then(|a| a.as_u64()) {
                            let current_balance = self.storage.get_balance(address.to_string()).await?;
                            let new_balance = current_balance.saturating_sub(amount);
                            self.storage.update_balance(address.to_string(), new_balance).await?;
                        }

                        // Remove from address index
                        self.storage.remove_utxo_from_address_index(address.to_string(), utxo_key.clone()).await?;
                    }
                }
            }

            self.storage.delete_utxo(utxo_key.clone()).await?;
        }

        Ok(())
    }

    /// Process a single transaction
    async fn process_transaction(
        &mut self,
        tx: &MultiEraTx<'_>,
        slot: u64,
        block_hash: &[u8],
        tx_index: u32,
        block_timestamp: u64,
        stats: &mut BlockStats,
        rollback_info: &mut RollbackInfo,
    ) -> Result<()> {
        stats.tx_count += 1;

        let tx_hash = tx.hash();
        let tx_hash_hex = hex::encode(tx_hash.as_ref());

        // Process inputs (spend UTxOs)
        for input in tx.inputs() {
            self.process_input(&input, &tx_hash, slot, block_hash, tx_index, block_timestamp, stats, rollback_info).await?;
        }

        // Process outputs (create UTxOs)
        let outputs = tx.outputs();
        for (output_idx, output) in outputs.iter().enumerate() {
            self.process_output(output, &tx_hash, output_idx, slot, block_hash, tx_index, block_timestamp, stats, rollback_info).await?;
        }

        // Add transaction to history for all affected addresses
        for address_hex in &stats.addresses_affected {
            self.storage.add_tx_to_address_history(address_hex.clone(), tx_hash_hex.clone()).await?;
        }

        // Track native tokens - check if this transaction contains any tracked tokens
        if !self.tracked_tokens.is_empty() {
            self.track_tokens_in_transaction(tx, &tx_hash_hex).await?;
        }

        // TODO: Process certificates (delegations, pool registrations, etc.)
        // TODO: Process withdrawals
        // TODO: Process metadata

        Ok(())
    }

    /// Process an input (spend a UTxO)
    async fn process_input(
        &mut self,
        input: &pallas_traverse::MultiEraInput<'_>,
        spending_tx_hash: &Hash<32>,
        slot: u64,
        block_hash: &[u8],
        tx_index: u32,
        block_timestamp: u64,
        stats: &mut BlockStats,
        rollback_info: &mut RollbackInfo,
    ) -> Result<()> {
        // Build UTxO key from input
        let utxo_key = format!("{}#{}", hex::encode(input.hash()), input.index());

        // Check if this UTxO exists in our storage
        if let Some(utxo_data) = self.storage.get_utxo(utxo_key.clone()).await? {
            // Parse the stored UTxO data
            if let Ok(utxo_json) = serde_json::from_slice::<serde_json::Value>(&utxo_data) {
                // Extract address and amount
                if let Some(address) = utxo_json.get("address").and_then(|a| a.as_str()) {
                    stats.addresses_affected.insert(address.to_string());

                    // Update balance (subtract)
                    if let Some(amount) = utxo_json.get("amount").and_then(|a| a.as_u64()) {
                        let current_balance = self.storage.get_balance(address.to_string()).await?;
                        let new_balance = current_balance.saturating_sub(amount);
                        self.storage.update_balance(address.to_string(), new_balance).await?;
                    }

                    // Remove from address index
                    self.storage.remove_utxo_from_address_index(address.to_string(), utxo_key.clone()).await?;
                }
            }

            // Record spend event before deleting the UTxO
            let spend_event = serde_json::json!({
                "spent_at_slot": slot,
                "spent_at_block_hash": hex::encode(block_hash),
                "spent_at_tx_index": tx_index,
                "spent_at_tx_hash": hex::encode(spending_tx_hash.as_ref()),
                "spent_at_block_timestamp": block_timestamp,
            });

            let spend_event_bytes = serde_json::to_vec(&spend_event)?;
            self.storage.insert_spent_utxo(utxo_key.clone(), spend_event_bytes.clone()).await?;

            // Save for rollback (UTxO data + spend metadata)
            rollback_info.utxos_spent.push((utxo_key.clone(), utxo_data.clone(), spend_event_bytes));

            // Save spend event for rollback (so we can remove it if we rollback)
            rollback_info.spent_utxos_recorded.push(utxo_key.clone());

            // Delete the UTxO
            self.storage.delete_utxo(utxo_key.clone()).await?;
            stats.utxos_spent += 1;

            tracing::debug!("Spent UTxO: {} by tx {}", utxo_key, hex::encode(spending_tx_hash.as_ref()));
        }

        Ok(())
    }

    /// Process an output (create a UTxO)
    async fn process_output(
        &mut self,
        output: &MultiEraOutput<'_>,
        tx_hash: &Hash<32>,
        output_idx: usize,
        slot: u64,
        block_hash: &[u8],
        tx_index: u32,
        block_timestamp: u64,
        stats: &mut BlockStats,
        rollback_info: &mut RollbackInfo,
    ) -> Result<()> {
        let utxo_key = format!("{}#{}", hex::encode(tx_hash.as_ref()), output_idx);

        // Extract address and amount
        let address_bytes = output.address().context("Failed to get output address")?.to_vec();
        let address_hex = hex::encode(&address_bytes);

        // Check if this is an address we care about
        if !self.filter.is_our_address(&address_bytes) && !self.filter.tracked_payment_keys.is_empty() {
            return Ok(()); // Skip UTxOs we're not tracking
        }

        let value = output.value();
        let lovelace = value.coin();

        // Extract multi-assets
        let assets_map = {
            let assets = value.assets();
            if !assets.is_empty() {
                let mut map = serde_json::Map::new();
                for policy_assets in assets {
                    let policy_id_hex = hex::encode(policy_assets.policy().as_ref());
                    for asset in policy_assets.assets() {
                        let asset_name_hex = hex::encode(asset.name());
                        let key = format!("{}.{}", policy_id_hex, asset_name_hex);

                        // Extract actual asset amount based on era
                        let amount = match &asset {
                            pallas_traverse::MultiEraAsset::AlonzoCompatibleOutput(_, _, amt) => *amt,
                            pallas_traverse::MultiEraAsset::ConwayOutput(_, _, amt) => u64::from(*amt),
                            _ => 0, // Shouldn't happen for outputs
                        };

                        map.insert(key, serde_json::json!(amount));
                    }
                }
                Some(serde_json::Value::Object(map))
            } else {
                None
            }
        };

        // Extract datum - CRITICAL for midnight-node governance and CNight queries
        // Midnight-node needs RAW CBOR bytes (hex-encoded), not decoded JSON
        let (datum_hash_hex, inline_datum_hex) = match output.datum() {
            Some(datum_option) => {
                use pallas_primitives::conway::MintedDatumOption;
                match datum_option {
                    MintedDatumOption::Hash(hash) => {
                        // Datum hash only (datum stored separately on-chain)
                        (Some(hex::encode(hash)), None)
                    }
                    MintedDatumOption::Data(inline_datum) => {
                        // Inline datum (Babbage+ era) - extract raw CBOR bytes
                        let datum_bytes = inline_datum.raw_cbor().to_vec();

                        // Compute datum hash using Blake2b256
                        let mut hasher = pallas_crypto::hash::Hasher::<256>::new();
                        hasher.input(&datum_bytes);
                        let hash = hasher.finalize();

                        // Return both hash and raw CBOR (hex-encoded)
                        (Some(hex::encode(hash)), Some(hex::encode(datum_bytes)))
                    }
                }
            }
            None => (None, None),
        };

        // TODO: Extract script reference (Babbage+ feature)
        let script_ref_hex: Option<String> = None;

        // Store UTxO data with all fields midnight-node needs
        let mut utxo_data = serde_json::json!({
            "tx_hash": hex::encode(tx_hash.as_ref()),
            "output_index": output_idx,
            "address": address_hex,
            "amount": lovelace,
            "slot": slot,
            "block_hash": hex::encode(block_hash),
            "tx_index": tx_index,
            "block_timestamp": block_timestamp,
        });

        // Add optional fields only if present
        if let Some(assets) = assets_map {
            utxo_data["assets"] = assets;
        }
        if let Some(datum_hash) = datum_hash_hex {
            utxo_data["datum_hash"] = serde_json::json!(datum_hash);
        }
        if let Some(datum) = inline_datum_hex {
            utxo_data["datum"] = serde_json::json!(datum);
        }
        if let Some(script_ref) = script_ref_hex {
            utxo_data["script_ref"] = serde_json::json!(script_ref);
        }

        let utxo_bytes = serde_json::to_vec(&utxo_data)?;
        self.storage.insert_utxo(utxo_key.clone(), utxo_bytes).await?;

        // Save for rollback
        rollback_info.utxos_created.push(utxo_key.clone());

        // Update balance
        let current_balance = self.storage.get_balance(address_hex.clone()).await?;
        self.storage.update_balance(address_hex.clone(), current_balance + lovelace).await?;

        // Add to address index
        self.storage.add_utxo_to_address_index(address_hex.clone(), utxo_key.clone()).await?;

        stats.utxos_created += 1;
        stats.addresses_affected.insert(address_hex);

        tracing::debug!("Created UTxO: {} = {} lovelace", utxo_key, lovelace);

        Ok(())
    }

    /// Track native tokens in a transaction
    /// Checks if any outputs contain tracked tokens and adds tx to indexes
    async fn track_tokens_in_transaction(&mut self, tx: &MultiEraTx<'_>, tx_hash_hex: &str) -> Result<()> {
        // Check all outputs for tracked tokens
        for output in tx.outputs() {
            let value = output.value();

            // Extract multi-assets from the value
            // assets() returns a Vec of policy assets, empty vec if no assets
            let assets = value.assets();

            if assets.is_empty() {
                continue;
            }

            // Iterate through all policy IDs and assets in this output
            for policy_assets in &assets {
                let policy_id_bytes = policy_assets.policy();
                let policy_id_hex = hex::encode(policy_id_bytes.as_ref());

                // Check if this policy is tracked
                for tracked_token in &self.tracked_tokens {
                    if tracked_token.policy_id == policy_id_hex {
                        // Track at policy level
                        self.storage.add_tx_to_policy_index(policy_id_hex.clone(), tx_hash_hex.to_string()).await?;

                        // If tracking specific assets, check asset names
                        if let Some(ref tracked_asset_name) = tracked_token.asset_name {
                            for asset in policy_assets.assets() {
                                let asset_name_hex = hex::encode(asset.name());

                                if &asset_name_hex == tracked_asset_name {
                                    self.storage.add_tx_to_asset_index(
                                        policy_id_hex.clone(),
                                        asset_name_hex.clone(),
                                        tx_hash_hex.to_string()
                                    ).await?;

                                    tracing::debug!(
                                        "Tracked token in tx {}: {}.{}",
                                        tx_hash_hex,
                                        policy_id_hex,
                                        asset_name_hex
                                    );
                                }
                            }
                        } else {
                            // Track all assets under this policy
                            for asset in policy_assets.assets() {
                                let asset_name_hex = hex::encode(asset.name());
                                self.storage.add_tx_to_asset_index(
                                    policy_id_hex.clone(),
                                    asset_name_hex.clone(),
                                    tx_hash_hex.to_string()
                                ).await?;
                            }

                            tracing::debug!(
                                "Tracked policy in tx {}: {}",
                                tx_hash_hex,
                                policy_id_hex
                            );
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Create a final snapshot before shutdown
    ///
    /// This ensures minimal data loss by snapshotting the current state
    /// before the process exits gracefully.
    pub async fn create_final_snapshot(&mut self) -> Result<()> {
        let slot = self.current_slot;

        tracing::info!("Creating final snapshot at slot {}", slot);

        self.storage.save_snapshots(slot).await?;

        Ok(())
    }
}

/// Statistics from processing a block
#[derive(Debug, Clone)]
pub struct BlockStats {
    pub slot: u64,
    pub block_hash: Vec<u8>,
    pub tx_count: usize,
    pub utxos_created: usize,
    pub utxos_spent: usize,
    pub addresses_affected: HashSet<String>,
}

impl BlockStats {
    pub fn summary(&self) -> String {
        format!(
            "Block at slot {}: {} txs, +{} UTxOs, -{} UTxOs, {} addresses",
            self.slot,
            self.tx_count,
            self.utxos_created,
            self.utxos_spent,
            self.addresses_affected.len()
        )
    }
}

// Helper functions
pub fn slot_to_epoch(slot: u64) -> u64 {
    // TODO: This should come from network config or shelley genesis
    // For now, defaulting to Preview/SanchoNet epoch length (86400)
    // Mainnet/Preprod use 432000 slots per epoch
    slot / 86_400
}

pub fn is_epoch_boundary(slot: u64) -> bool {
    slot.is_multiple_of(86_400)
}

pub fn epoch_to_slot(epoch: u64) -> u64 {
    epoch * 86_400
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slot_epoch_conversion() {
        // Using Preview/SanchoNet epoch length (86400)
        assert_eq!(slot_to_epoch(0), 0);
        assert_eq!(slot_to_epoch(86_400), 1);
        assert_eq!(slot_to_epoch(172_800), 2);

        assert_eq!(epoch_to_slot(0), 0);
        assert_eq!(epoch_to_slot(1), 86_400);
        assert_eq!(epoch_to_slot(2), 172_800);
    }

    #[test]
    fn test_epoch_boundaries() {
        // Using Preview/SanchoNet epoch length (86400)
        assert!(is_epoch_boundary(0));
        assert!(is_epoch_boundary(86_400));
        assert!(!is_epoch_boundary(1));
        assert!(!is_epoch_boundary(86_401));
    }
}
