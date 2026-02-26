// Hayate-Node Storage
// Full UTxO set and epoch boundary snapshots

use cardano_lsm::{LsmTree, LsmConfig, Key, Value};
use std::path::PathBuf;
use serde::{Serialize, Deserialize};
use anyhow::Result;
use std::collections::HashMap;

use crate::indexer::Network;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UtxoEntry {
    pub address: Vec<u8>,
    pub amount: u64,
    pub assets: HashMap<String, u64>, // policy_id.asset_name -> amount
    pub datum_hash: Option<Vec<u8>>,  // Hash of the datum (always present if datum exists)
    pub datum: Option<Vec<u8>>,       // Inline datum data (if present)
    pub script_ref: Option<Vec<u8>>,
    pub stake_credential: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StakeSnapshot {
    pub epoch: u64,
    pub amount: u64,           // Lovelace staked
    pub pool_id: Option<Vec<u8>>, // Pool delegated to
    pub rewards: u64,          // Unclaimed rewards
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolSnapshot {
    pub epoch: u64,
    pub pool_id: Vec<u8>,
    pub vrf_key: Vec<u8>,
    pub pledge: u64,
    pub cost: u64,
    pub margin_numerator: u64,
    pub margin_denominator: u64,
    pub owners: Vec<Vec<u8>>,
    // TODO: Add relays and metadata
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolParams {
    pub epoch: u64,
    pub epoch_length: u64,
    pub slot_length: u64,
    pub active_slots_coeff: f64,
    pub security_param: u64,
    // TODO: Add more protocol parameters
}

/// Full node storage for ledger state and snapshots
pub struct NodeStorage {
    pub network: Network,

    // Complete UTxO set
    pub utxo_tree: LsmTree,

    // Epoch snapshots
    pub stake_tree: LsmTree,      // stake:{epoch}:{stake_cred} -> StakeSnapshot
    pub pool_tree: LsmTree,       // pool:{epoch}:{pool_id} -> PoolSnapshot
    pub nonce_tree: LsmTree,      // nonce:{epoch} -> [u8; 32]
    pub protocol_tree: LsmTree,   // protocol:{epoch} -> ProtocolParams

    // Chain tip
    pub chain_tip_tree: LsmTree,

    // Track delegations (stake_cred -> pool_id)
    pub delegation_tree: LsmTree,

    // Track pool registrations
    pub pool_registration_tree: LsmTree,

    // In-memory stake tracking for current epoch (will be snapshotted at boundary)
    // This avoids needing to iterate all UTxOs
    current_stake: HashMap<Vec<u8>, u64>,

    #[allow(dead_code)]
    base_path: PathBuf,
}

impl NodeStorage {
    pub fn open(base_path: PathBuf, network: Network) -> Result<Self> {
        let network_path = base_path.join("node").join(network.as_str());

        tracing::info!("Opening node storage for {} at {:?}", network.as_str(), network_path);

        std::fs::create_dir_all(&network_path)?;

        let utxo_tree = LsmTree::open(network_path.join("utxos"), LsmConfig::default())?;
        let stake_tree = LsmTree::open(network_path.join("stakes"), LsmConfig::default())?;
        let pool_tree = LsmTree::open(network_path.join("pools"), LsmConfig::default())?;
        let nonce_tree = LsmTree::open(network_path.join("nonces"), LsmConfig::default())?;
        let protocol_tree = LsmTree::open(network_path.join("protocol"), LsmConfig::default())?;
        let chain_tip_tree = LsmTree::open(network_path.join("chain_tip"), LsmConfig::default())?;
        let delegation_tree = LsmTree::open(network_path.join("delegations"), LsmConfig::default())?;
        let pool_registration_tree = LsmTree::open(network_path.join("pool_registrations"), LsmConfig::default())?;

        Ok(Self {
            network,
            utxo_tree,
            stake_tree,
            pool_tree,
            nonce_tree,
            protocol_tree,
            chain_tip_tree,
            delegation_tree,
            pool_registration_tree,
            base_path: network_path,
            current_stake: std::collections::HashMap::new(),
        })
    }

    // UTxO operations

    pub fn insert_utxo(&mut self, tx_hash: &[u8], output_index: u32, utxo: &UtxoEntry) -> Result<()> {
        let key = format!("{}:{}", hex::encode(tx_hash), output_index);
        let value = bincode::serialize(utxo)?;

        self.utxo_tree.insert(
            &Key::from(key.as_bytes()),
            &Value::from(&value)
        )?;

        // Update in-memory stake tracking
        if let Some(stake_cred) = &utxo.stake_credential {
            *self.current_stake.entry(stake_cred.clone()).or_insert(0) += utxo.amount;
        }

        Ok(())
    }

    pub fn remove_utxo(&mut self, tx_hash: &[u8], output_index: u32) -> Result<Option<UtxoEntry>> {
        let key = format!("{}:{}", hex::encode(tx_hash), output_index);
        let key_bytes = Key::from(key.as_bytes());

        let utxo: Option<UtxoEntry> = if let Some(value) = self.utxo_tree.get(&key_bytes)? {
            Some(bincode::deserialize(value.as_ref())?)
        } else {
            None
        };

        // Update in-memory stake tracking
        if let Some(ref utxo_entry) = utxo {
            if let Some(stake_cred) = &utxo_entry.stake_credential {
                if let Some(current) = self.current_stake.get_mut(stake_cred) {
                    *current = current.saturating_sub(utxo_entry.amount);
                    if *current == 0 {
                        self.current_stake.remove(stake_cred);
                    }
                }
            }
        }

        // Delete by inserting empty value (tombstone)
        self.utxo_tree.insert(&key_bytes, &Value::from(&[] as &[u8]))?;

        Ok(utxo)
    }

    pub fn get_utxo(&self, tx_hash: &[u8], output_index: u32) -> Result<Option<UtxoEntry>> {
        let key = format!("{}:{}", hex::encode(tx_hash), output_index);

        if let Some(value) = self.utxo_tree.get(&Key::from(key.as_bytes()))? {
            Ok(Some(bincode::deserialize(value.as_ref())?))
        } else {
            Ok(None)
        }
    }

    // Delegation operations

    pub fn update_delegation(&mut self, stake_cred: &[u8], pool_id: &[u8]) -> Result<()> {
        let key = format!("delegation:{}", hex::encode(stake_cred));
        self.delegation_tree.insert(
            &Key::from(key.as_bytes()),
            &Value::from(pool_id)
        )?;
        Ok(())
    }

    pub fn get_delegation(&self, stake_cred: &[u8]) -> Result<Option<Vec<u8>>> {
        let key = format!("delegation:{}", hex::encode(stake_cred));

        if let Some(value) = self.delegation_tree.get(&Key::from(key.as_bytes()))? {
            Ok(Some(value.as_ref().to_vec()))
        } else {
            Ok(None)
        }
    }

    // Epoch snapshot operations

    /// Calculate and store stake snapshot at epoch boundary
    /// Uses in-memory stake tracking for efficiency
    pub fn snapshot_stake_distribution(&mut self, epoch: u64) -> Result<HashMap<Vec<u8>, u64>> {
        tracing::info!("📸 Creating stake distribution snapshot for epoch {}", epoch);

        // Use the in-memory current_stake map
        let stake_map = self.current_stake.clone();

        tracing::info!("Found {} stake keys with {} total lovelace",
            stake_map.len(),
            stake_map.values().sum::<u64>());

        // Store snapshots
        for (stake_cred, amount) in &stake_map {
            let pool_id = self.get_delegation(stake_cred)?;

            let snapshot = StakeSnapshot {
                epoch,
                amount: *amount,
                pool_id,
                rewards: 0, // TODO: Get from ledger state
            };

            self.store_stake_snapshot(stake_cred, epoch, &snapshot)?;
        }

        tracing::info!("✅ Stake snapshot complete for epoch {}: {} stake keys", epoch, stake_map.len());

        Ok(stake_map)
    }

    pub fn store_stake_snapshot(&mut self, stake_cred: &[u8], epoch: u64, snapshot: &StakeSnapshot) -> Result<()> {
        let key = format!("stake:{}:{}", epoch, hex::encode(stake_cred));
        let value = bincode::serialize(snapshot)?;

        self.stake_tree.insert(
            &Key::from(key.as_bytes()),
            &Value::from(&value)
        )?;

        Ok(())
    }

    pub fn get_stake_snapshot(&self, stake_cred: &[u8], epoch: u64) -> Result<Option<StakeSnapshot>> {
        let key = format!("stake:{}:{}", epoch, hex::encode(stake_cred));

        if let Some(value) = self.stake_tree.get(&Key::from(key.as_bytes()))? {
            Ok(Some(bincode::deserialize(value.as_ref())?))
        } else {
            Ok(None)
        }
    }

    // Nonce operations

    pub fn store_nonce(&mut self, epoch: u64, nonce: &[u8; 32]) -> Result<()> {
        let key = format!("nonce:{}", epoch);
        self.nonce_tree.insert(
            &Key::from(key.as_bytes()),
            &Value::from(&nonce[..])
        )?;

        tracing::info!("Stored epoch nonce for epoch {}: {}", epoch, hex::encode(nonce));
        Ok(())
    }

    pub fn get_nonce(&self, epoch: u64) -> Result<Option<[u8; 32]>> {
        let key = format!("nonce:{}", epoch);

        if let Some(value) = self.nonce_tree.get(&Key::from(key.as_bytes()))? {
            let bytes = value.as_ref();
            if bytes.len() == 32 {
                let mut nonce = [0u8; 32];
                nonce.copy_from_slice(bytes);
                Ok(Some(nonce))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    // Pool operations

    pub fn store_pool_snapshot(&mut self, pool_id: &[u8], epoch: u64, pool: &PoolSnapshot) -> Result<()> {
        let key = format!("pool:{}:{}", epoch, hex::encode(pool_id));
        let value = bincode::serialize(pool)?;

        self.pool_tree.insert(
            &Key::from(key.as_bytes()),
            &Value::from(&value)
        )?;

        Ok(())
    }

    pub fn get_pool_snapshot(&self, pool_id: &[u8], epoch: u64) -> Result<Option<PoolSnapshot>> {
        let key = format!("pool:{}:{}", epoch, hex::encode(pool_id));

        if let Some(value) = self.pool_tree.get(&Key::from(key.as_bytes()))? {
            Ok(Some(bincode::deserialize(value.as_ref())?))
        } else {
            Ok(None)
        }
    }

    // Chain tip operations

    pub fn store_chain_tip(&mut self, slot: u64, hash: &[u8]) -> Result<()> {
        let tip_data = serde_json::json!({
            "slot": slot,
            "hash": hex::encode(hash),
        });

        self.chain_tip_tree.insert(
            &Key::from(b"current_tip"),
            &Value::from(&serde_json::to_vec(&tip_data)?),
        )?;

        Ok(())
    }

    pub fn get_chain_tip(&self) -> Result<Option<(u64, Vec<u8>)>> {
        if let Some(value) = self.chain_tip_tree.get(&Key::from(b"current_tip"))? {
            let tip_data: serde_json::Value = serde_json::from_slice(value.as_ref())?;

            let slot = tip_data["slot"].as_u64().unwrap_or(0);
            let hash = hex::decode(tip_data["hash"].as_str().unwrap_or("")).unwrap_or_default();

            Ok(Some((slot, hash)))
        } else {
            Ok(None)
        }
    }

    /// Save snapshots of all LSM trees
    ///
    /// This creates a consistent snapshot across all node storage trees at the given slot.
    pub fn save_all_snapshots(&mut self, slot: u64) -> Result<()> {
        let snapshot_name = format!("slot-{:020}", slot);
        let label = format!("Slot {}", slot);

        tracing::debug!("Saving node storage snapshots at slot {} ({})", slot, snapshot_name);

        // Save all 8 LSM trees
        self.utxo_tree.save_snapshot(&snapshot_name, &label)?;
        self.stake_tree.save_snapshot(&snapshot_name, &label)?;
        self.pool_tree.save_snapshot(&snapshot_name, &label)?;
        self.nonce_tree.save_snapshot(&snapshot_name, &label)?;
        self.protocol_tree.save_snapshot(&snapshot_name, &label)?;
        self.chain_tip_tree.save_snapshot(&snapshot_name, &label)?;
        self.delegation_tree.save_snapshot(&snapshot_name, &label)?;
        self.pool_registration_tree.save_snapshot(&snapshot_name, &label)?;

        tracing::info!("Saved node storage snapshots at slot {}", slot);
        Ok(())
    }
}

// Helper functions for epoch calculations

pub fn slot_to_epoch(slot: u64, network: &Network) -> u64 {
    let epoch_length = match network {
        Network::Mainnet | Network::Preprod => 432_000,  // 5 days
        Network::Preview => 86_400,  // 1 day
        Network::SanchoNet => 86_400,  // 1 day (testnet)
        Network::Custom(_) => 432_000,
    };
    slot / epoch_length
}

pub fn is_epoch_boundary(slot: u64, network: &Network) -> bool {
    let epoch_length = match network {
        Network::Mainnet | Network::Preprod => 432_000,
        Network::Preview => 86_400,
        Network::SanchoNet => 86_400,
        Network::Custom(_) => 432_000,
    };
    (slot + 1) % epoch_length == 0
}

pub fn epoch_to_slot(epoch: u64, network: &Network) -> u64 {
    let epoch_length = match network {
        Network::Mainnet | Network::Preprod => 432_000,
        Network::Preview => 86_400,
        Network::SanchoNet => 86_400,
        Network::Custom(_) => 432_000,
    };
    epoch * epoch_length
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_epoch_calculations() {
        let network = Network::Preview;

        assert_eq!(slot_to_epoch(0, &network), 0);
        assert_eq!(slot_to_epoch(86_400, &network), 1);
        assert_eq!(slot_to_epoch(172_800, &network), 2);

        assert!(is_epoch_boundary(86_400 - 1, &network));
        assert!(is_epoch_boundary(172_800 - 1, &network));
        assert!(!is_epoch_boundary(0, &network));
        assert!(!is_epoch_boundary(86_400, &network));
    }
}
