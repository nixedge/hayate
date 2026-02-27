// Comprehensive tests for rewards tracking (cardano-wallet pattern)

use hayate::rewards::RewardsTracker;
use tempfile::TempDir;

// ===== Basic Functionality =====

#[test]
fn test_rewards_tracker_creation() {
    let temp = TempDir::new().unwrap();
    let tracker = RewardsTracker::open(temp.path(), 100).unwrap();
    
    assert_eq!(tracker.indexing_start_epoch, 100);
}

#[test]
fn test_snapshot_storage_and_retrieval() {
    let temp = TempDir::new().unwrap();
    let mut tracker = RewardsTracker::open(temp.path(), 0).unwrap();
    
    let stake_key = vec![0xaa; 28];
    let epoch = 150;
    let balance = 25_000_000;
    let pool_id = Some(vec![0xbb; 28]);
    
    // Store snapshot
    tracker.snapshot_rewards(&stake_key, epoch, balance, pool_id.clone()).unwrap();
    
    // Retrieve snapshot
    let snapshot = tracker.get_snapshot(&stake_key, epoch).unwrap().unwrap();
    
    assert_eq!(snapshot.epoch, epoch);
    assert_eq!(snapshot.balance, balance);
    assert_eq!(snapshot.pool_id, pool_id);
}

#[test]
fn test_missing_snapshot_returns_none() {
    let temp = TempDir::new().unwrap();
    let tracker = RewardsTracker::open(temp.path(), 100).unwrap();
    
    let stake_key = vec![0xaa; 28];
    
    // Query non-existent snapshot
    let result = tracker.get_snapshot(&stake_key, 150).unwrap();
    assert!(result.is_none());
}

#[test]
fn test_snapshot_before_indexing_start() {
    let temp = TempDir::new().unwrap();
    let tracker = RewardsTracker::open(temp.path(), 100).unwrap();
    
    let stake_key = vec![0xaa; 28];
    
    // Query epoch before we started indexing
    let result = tracker.get_snapshot(&stake_key, 50).unwrap();
    assert!(result.is_none());  // Should return None - unavailable
}

// ===== Withdrawal Tracking =====

#[test]
fn test_withdrawal_recording() {
    let temp = TempDir::new().unwrap();
    let mut tracker = RewardsTracker::open(temp.path(), 0).unwrap();
    
    let stake_key = vec![0xaa; 28];
    let epoch = 100;
    let slot = 43_200_000;
    let amount = 10_000_000;
    let tx_hash = vec![0xcc; 32];
    
    tracker.record_withdrawal(&stake_key, epoch, slot, amount, tx_hash).unwrap();
    
    // Verify
    let total = tracker.get_epoch_withdrawals(&stake_key, epoch).unwrap();
    assert_eq!(total, amount);
}

#[test]
fn test_multiple_withdrawals_same_epoch() {
    let temp = TempDir::new().unwrap();
    let mut tracker = RewardsTracker::open(temp.path(), 0).unwrap();
    
    let stake_key = vec![0xaa; 28];
    let epoch = 100;
    
    // Three withdrawals
    tracker.record_withdrawal(&stake_key, epoch, 1000, 5_000_000, vec![0x11; 32]).unwrap();
    tracker.record_withdrawal(&stake_key, epoch, 2000, 3_000_000, vec![0x22; 32]).unwrap();
    tracker.record_withdrawal(&stake_key, epoch, 3000, 2_000_000, vec![0x33; 32]).unwrap();
    
    // Total: 10 ADA
    let total = tracker.get_epoch_withdrawals(&stake_key, epoch).unwrap();
    assert_eq!(total, 10_000_000);
}

#[test]
fn test_withdrawals_different_epochs() {
    let temp = TempDir::new().unwrap();
    let mut tracker = RewardsTracker::open(temp.path(), 0).unwrap();
    
    let stake_key = vec![0xaa; 28];
    
    // Withdraw in epoch 100
    tracker.record_withdrawal(&stake_key, 100, 1000, 5_000_000, vec![0x11; 32]).unwrap();
    
    // Withdraw in epoch 101
    tracker.record_withdrawal(&stake_key, 101, 2000, 3_000_000, vec![0x22; 32]).unwrap();
    
    // Check each epoch separately
    assert_eq!(tracker.get_epoch_withdrawals(&stake_key, 100).unwrap(), 5_000_000);
    assert_eq!(tracker.get_epoch_withdrawals(&stake_key, 101).unwrap(), 3_000_000);
    assert_eq!(tracker.get_epoch_withdrawals(&stake_key, 102).unwrap(), 0);
}

// ===== Delegation Tracking =====

#[test]
fn test_delegation_recording() {
    let temp = TempDir::new().unwrap();
    let mut tracker = RewardsTracker::open(temp.path(), 0).unwrap();
    
    let stake_key = vec![0xaa; 28];
    let pool_id = vec![0xbb; 28];
    let slot = 1000;
    
    tracker.record_delegation(&stake_key, &pool_id, slot).unwrap();
    
    let current = tracker.get_current_delegation(&stake_key).unwrap();
    assert_eq!(current, Some(pool_id));
}

#[test]
fn test_delegation_history() {
    let temp = TempDir::new().unwrap();
    let mut tracker = RewardsTracker::open(temp.path(), 0).unwrap();
    
    let stake_key = vec![0xaa; 28];
    let pool1 = vec![0x11; 28];
    let pool2 = vec![0x22; 28];
    let pool3 = vec![0x33; 28];
    
    // Delegation chain
    tracker.record_delegation(&stake_key, &pool1, 1000).unwrap();
    tracker.record_delegation(&stake_key, &pool2, 2000).unwrap();
    tracker.record_delegation(&stake_key, &pool3, 3000).unwrap();
    
    // Should return most recent (pool3)
    let current = tracker.get_current_delegation(&stake_key).unwrap();
    assert_eq!(current, Some(pool3));
}

// ===== Epoch Reward Calculation =====

#[test]
fn test_simple_epoch_reward_calculation() {
    let temp = TempDir::new().unwrap();
    let mut tracker = RewardsTracker::open(temp.path(), 0).unwrap();
    
    let stake_key = vec![0xaa; 28];
    
    // Epoch 100: 10 ADA
    tracker.snapshot_rewards(&stake_key, 100, 10_000_000, None).unwrap();
    
    // Epoch 101: 15 ADA (earned 5 ADA)
    tracker.snapshot_rewards(&stake_key, 101, 15_000_000, None).unwrap();
    
    let earned = tracker.calculate_epoch_reward(&stake_key, 101).unwrap().unwrap();
    assert_eq!(earned, 5_000_000);
}

#[test]
fn test_epoch_reward_with_withdrawal() {
    let temp = TempDir::new().unwrap();
    let mut tracker = RewardsTracker::open(temp.path(), 0).unwrap();
    
    let stake_key = vec![0xaa; 28];
    
    // Epoch 100: 20 ADA
    tracker.snapshot_rewards(&stake_key, 100, 20_000_000, None).unwrap();
    
    // Epoch 101: Withdraw 15 ADA
    tracker.record_withdrawal(&stake_key, 101, 1000, 15_000_000, vec![0xcc; 32]).unwrap();
    
    // Epoch 101: 10 ADA left + 5 ADA earned = 15 ADA total earned
    tracker.snapshot_rewards(&stake_key, 101, 10_000_000, None).unwrap();
    
    // Earned = (10 + 15) - 20 = 5 ADA
    let earned = tracker.calculate_epoch_reward(&stake_key, 101).unwrap().unwrap();
    assert_eq!(earned, 5_000_000);
}

#[test]
fn test_lifetime_rewards_calculation() {
    let temp = TempDir::new().unwrap();
    let mut tracker = RewardsTracker::open(temp.path(), 0).unwrap();
    
    let stake_key = vec![0xaa; 28];
    let current_balance = 5_000_000;  // 5 ADA currently
    
    // Historical withdrawals
    tracker.record_withdrawal(&stake_key, 100, 1000, 20_000_000, vec![0x11; 32]).unwrap();
    tracker.record_withdrawal(&stake_key, 105, 2000, 15_000_000, vec![0x22; 32]).unwrap();
    tracker.record_withdrawal(&stake_key, 110, 3000, 10_000_000, vec![0x33; 32]).unwrap();
    
    // Lifetime = current + all withdrawals
    let lifetime = tracker.get_lifetime_rewards(&stake_key, current_balance).unwrap();
    assert_eq!(lifetime, 50_000_000);  // 5 + 20 + 15 + 10
}

// ===== Multi-Wallet Scenarios =====

#[test]
fn test_multiple_stake_keys() {
    let temp = TempDir::new().unwrap();
    let mut tracker = RewardsTracker::open(temp.path(), 0).unwrap();
    
    let stake_key1 = vec![0x11; 28];
    let stake_key2 = vec![0x22; 28];
    let stake_key3 = vec![0x33; 28];
    
    // Different balances for each
    tracker.snapshot_rewards(&stake_key1, 100, 10_000_000, None).unwrap();
    tracker.snapshot_rewards(&stake_key2, 100, 20_000_000, None).unwrap();
    tracker.snapshot_rewards(&stake_key3, 100, 30_000_000, None).unwrap();
    
    // Verify each independently
    assert_eq!(tracker.get_snapshot(&stake_key1, 100).unwrap().unwrap().balance, 10_000_000);
    assert_eq!(tracker.get_snapshot(&stake_key2, 100).unwrap().unwrap().balance, 20_000_000);
    assert_eq!(tracker.get_snapshot(&stake_key3, 100).unwrap().unwrap().balance, 30_000_000);
}

// ===== Regression Tests =====

#[test]
fn test_no_rewards_regression() {
    let temp = TempDir::new().unwrap();
    let mut tracker = RewardsTracker::open(temp.path(), 0).unwrap();
    
    let stake_key = vec![0xaa; 28];
    
    // Epoch 100: 10 ADA
    tracker.snapshot_rewards(&stake_key, 100, 10_000_000, None).unwrap();
    
    // Epoch 101: Still 10 ADA (no rewards earned, no withdrawal)
    tracker.snapshot_rewards(&stake_key, 101, 10_000_000, None).unwrap();
    
    // Should show 0 earned
    let earned = tracker.calculate_epoch_reward(&stake_key, 101).unwrap().unwrap();
    assert_eq!(earned, 0);
}

#[test]
fn test_partial_withdrawal() {
    let temp = TempDir::new().unwrap();
    let mut tracker = RewardsTracker::open(temp.path(), 0).unwrap();
    
    let stake_key = vec![0xaa; 28];
    
    // Epoch 100: 20 ADA
    tracker.snapshot_rewards(&stake_key, 100, 20_000_000, None).unwrap();
    
    // Epoch 101: Withdraw 5 ADA, earn 3 ADA
    tracker.record_withdrawal(&stake_key, 101, 1000, 5_000_000, vec![0xcc; 32]).unwrap();
    tracker.snapshot_rewards(&stake_key, 101, 18_000_000, None).unwrap();  // 20 - 5 + 3
    
    // Earned = (18 + 5) - 20 = 3 ADA
    let earned = tracker.calculate_epoch_reward(&stake_key, 101).unwrap().unwrap();
    assert_eq!(earned, 3_000_000);
}
