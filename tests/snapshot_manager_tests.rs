// Snapshot Manager Integration Tests
// Tests for src/snapshot_manager.rs (297 LOC)
// Target: 85% coverage, 8 integration tests
// Note: Unit tests already exist in src/snapshot_manager.rs (4 tests)

use anyhow::Result;
use hayate::snapshot_manager::SnapshotManager;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

// Test 1: Find latest snapshot with real filesystem
#[test]
fn test_find_latest_snapshot_with_files() -> Result<()> {
    let temp = TempDir::new()?;
    let tree_path = temp.path().join("tree");
    let snapshots_dir = tree_path.join("snapshots");
    fs::create_dir_all(&snapshots_dir)?;

    // Create multiple snapshot directories
    fs::create_dir(snapshots_dir.join("slot-00000000000000001000"))?;
    fs::create_dir(snapshots_dir.join("slot-00000000000000002000"))?;
    fs::create_dir(snapshots_dir.join("slot-00000000000000001500"))?;

    // Find latest should return slot 2000
    let latest = SnapshotManager::find_latest_snapshot(&tree_path)?;
    assert!(latest.is_some());
    assert_eq!(latest.unwrap(), "slot-00000000000000002000");

    Ok(())
}

// Test 2: Find latest snapshot with no snapshots directory
#[test]
fn test_find_latest_snapshot_no_dir() -> Result<()> {
    let temp = TempDir::new()?;
    let tree_path = temp.path().join("tree");
    fs::create_dir_all(&tree_path)?;

    // No snapshots directory exists
    let latest = SnapshotManager::find_latest_snapshot(&tree_path)?;
    assert!(latest.is_none());

    Ok(())
}

// Test 3: Find latest snapshot with empty snapshots directory
#[test]
fn test_find_latest_snapshot_empty_dir() -> Result<()> {
    let temp = TempDir::new()?;
    let tree_path = temp.path().join("tree");
    let snapshots_dir = tree_path.join("snapshots");
    fs::create_dir_all(&snapshots_dir)?;

    // Empty snapshots directory
    let latest = SnapshotManager::find_latest_snapshot(&tree_path)?;
    assert!(latest.is_none());

    Ok(())
}

// Test 4: Find latest snapshot ignores invalid names
#[test]
fn test_find_latest_snapshot_ignores_invalid() -> Result<()> {
    let temp = TempDir::new()?;
    let tree_path = temp.path().join("tree");
    let snapshots_dir = tree_path.join("snapshots");
    fs::create_dir_all(&snapshots_dir)?;

    // Create valid and invalid snapshot directories
    fs::create_dir(snapshots_dir.join("slot-00000000000000001000"))?;
    fs::create_dir(snapshots_dir.join("invalid-snapshot"))?;
    fs::create_dir(snapshots_dir.join("slot-abc"))?;
    fs::create_dir(snapshots_dir.join("slot-00000000000000002000"))?;

    // Should find latest valid snapshot (ignoring invalid ones)
    let latest = SnapshotManager::find_latest_snapshot(&tree_path)?;
    assert!(latest.is_some());
    assert_eq!(latest.unwrap(), "slot-00000000000000002000");

    Ok(())
}

// Test 5: Cleanup old snapshots - keep N most recent
#[test]
fn test_cleanup_old_snapshots() -> Result<()> {
    let temp = TempDir::new()?;
    let tree_path = temp.path().join("tree");
    let snapshots_dir = tree_path.join("snapshots");
    fs::create_dir_all(&snapshots_dir)?;

    // Create 10 snapshot directories
    for i in 0..10 {
        let slot = 1000 + (i * 100);
        let snapshot_name = SnapshotManager::snapshot_name(slot);
        fs::create_dir(snapshots_dir.join(&snapshot_name))?;
    }

    // Verify 10 snapshots exist
    let count_before = fs::read_dir(&snapshots_dir)?.count();
    assert_eq!(count_before, 10);

    // Cleanup, keeping only 5 most recent
    let manager = SnapshotManager::new(100, 300, 600, 10);
    manager.cleanup_old_snapshots(&tree_path, Some(5))?;

    // Verify only 5 snapshots remain
    let count_after = fs::read_dir(&snapshots_dir)?.count();
    assert_eq!(count_after, 5);

    // Verify the 5 most recent are kept (slots 1500-1900)
    let remaining: Vec<String> = fs::read_dir(&snapshots_dir)?
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().to_str().map(String::from))
        .collect();

    assert!(remaining.contains(&SnapshotManager::snapshot_name(1500)));
    assert!(remaining.contains(&SnapshotManager::snapshot_name(1600)));
    assert!(remaining.contains(&SnapshotManager::snapshot_name(1700)));
    assert!(remaining.contains(&SnapshotManager::snapshot_name(1800)));
    assert!(remaining.contains(&SnapshotManager::snapshot_name(1900)));

    // Verify oldest snapshots were deleted
    assert!(!remaining.contains(&SnapshotManager::snapshot_name(1000)));
    assert!(!remaining.contains(&SnapshotManager::snapshot_name(1100)));

    Ok(())
}

// Test 6: Cleanup with fewer snapshots than keep limit
#[test]
fn test_cleanup_fewer_than_limit() -> Result<()> {
    let temp = TempDir::new()?;
    let tree_path = temp.path().join("tree");
    let snapshots_dir = tree_path.join("snapshots");
    fs::create_dir_all(&snapshots_dir)?;

    // Create only 3 snapshot directories
    fs::create_dir(snapshots_dir.join(SnapshotManager::snapshot_name(1000)))?;
    fs::create_dir(snapshots_dir.join(SnapshotManager::snapshot_name(2000)))?;
    fs::create_dir(snapshots_dir.join(SnapshotManager::snapshot_name(3000)))?;

    // Cleanup with keep=5 (more than we have)
    let manager = SnapshotManager::new(100, 300, 600, 10);
    manager.cleanup_old_snapshots(&tree_path, Some(5))?;

    // All 3 should remain
    let count_after = fs::read_dir(&snapshots_dir)?.count();
    assert_eq!(count_after, 3);

    Ok(())
}

// Test 7: Restore snapshot - basic functionality
#[test]
fn test_restore_snapshot() -> Result<()> {
    let temp = TempDir::new()?;
    let tree_path = temp.path().join("tree");
    let snapshots_dir = tree_path.join("snapshots");
    let snapshot_name = SnapshotManager::snapshot_name(5000);
    let snapshot_path = snapshots_dir.join(&snapshot_name);
    let active_dir = tree_path.join("active");

    fs::create_dir_all(&snapshot_path)?;
    fs::create_dir_all(&active_dir)?;

    // Create some files in the snapshot
    fs::write(snapshot_path.join("data1.sst"), b"test data 1")?;
    fs::write(snapshot_path.join("data2.sst"), b"test data 2")?;
    fs::write(snapshot_path.join("manifest"), b"manifest data")?;

    // Create a file in active directory (should be replaced)
    fs::write(active_dir.join("old_file.sst"), b"old data")?;

    // Restore the snapshot
    let restored_slot = SnapshotManager::restore_snapshot(&tree_path, &snapshot_name)?;
    assert_eq!(restored_slot, 5000);

    // Verify files were hard-linked to active directory
    assert!(active_dir.join("data1.sst").exists());
    assert!(active_dir.join("data2.sst").exists());
    assert!(active_dir.join("manifest").exists());

    // Verify old file was removed
    assert!(!active_dir.join("old_file.sst").exists());

    // Verify content is correct
    let content1 = fs::read(active_dir.join("data1.sst"))?;
    assert_eq!(content1, b"test data 1");

    Ok(())
}

// Test 8: Restore non-existent snapshot (error case)
#[test]
fn test_restore_nonexistent_snapshot() -> Result<()> {
    let temp = TempDir::new()?;
    let tree_path = temp.path().join("tree");
    fs::create_dir_all(&tree_path)?;

    let snapshot_name = SnapshotManager::snapshot_name(9999);

    // Attempting to restore non-existent snapshot should fail
    let result = SnapshotManager::restore_snapshot(&tree_path, &snapshot_name);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("does not exist"));

    Ok(())
}

// Test 9: Restore snapshot with invalid name (error case)
#[test]
fn test_restore_invalid_snapshot_name() -> Result<()> {
    let temp = TempDir::new()?;
    let tree_path = temp.path().join("tree");
    let snapshots_dir = tree_path.join("snapshots");
    fs::create_dir_all(&snapshots_dir)?;

    // Create snapshot with invalid name
    let invalid_name = "invalid-snapshot-name";
    fs::create_dir(snapshots_dir.join(invalid_name))?;

    // Attempting to restore with invalid name should fail
    let result = SnapshotManager::restore_snapshot(&tree_path, invalid_name);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Invalid snapshot name"));

    Ok(())
}

// Test 10: Recording snapshots updates state
#[test]
fn test_record_snapshot_updates_state() -> Result<()> {
    let mut manager = SnapshotManager::new(100, 300, 600, 10);

    // Record first snapshot
    manager.record_snapshot(1000, 10);

    // Check that should_snapshot returns false immediately after recording
    assert!(!manager.should_snapshot(1000, 10, 1100));

    // Record another snapshot
    manager.record_snapshot(2000, 20);

    // Should still not snapshot immediately
    assert!(!manager.should_snapshot(2000, 20, 2100));

    Ok(())
}

// Test 11: Snapshot decision based on blocks behind (near tip)
#[test]
fn test_snapshot_decision_blocks_behind_near_tip() -> Result<()> {
    let manager = SnapshotManager::new(100, 1, 5, 10); // 1 sec at tip, 5 sec bulk

    // 50 blocks behind (near tip) - uses 1 second interval
    let current_slot = 1000;
    let chain_tip_slot = 1050; // 50 blocks behind

    // Immediately after creation - should not snapshot
    assert!(!manager.should_snapshot(current_slot, 10, chain_tip_slot));

    // Wait 2 seconds (more than tip interval of 1 sec)
    std::thread::sleep(std::time::Duration::from_secs(2));
    assert!(manager.should_snapshot(current_slot, 10, chain_tip_slot));

    Ok(())
}

// Test 12: Snapshot decision based on blocks behind (bulk sync)
#[test]
fn test_snapshot_decision_blocks_behind_bulk() -> Result<()> {
    let manager = SnapshotManager::new(100, 1, 3, 10); // 1 sec at tip, 3 sec bulk

    // 500 blocks behind (bulk sync) - uses 3 second interval
    let current_slot = 1000;
    let chain_tip_slot = 1500; // 500 blocks behind

    // Immediately after creation - should not snapshot
    assert!(!manager.should_snapshot(current_slot, 10, chain_tip_slot));

    // Wait 2 seconds (less than bulk interval of 3 sec)
    std::thread::sleep(std::time::Duration::from_secs(2));
    assert!(!manager.should_snapshot(current_slot, 10, chain_tip_slot));

    // Wait another 2 seconds (total 4 sec, more than bulk interval)
    std::thread::sleep(std::time::Duration::from_secs(2));
    assert!(manager.should_snapshot(current_slot, 10, chain_tip_slot));

    Ok(())
}

// Helper function to create test snapshot structure
#[allow(dead_code)]
fn create_test_snapshot_structure(tree_path: &Path, slot: u64, num_files: usize) -> Result<()> {
    let snapshots_dir = tree_path.join("snapshots");
    let snapshot_name = SnapshotManager::snapshot_name(slot);
    let snapshot_path = snapshots_dir.join(&snapshot_name);
    fs::create_dir_all(&snapshot_path)?;

    for i in 0..num_files {
        let filename = format!("data{}.sst", i);
        fs::write(snapshot_path.join(filename), format!("test data {}", i))?;
    }

    Ok(())
}
