// Rewards tracking following cardano-wallet pattern
// - Query current rewards from ledger state
// - Track withdrawals from transactions
// - Snapshot at epoch boundaries (when running)
// - Historical data only available from when Hayate started indexing

#![allow(dead_code)]

use cardano_lsm::{LsmTree, LsmConfig, Key, Value};
use std::path::Path;
use serde::{Serialize, Deserialize};
use anyhow::Result;

/// Helper to find the latest snapshot for an LSM tree
fn get_latest_snapshot(tree_path: &std::path::Path) -> Result<Option<String>> {
    let temp_tree = LsmTree::open(tree_path, LsmConfig::default())?;
    let snapshots = temp_tree.list_snapshots()?;

    if snapshots.is_empty() {
        return Ok(None);
    }

    Ok(snapshots.into_iter().max())
}

/// Open an LSM tree, restoring from latest snapshot if available
fn open_lsm_tree_with_snapshot(tree_path: std::path::PathBuf) -> Result<LsmTree> {
    if let Some(snapshot_name) = get_latest_snapshot(&tree_path)? {
        tracing::info!("Restoring {:?} from snapshot: {}", tree_path.file_name().unwrap_or_default(), snapshot_name);
        Ok(LsmTree::open_snapshot(tree_path, &snapshot_name)?)
    } else {
        Ok(LsmTree::open(tree_path, LsmConfig::default())?)
    }
}

/// Stake credential (stake key hash)
pub type StakeCredential = Vec<u8>;

/// Reward account balance at a specific epoch
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RewardSnapshot {
    pub epoch: u64,
    pub balance: u64,  // Lovelace
    pub pool_id: Option<Vec<u8>>,  // Delegated pool
}

/// Withdrawal from rewards account
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Withdrawal {
    pub epoch: u64,
    pub slot: u64,
    pub amount: u64,
    pub tx_hash: Vec<u8>,
}

pub struct RewardsTracker {
    // Reward balance snapshots (epoch:stake_key -> RewardSnapshot)
    snapshots: LsmTree,
    
    // Withdrawal history (stake_key:epoch:slot -> Withdrawal)
    withdrawals: LsmTree,
    
    // Delegation history (stake_key:slot -> pool_id)
    delegations: LsmTree,
    
    // When we started indexing (for "data unavailable" responses)
    pub indexing_start_epoch: u64,
}

impl RewardsTracker {
    pub fn open(path: impl AsRef<Path>, start_epoch: u64) -> Result<Self> {
        let path = path.as_ref();

        Ok(Self {
            snapshots: open_lsm_tree_with_snapshot(path.join("reward_snapshots"))?,
            withdrawals: open_lsm_tree_with_snapshot(path.join("withdrawals"))?,
            delegations: open_lsm_tree_with_snapshot(path.join("delegations"))?,
            indexing_start_epoch: start_epoch,
        })
    }
    
    /// Store a reward withdrawal from a transaction
    pub fn record_withdrawal(
        &mut self,
        stake_key: &StakeCredential,
        epoch: u64,
        slot: u64,
        amount: u64,
        tx_hash: Vec<u8>,
    ) -> Result<()> {
        let withdrawal = Withdrawal {
            epoch,
            slot,
            amount,
            tx_hash,
        };
        
        let key = format!(
            "withdrawal:{}:{}:{}",
            hex::encode(stake_key),
            epoch,
            slot
        );
        
        self.withdrawals.insert(
            &Key::from(key.as_bytes()),
            &Value::from(&bincode::serialize(&withdrawal)?)
        )?;
        
        Ok(())
    }
    
    /// Store a delegation certificate
    pub fn record_delegation(
        &mut self,
        stake_key: &StakeCredential,
        pool_id: &[u8],
        slot: u64,
    ) -> Result<()> {
        let key = format!("delegation:{}:{}", hex::encode(stake_key), slot);
        
        self.delegations.insert(
            &Key::from(key.as_bytes()),
            &Value::from(pool_id)
        )?;
        
        Ok(())
    }
    
    /// Store a reward balance snapshot (called at epoch boundaries)
    /// This is how we build historical data!
    pub fn snapshot_rewards(
        &mut self,
        stake_key: &StakeCredential,
        epoch: u64,
        balance: u64,
        pool_id: Option<Vec<u8>>,
    ) -> Result<()> {
        let snapshot = RewardSnapshot {
            epoch,
            balance,
            pool_id,
        };
        
        let key = format!("snapshot:{}:{}", hex::encode(stake_key), epoch);
        
        self.snapshots.insert(
            &Key::from(key.as_bytes()),
            &Value::from(&bincode::serialize(&snapshot)?)
        )?;
        
        Ok(())
    }
    
    /// Get reward snapshot for a specific epoch
    /// Returns None if we don't have data (before we started indexing)
    pub fn get_snapshot(
        &self,
        stake_key: &StakeCredential,
        epoch: u64,
    ) -> Result<Option<RewardSnapshot>> {
        if epoch < self.indexing_start_epoch {
            // Data not available - we weren't running yet
            return Ok(None);
        }
        
        let key = format!("snapshot:{}:{}", hex::encode(stake_key), epoch);
        
        if let Some(data) = self.snapshots.get(&Key::from(key.as_bytes()))? {
            let snapshot: RewardSnapshot = bincode::deserialize(data.as_ref())?;
            return Ok(Some(snapshot));
        }
        
        Ok(None)
    }
    
    /// Calculate rewards earned in a specific epoch
    pub fn calculate_epoch_reward(
        &self,
        stake_key: &StakeCredential,
        epoch: u64,
    ) -> Result<Option<u64>> {
        // Need snapshots for both epochs
        let snapshot_current = match self.get_snapshot(stake_key, epoch)? {
            Some(s) => s,
            None => return Ok(None),  // Data not available
        };
        
        let snapshot_prev = match self.get_snapshot(stake_key, epoch.saturating_sub(1))? {
            Some(s) => s,
            None => {
                // First epoch we have data for
                // Return the balance as lifetime rewards up to this point
                return Ok(Some(snapshot_current.balance));
            }
        };
        
        // Get withdrawals in this epoch
        let withdrawals = self.get_epoch_withdrawals(stake_key, epoch)?;
        
        // Earned = (current + withdrawals) - previous
        let earned = (snapshot_current.balance + withdrawals)
            .saturating_sub(snapshot_prev.balance);
        
        Ok(Some(earned))
    }
    
    /// Get total withdrawals for an epoch
    pub fn get_epoch_withdrawals(
        &self,
        stake_key: &StakeCredential,
        epoch: u64,
    ) -> Result<u64> {
        let prefix = format!("withdrawal:{}:{}", hex::encode(stake_key), epoch);
        
        let mut total = 0u64;
        for (_key, value) in self.withdrawals.scan_prefix(prefix.as_bytes()) {
            let withdrawal: Withdrawal = bincode::deserialize(value.as_ref())?;
            total += withdrawal.amount;
        }
        
        Ok(total)
    }
    
    /// Get lifetime rewards (current balance + all withdrawals)
    pub fn get_lifetime_rewards(
        &self,
        stake_key: &StakeCredential,
        current_balance: u64,
    ) -> Result<u64> {
        let prefix = format!("withdrawal:{}", hex::encode(stake_key));
        
        let mut total_withdrawn = 0u64;
        for (_key, value) in self.withdrawals.scan_prefix(prefix.as_bytes()) {
            let withdrawal: Withdrawal = bincode::deserialize(value.as_ref())?;
            total_withdrawn += withdrawal.amount;
        }
        
        // Lifetime = current + all historical withdrawals
        Ok(current_balance + total_withdrawn)
    }
    
    /// Get current delegation for a stake key
    pub fn get_current_delegation(
        &self,
        stake_key: &StakeCredential,
    ) -> Result<Option<Vec<u8>>> {
        let prefix = format!("delegation:{}", hex::encode(stake_key));
        
        // Get latest delegation (highest slot)
        let mut latest: Option<(u64, Vec<u8>)> = None;
        
        for (key, value) in self.delegations.scan_prefix(prefix.as_bytes()) {
            // Extract slot from key: "delegation:stake_key:slot"
            if let Some(slot_str) = key.as_ref()
                .split(|&b| b == b':')
                .nth(2)
                .and_then(|s| std::str::from_utf8(s).ok())
                .and_then(|s| s.parse::<u64>().ok())
            {
                let pool_id = value.as_ref().to_vec();
                
                if latest.is_none() || slot_str > latest.as_ref().unwrap().0 {
                    latest = Some((slot_str, pool_id));
                }
            }
        }
        
        Ok(latest.map(|(_, pool_id)| pool_id))
    }

    /// Save snapshots of all rewards tracker trees
    ///
    /// This creates consistent snapshots for all 3 trees (snapshots, withdrawals, delegations)
    pub fn save_snapshot(&mut self, slot: u64) -> Result<()> {
        let snapshot_name = format!("slot-{:020}", slot);
        let label = format!("Slot {}", slot);

        self.snapshots.save_snapshot(&snapshot_name, &label)?;
        self.withdrawals.save_snapshot(&snapshot_name, &label)?;
        self.delegations.save_snapshot(&snapshot_name, &label)?;

        tracing::debug!("Saved rewards tracker snapshots at slot {}", slot);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[test]
    fn test_withdrawal_tracking() {
        let temp = TempDir::new().unwrap();
        let mut tracker = RewardsTracker::open(temp.path(), 100).unwrap();
        
        let stake_key = vec![1, 2, 3, 4];
        
        // Record withdrawal
        tracker.record_withdrawal(&stake_key, 100, 1000, 5_000_000, vec![]).unwrap();
        
        // Verify
        let withdrawn = tracker.get_epoch_withdrawals(&stake_key, 100).unwrap();
        assert_eq!(withdrawn, 5_000_000);
    }
    
    #[test]
    fn test_epoch_reward_calculation() {
        let temp = TempDir::new().unwrap();
        let mut tracker = RewardsTracker::open(temp.path(), 100).unwrap();
        
        let stake_key = vec![1, 2, 3, 4];
        
        // Snapshot epoch 100: 10 ADA rewards
        tracker.snapshot_rewards(&stake_key, 100, 10_000_000, None).unwrap();
        
        // Snapshot epoch 101: 12 ADA rewards (earned 2 ADA)
        tracker.snapshot_rewards(&stake_key, 101, 12_000_000, None).unwrap();
        
        // Calculate earned in epoch 101
        let earned = tracker.calculate_epoch_reward(&stake_key, 101).unwrap();
        assert_eq!(earned, Some(2_000_000));
    }
}
