// Rewards Tracker Integration Tests
// Tests for src/rewards.rs (326 LOC)
// Target: 85% coverage, 10 integration tests
// Note: Unit tests already exist in src/rewards.rs (2 tests)

mod common;

use anyhow::Result;
use hayate::rewards::{RewardsTracker, RewardSnapshot};
use tempfile::TempDir;

// Test 1: Basic rewards tracker creation and initialization
#[test]
fn test_rewards_tracker_creation() -> Result<()> {
    let temp = TempDir::new()?;
    let start_epoch = 100;

    let tracker = RewardsTracker::open(temp.path(), start_epoch)?;
    assert_eq!(tracker.indexing_start_epoch, start_epoch);

    Ok(())
}

// Test 2: Record and retrieve withdrawal
#[test]
fn test_record_and_get_withdrawal() -> Result<()> {
    let temp = TempDir::new()?;
    let mut tracker = RewardsTracker::open(temp.path(), 100)?;

    let stake_key = vec![1, 2, 3, 4];
    let epoch = 105;
    let slot = 1000;
    let amount = 5_000_000u64; // 5 ADA
    let tx_hash = vec![9, 8, 7, 6];

    // Record withdrawal
    tracker.record_withdrawal(&stake_key, epoch, slot, amount, tx_hash)?;

    // Verify withdrawal was recorded
    let total_withdrawn = tracker.get_epoch_withdrawals(&stake_key, epoch)?;
    assert_eq!(total_withdrawn, amount);

    Ok(())
}

// Test 3: Multiple withdrawals in same epoch
#[test]
fn test_multiple_withdrawals_same_epoch() -> Result<()> {
    let temp = TempDir::new()?;
    let mut tracker = RewardsTracker::open(temp.path(), 100)?;

    let stake_key = vec![1, 2, 3, 4];
    let epoch = 105;

    // Record 3 withdrawals in same epoch
    tracker.record_withdrawal(&stake_key, epoch, 1000, 2_000_000, vec![1])?;
    tracker.record_withdrawal(&stake_key, epoch, 1001, 3_000_000, vec![2])?;
    tracker.record_withdrawal(&stake_key, epoch, 1002, 5_000_000, vec![3])?;

    // Total should be sum of all withdrawals
    let total = tracker.get_epoch_withdrawals(&stake_key, epoch)?;
    assert_eq!(total, 10_000_000); // 2 + 3 + 5 = 10 ADA

    Ok(())
}

// Test 4: Withdrawals in different epochs
#[test]
fn test_withdrawals_different_epochs() -> Result<()> {
    let temp = TempDir::new()?;
    let mut tracker = RewardsTracker::open(temp.path(), 100)?;

    let stake_key = vec![1, 2, 3, 4];

    // Record withdrawals in different epochs
    tracker.record_withdrawal(&stake_key, 105, 1000, 5_000_000, vec![1])?;
    tracker.record_withdrawal(&stake_key, 106, 2000, 3_000_000, vec![2])?;
    tracker.record_withdrawal(&stake_key, 107, 3000, 7_000_000, vec![3])?;

    // Verify each epoch separately
    assert_eq!(tracker.get_epoch_withdrawals(&stake_key, 105)?, 5_000_000);
    assert_eq!(tracker.get_epoch_withdrawals(&stake_key, 106)?, 3_000_000);
    assert_eq!(tracker.get_epoch_withdrawals(&stake_key, 107)?, 7_000_000);

    Ok(())
}

// Test 5: Reward snapshot storage and retrieval
#[test]
fn test_reward_snapshots() -> Result<()> {
    let temp = TempDir::new()?;
    let mut tracker = RewardsTracker::open(temp.path(), 100)?;

    let stake_key = vec![1, 2, 3, 4];
    let epoch = 105;
    let balance = 50_000_000u64; // 50 ADA
    let pool_id = Some(vec![5, 6, 7, 8]);

    // Store reward snapshot
    tracker.snapshot_rewards(&stake_key, epoch, balance, pool_id.clone())?;

    // Retrieve snapshot
    let snapshot = tracker.get_snapshot(&stake_key, epoch)?;
    assert!(snapshot.is_some());

    let snap = snapshot.unwrap();
    assert_eq!(snap.epoch, epoch);
    assert_eq!(snap.balance, balance);
    assert_eq!(snap.pool_id, pool_id);

    Ok(())
}

// Test 6: Reward snapshots across multiple epochs
#[test]
fn test_reward_snapshots_multiple_epochs() -> Result<()> {
    let temp = TempDir::new()?;
    let mut tracker = RewardsTracker::open(temp.path(), 100)?;

    let stake_key = vec![1, 2, 3, 4];
    let pool_id = Some(vec![5, 6, 7, 8]);

    // Store snapshots for 5 epochs
    for epoch in 100..105 {
        let balance = (epoch - 99) * 10_000_000; // Growing balance
        tracker.snapshot_rewards(&stake_key, epoch, balance, pool_id.clone())?;
    }

    // Verify all snapshots
    for epoch in 100..105 {
        let snapshot = tracker.get_snapshot(&stake_key, epoch)?;
        assert!(snapshot.is_some());
        assert_eq!(snapshot.unwrap().balance, (epoch - 99) * 10_000_000);
    }

    Ok(())
}

// Test 7: Calculate epoch reward (no withdrawals)
#[test]
fn test_calculate_epoch_reward_no_withdrawals() -> Result<()> {
    let temp = TempDir::new()?;
    let mut tracker = RewardsTracker::open(temp.path(), 100)?;

    let stake_key = vec![1, 2, 3, 4];

    // Epoch 100: 10 ADA
    tracker.snapshot_rewards(&stake_key, 100, 10_000_000, None)?;

    // Epoch 101: 12 ADA (earned 2 ADA)
    tracker.snapshot_rewards(&stake_key, 101, 12_000_000, None)?;

    // Calculate reward earned in epoch 101
    let earned = tracker.calculate_epoch_reward(&stake_key, 101)?;
    assert_eq!(earned, Some(2_000_000)); // 2 ADA

    Ok(())
}

// Test 8: Calculate epoch reward with withdrawals
#[test]
fn test_calculate_epoch_reward_with_withdrawals() -> Result<()> {
    let temp = TempDir::new()?;
    let mut tracker = RewardsTracker::open(temp.path(), 100)?;

    let stake_key = vec![1, 2, 3, 4];

    // Epoch 100: 10 ADA
    tracker.snapshot_rewards(&stake_key, 100, 10_000_000, None)?;

    // During epoch 101: earned 3 ADA but withdrew 2 ADA
    // Epoch 101: 11 ADA (10 + 3 - 2)
    tracker.snapshot_rewards(&stake_key, 101, 11_000_000, None)?;

    // Record withdrawal
    tracker.record_withdrawal(&stake_key, 101, 1000, 2_000_000, vec![1])?;

    // Calculate reward: current (11) + withdrawn (2) - previous (10) = 3 ADA
    let earned = tracker.calculate_epoch_reward(&stake_key, 101)?;
    assert_eq!(earned, Some(3_000_000));

    Ok(())
}

// Test 9: Lifetime rewards calculation
#[test]
fn test_lifetime_rewards() -> Result<()> {
    let temp = TempDir::new()?;
    let mut tracker = RewardsTracker::open(temp.path(), 100)?;

    let stake_key = vec![1, 2, 3, 4];

    // Current balance: 50 ADA
    let current_balance = 50_000_000u64;

    // Historical withdrawals across multiple epochs
    tracker.record_withdrawal(&stake_key, 100, 1000, 10_000_000, vec![1])?;
    tracker.record_withdrawal(&stake_key, 101, 2000, 15_000_000, vec![2])?;
    tracker.record_withdrawal(&stake_key, 102, 3000, 20_000_000, vec![3])?;

    // Lifetime = current + all withdrawals = 50 + 45 = 95 ADA
    let lifetime = tracker.get_lifetime_rewards(&stake_key, current_balance)?;
    assert_eq!(lifetime, 95_000_000);

    Ok(())
}

// Test 10: Delegation tracking
#[test]
fn test_delegation_tracking() -> Result<()> {
    let temp = TempDir::new()?;
    let mut tracker = RewardsTracker::open(temp.path(), 100)?;

    let stake_key = vec![1, 2, 3, 4];
    let pool_id1 = vec![10, 11, 12, 13];
    let pool_id2 = vec![20, 21, 22, 23];

    // Initial delegation at slot 1000
    tracker.record_delegation(&stake_key, &pool_id1, 1000)?;

    // Change delegation at slot 2000
    tracker.record_delegation(&stake_key, &pool_id2, 2000)?;

    // Get current delegation (should be latest)
    let current = tracker.get_current_delegation(&stake_key)?;
    assert!(current.is_some());
    assert_eq!(current.unwrap(), pool_id2); // Latest delegation

    Ok(())
}

// Test 11: Multiple stake keys
#[test]
fn test_multiple_stake_keys() -> Result<()> {
    let temp = TempDir::new()?;
    let mut tracker = RewardsTracker::open(temp.path(), 100)?;

    let stake_key1 = vec![1, 1, 1, 1];
    let stake_key2 = vec![2, 2, 2, 2];
    let stake_key3 = vec![3, 3, 3, 3];

    let epoch = 105;

    // Store snapshots for different stake keys
    tracker.snapshot_rewards(&stake_key1, epoch, 10_000_000, None)?;
    tracker.snapshot_rewards(&stake_key2, epoch, 20_000_000, None)?;
    tracker.snapshot_rewards(&stake_key3, epoch, 30_000_000, None)?;

    // Verify each stake key independently
    assert_eq!(tracker.get_snapshot(&stake_key1, epoch)?.unwrap().balance, 10_000_000);
    assert_eq!(tracker.get_snapshot(&stake_key2, epoch)?.unwrap().balance, 20_000_000);
    assert_eq!(tracker.get_snapshot(&stake_key3, epoch)?.unwrap().balance, 30_000_000);

    Ok(())
}

// Test 12: Snapshot before indexing start epoch
#[test]
fn test_snapshot_before_indexing_start() -> Result<()> {
    let temp = TempDir::new()?;
    let tracker = RewardsTracker::open(temp.path(), 100)?;

    let stake_key = vec![1, 2, 3, 4];

    // Try to get snapshot before we started indexing (epoch 99)
    let snapshot = tracker.get_snapshot(&stake_key, 99)?;
    assert!(snapshot.is_none()); // Should return None (data not available)

    Ok(())
}

// Test 13: Snapshot save and restore
#[test]
fn test_rewards_tracker_snapshot_save() -> Result<()> {
    let temp = TempDir::new()?;
    let mut tracker = RewardsTracker::open(temp.path(), 100)?;

    let stake_key = vec![1, 2, 3, 4];

    // Store some data
    tracker.snapshot_rewards(&stake_key, 105, 50_000_000, None)?;
    tracker.record_withdrawal(&stake_key, 105, 1000, 5_000_000, vec![1])?;

    // Save snapshot at slot 1000
    tracker.save_snapshot(1000)?;

    // Verify data is still accessible
    let snapshot = tracker.get_snapshot(&stake_key, 105)?;
    assert!(snapshot.is_some());
    assert_eq!(snapshot.unwrap().balance, 50_000_000);

    let withdrawn = tracker.get_epoch_withdrawals(&stake_key, 105)?;
    assert_eq!(withdrawn, 5_000_000);

    Ok(())
}
