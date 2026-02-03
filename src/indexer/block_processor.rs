// Block processor - processes blocks from chain sync and updates storage

use crate::indexer::NetworkStorage;
use anyhow::{Context, Result};
use cardano_lsm::{Key, Value};
use pallas_crypto::hash::Hash;
use pallas_traverse::{MultiEraBlock, MultiEraTx, MultiEraOutput};
use std::collections::HashSet;

pub struct WalletFilter {
    tracked_payment_keys: HashSet<Hash<28>>,
    tracked_stake_keys: HashSet<Hash<28>>,
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
    pub fn is_our_address(&self, _address_bytes: &[u8]) -> bool {
        // For now, if we have no filters, track everything
        if self.tracked_payment_keys.is_empty() && self.tracked_stake_keys.is_empty() {
            return true;
        }

        // Parse address and check payment/stake credentials
        // TODO: Implement proper address parsing
        false
    }
}

pub struct BlockProcessor {
    pub storage: NetworkStorage,
    pub filter: WalletFilter,
    pub current_epoch: u64,
    pub blocks_processed: u64,
}

impl BlockProcessor {
    pub fn new(storage: NetworkStorage) -> Self {
        Self {
            storage,
            filter: WalletFilter::new(),
            current_epoch: 0,
            blocks_processed: 0,
        }
    }

    pub fn add_wallet(&mut self, payment_key: Hash<28>, stake_key: Hash<28>) {
        self.filter.add_payment_key_hash(payment_key);
        self.filter.add_stake_credential(stake_key);
    }

    /// Process a block from CBOR bytes
    pub fn process_block(&mut self, block_bytes: &[u8], slot: u64, block_hash: &[u8]) -> Result<BlockStats> {
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

        // Update epoch tracking
        let epoch = slot_to_epoch(slot);
        if epoch > self.current_epoch {
            tracing::info!("📅 Epoch boundary: {} → {}", self.current_epoch, epoch);
            self.current_epoch = epoch;
        }

        // Process each transaction in the block
        for tx in block.txs() {
            self.process_transaction(&tx, slot, block_hash, &mut stats)?;
        }

        self.blocks_processed += 1;

        if self.blocks_processed % 100 == 0 {
            tracing::info!(
                "Processed {} blocks, current slot: {}, epoch: {}",
                self.blocks_processed,
                slot,
                epoch
            );
        }

        Ok(stats)
    }

    /// Process a single transaction
    fn process_transaction(
        &mut self,
        tx: &MultiEraTx,
        slot: u64,
        block_hash: &[u8],
        stats: &mut BlockStats,
    ) -> Result<()> {
        stats.tx_count += 1;

        let tx_hash = tx.hash();

        // Process inputs (spend UTxOs)
        for input in tx.inputs() {
            self.process_input(&input, &tx_hash, stats)?;
        }

        // Process outputs (create UTxOs)
        let outputs = tx.outputs();
        for output_idx in 0..outputs.len() {
            self.process_output(&outputs[output_idx], &tx_hash, output_idx, slot, block_hash, stats)?;
        }

        // TODO: Process certificates (delegations, pool registrations, etc.)
        // TODO: Process withdrawals
        // TODO: Process metadata

        Ok(())
    }

    /// Process an input (spend a UTxO)
    fn process_input(
        &mut self,
        input: &pallas_traverse::MultiEraInput,
        _tx_hash: &Hash<32>,
        stats: &mut BlockStats,
    ) -> Result<()> {
        // Build UTxO key from input
        let utxo_key = format!("{}#{}", hex::encode(input.hash()), input.index());

        // Check if this UTxO exists in our storage
        let key = Key::from(utxo_key.as_bytes());
        if let Some(utxo_data) = self.storage.utxo_tree.get(&key)? {
            // Parse the stored UTxO data
            if let Ok(utxo_json) = serde_json::from_slice::<serde_json::Value>(utxo_data.as_ref()) {
                // Extract address and amount
                if let Some(address) = utxo_json.get("address").and_then(|a| a.as_str()) {
                    stats.addresses_affected.insert(address.to_string());

                    // Update balance (subtract)
                    if let Some(amount) = utxo_json.get("amount").and_then(|a| a.as_u64()) {
                        let address_key = Key::from(address.as_bytes());
                        let current_balance = self.storage.balance_tree.get(&address_key)?;
                        let new_balance = current_balance.saturating_sub(amount);
                        self.storage.balance_tree.insert(&address_key, &new_balance)?;
                    }
                }
            }

            // Delete the UTxO
            self.storage.utxo_tree.delete(&key)?;
            stats.utxos_spent += 1;

            tracing::debug!("Spent UTxO: {}", utxo_key);
        }

        Ok(())
    }

    /// Process an output (create a UTxO)
    fn process_output(
        &mut self,
        output: &MultiEraOutput,
        tx_hash: &Hash<32>,
        output_idx: usize,
        slot: u64,
        block_hash: &[u8],
        stats: &mut BlockStats,
    ) -> Result<()> {
        let utxo_key = format!("{}#{}", hex::encode(tx_hash.as_ref()), output_idx);

        // Extract address and amount
        let address_bytes = output.address().context("Failed to get output address")?.to_vec();
        let address_hex = hex::encode(&address_bytes);

        // Check if this is an address we care about
        if !self.filter.is_our_address(&address_bytes) && !self.filter.tracked_payment_keys.is_empty() {
            return Ok(()); // Skip UTxOs we're not tracking
        }

        let lovelace = output.value().coin();

        // Store UTxO data
        let utxo_data = serde_json::json!({
            "tx_hash": hex::encode(tx_hash.as_ref()),
            "output_index": output_idx,
            "address": address_hex,
            "amount": lovelace,
            "slot": slot,
            "block_hash": hex::encode(block_hash),
            // TODO: Add assets, datum, script_ref when needed
        });

        let key = Key::from(utxo_key.as_bytes());
        let value = Value::from(&serde_json::to_vec(&utxo_data)?);
        self.storage.utxo_tree.insert(&key, &value)?;

        // Update balance
        let address_key = Key::from(address_hex.as_bytes());
        let current_balance = self.storage.balance_tree.get(&address_key)?;
        self.storage.balance_tree.insert(&address_key, &(current_balance + lovelace))?;

        stats.utxos_created += 1;
        stats.addresses_affected.insert(address_hex);

        tracing::debug!("Created UTxO: {} = {} lovelace", utxo_key, lovelace);

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
    slot / 432_000 // Cardano epoch = 432,000 slots (5 days)
}

pub fn is_epoch_boundary(slot: u64) -> bool {
    slot % 432_000 == 0
}

pub fn epoch_to_slot(epoch: u64) -> u64 {
    epoch * 432_000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slot_epoch_conversion() {
        assert_eq!(slot_to_epoch(0), 0);
        assert_eq!(slot_to_epoch(432_000), 1);
        assert_eq!(slot_to_epoch(864_000), 2);

        assert_eq!(epoch_to_slot(0), 0);
        assert_eq!(epoch_to_slot(1), 432_000);
        assert_eq!(epoch_to_slot(2), 864_000);
    }

    #[test]
    fn test_epoch_boundaries() {
        assert!(is_epoch_boundary(0));
        assert!(is_epoch_boundary(432_000));
        assert!(!is_epoch_boundary(1));
        assert!(!is_epoch_boundary(432_001));
    }
}
