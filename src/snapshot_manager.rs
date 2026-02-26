// Snapshot management for cardano-lsm storage
//
// Implements adaptive snapshot strategy:
// - During bulk sync (>100 blocks behind): Snapshot once per epoch
// - Near tip (≤100 blocks behind): Snapshot every 5 minutes
//
// This optimizes for:
// - Minimal overhead during CPU-bound initial sync
// - Better recovery characteristics when network-bound near tip

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use anyhow::Result;

/// Manages snapshot timing and cleanup for LSM trees
pub struct SnapshotManager {
    /// Time of last snapshot
    last_snapshot_time: Instant,

    /// Slot of last snapshot
    last_snapshot_slot: u64,

    /// Epoch of last snapshot
    last_snapshot_epoch: u64,

    /// Minimum time between snapshots when near tip (5 minutes)
    time_interval: Duration,

    /// Distance from tip to switch strategies (100 blocks)
    tip_threshold: u64,

    /// Maximum number of snapshots to keep
    max_snapshots: usize,
}

impl SnapshotManager {
    /// Create a new snapshot manager
    ///
    /// # Arguments
    /// * `tip_threshold` - Blocks behind tip to trigger time-based snapshots (default: 100)
    /// * `time_interval_secs` - Seconds between snapshots when near tip (default: 300 = 5 minutes)
    /// * `max_snapshots` - Maximum snapshots to keep (default: 10)
    pub fn new(tip_threshold: u64, time_interval_secs: u64, max_snapshots: usize) -> Self {
        Self {
            last_snapshot_time: Instant::now(),
            last_snapshot_slot: 0,
            last_snapshot_epoch: 0,
            time_interval: Duration::from_secs(time_interval_secs),
            tip_threshold,
            max_snapshots,
        }
    }

    /// Create with default settings (100 blocks, 5 minutes, keep 10)
    pub fn default() -> Self {
        Self::new(100, 300, 10)
    }

    /// Determine if a snapshot should be taken
    ///
    /// Returns true if:
    /// - Near tip (≤100 blocks behind) AND 5 minutes elapsed, OR
    /// - Bulk sync (>100 blocks behind) AND new epoch
    pub fn should_snapshot(
        &self,
        current_slot: u64,
        current_epoch: u64,
        chain_tip_slot: u64,
    ) -> bool {
        // Check if we're near the tip
        let blocks_behind = chain_tip_slot.saturating_sub(current_slot);

        if blocks_behind <= self.tip_threshold {
            // Near tip: Time-based (every 5 minutes)
            self.last_snapshot_time.elapsed() >= self.time_interval
        } else {
            // Bulk sync: Epoch-based (once per epoch)
            current_epoch > self.last_snapshot_epoch
        }
    }

    /// Record that a snapshot was taken
    pub fn record_snapshot(&mut self, slot: u64, epoch: u64) {
        self.last_snapshot_time = Instant::now();
        self.last_snapshot_slot = slot;
        self.last_snapshot_epoch = epoch;
    }

    /// Get the snapshot name for a given slot
    ///
    /// Format: "slot-{slot:020}" (20-digit zero-padded)
    /// Example: "slot-00000000000000012345"
    ///
    /// This format is:
    /// - Sortable lexicographically
    /// - Parseable (extract slot number)
    /// - Deterministic
    pub fn snapshot_name(slot: u64) -> String {
        format!("slot-{:020}", slot)
    }

    /// Parse slot number from snapshot name
    ///
    /// Returns None if name doesn't match expected format
    pub fn parse_snapshot_slot(name: &str) -> Option<u64> {
        name.strip_prefix("slot-")
            .and_then(|s| s.parse::<u64>().ok())
    }

    /// Find the latest snapshot in a tree directory
    ///
    /// Returns the snapshot name, or None if no snapshots exist
    pub fn find_latest_snapshot(tree_path: &Path) -> Result<Option<String>> {
        let snapshots_dir = tree_path.join("snapshots");

        if !snapshots_dir.exists() {
            return Ok(None);
        }

        let mut snapshots = Vec::new();

        for entry in std::fs::read_dir(&snapshots_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    // Only include snapshots that match our naming convention
                    if Self::parse_snapshot_slot(name).is_some() {
                        snapshots.push(name.to_string());
                    }
                }
            }
        }

        if snapshots.is_empty() {
            return Ok(None);
        }

        // Sort and return latest (lexicographic sort works with zero-padded format)
        snapshots.sort();
        Ok(snapshots.last().cloned())
    }

    /// Clean up old snapshots, keeping only the most recent N
    ///
    /// # Arguments
    /// * `tree_path` - Path to the LSM tree directory
    /// * `keep_latest` - Number of snapshots to keep (default: uses max_snapshots)
    pub fn cleanup_old_snapshots(&self, tree_path: &Path, keep_latest: Option<usize>) -> Result<()> {
        let keep = keep_latest.unwrap_or(self.max_snapshots);
        let snapshots_dir = tree_path.join("snapshots");

        if !snapshots_dir.exists() {
            return Ok(());
        }

        let mut snapshots = Vec::new();

        for entry in std::fs::read_dir(&snapshots_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    if Self::parse_snapshot_slot(name).is_some() {
                        snapshots.push((name.to_string(), entry.path()));
                    }
                }
            }
        }

        if snapshots.len() <= keep {
            return Ok(());
        }

        // Sort by name (which sorts by slot due to our naming convention)
        snapshots.sort_by(|a, b| a.0.cmp(&b.0));

        // Delete oldest snapshots
        let to_delete = snapshots.len() - keep;
        for (name, path) in snapshots.iter().take(to_delete) {
            if let Err(e) = std::fs::remove_dir_all(path) {
                tracing::warn!("Failed to delete old snapshot {}: {}", name, e);
                // Continue with other deletions
            } else {
                tracing::debug!("Cleaned up old snapshot: {}", name);
            }
        }

        Ok(())
    }

    /// Restore a snapshot by name to the active directory
    ///
    /// This hard-links all files from snapshots/<name>/ to active/
    /// Call this before opening the LSM tree.
    ///
    /// # Arguments
    /// * `tree_path` - Path to the LSM tree directory (contains active/ and snapshots/)
    /// * `snapshot_name` - Name of the snapshot to restore
    ///
    /// # Returns
    /// The slot number of the restored snapshot
    pub fn restore_snapshot(tree_path: &Path, snapshot_name: &str) -> Result<u64> {
        let snapshot_dir = tree_path.join("snapshots").join(snapshot_name);
        let active_dir = tree_path.join("active");

        if !snapshot_dir.exists() {
            anyhow::bail!("Snapshot {} does not exist", snapshot_name);
        }

        // Parse slot from snapshot name
        let slot = Self::parse_snapshot_slot(snapshot_name)
            .ok_or_else(|| anyhow::anyhow!("Invalid snapshot name format: {}", snapshot_name))?;

        // Clear active directory
        if active_dir.exists() {
            std::fs::remove_dir_all(&active_dir)?;
        }
        std::fs::create_dir_all(&active_dir)?;

        // Hard-link all files from snapshot to active
        // This is fast and space-efficient
        for entry in std::fs::read_dir(&snapshot_dir)? {
            let entry = entry?;
            let file_name = entry.file_name();

            // Skip metadata files
            if file_name == "metadata" || file_name == "metadata.checksum" {
                continue;
            }

            let src_path = entry.path();
            let dst_path = active_dir.join(&file_name);

            if src_path.is_file() {
                // Hard-link the file
                std::fs::hard_link(&src_path, &dst_path)?;
                tracing::trace!("Hard-linked {} to active/", file_name.to_string_lossy());
            }
        }

        tracing::info!("Restored snapshot {} (slot {})", snapshot_name, slot);
        Ok(slot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_naming() {
        assert_eq!(SnapshotManager::snapshot_name(12345), "slot-00000000000000012345");
        assert_eq!(SnapshotManager::snapshot_name(0), "slot-00000000000000000000");
        assert_eq!(SnapshotManager::snapshot_name(999999999), "slot-00000000000999999999");
    }

    #[test]
    fn test_snapshot_parsing() {
        assert_eq!(SnapshotManager::parse_snapshot_slot("slot-00000000000000012345"), Some(12345));
        assert_eq!(SnapshotManager::parse_snapshot_slot("slot-00000000000000000000"), Some(0));
        assert_eq!(SnapshotManager::parse_snapshot_slot("invalid"), None);
        assert_eq!(SnapshotManager::parse_snapshot_slot("slot-abc"), None);
    }

    #[test]
    fn test_snapshot_decision_near_tip() {
        let manager = SnapshotManager::default();

        // Simulate being 50 blocks behind tip (near tip)
        // Should snapshot based on time, not epoch
        let current_slot = 100;
        let current_epoch = 1;
        let chain_tip_slot = 150; // 50 blocks behind

        // Immediately after previous snapshot - should NOT snapshot
        assert!(!manager.should_snapshot(current_slot, current_epoch, chain_tip_slot));
    }

    #[test]
    fn test_snapshot_decision_bulk_sync() {
        let mut manager = SnapshotManager::default();
        manager.last_snapshot_epoch = 5;

        // Simulate being 1000 blocks behind tip (bulk sync)
        let current_slot = 1000;
        let current_epoch = 6; // New epoch
        let chain_tip_slot = 2000; // 1000 blocks behind

        // New epoch - should snapshot
        assert!(manager.should_snapshot(current_slot, current_epoch, chain_tip_slot));
    }
}
