// Comprehensive tests for block processor
// Tests UTxO tracking, delegations, withdrawals, governance

use hayate::indexer::{BlockProcessor, NetworkStorage, Network};
use hayate::rewards::RewardsTracker;
use tempfile::TempDir;
use cardano_lsm::Key;
use pallas_crypto::hash::Hash;

async fn create_test_processor() -> (BlockProcessor, TempDir) {
    let temp = TempDir::new().unwrap();
    let storage = NetworkStorage::open(temp.path().to_path_buf(), Network::Preprod).unwrap();
    let (manager, handle) = hayate::indexer::StorageManager::new(storage);
    tokio::spawn(async move { manager.run().await; });
    let system_start_ms = Network::Preprod.system_start_ms();
    let processor = BlockProcessor::new(handle, system_start_ms).await.unwrap();
    (processor, temp)
}

// ===== UTxO Processing Tests =====

#[tokio::test]
async fn test_utxo_creation() {
    let (mut processor, _temp) = create_test_processor().await;
    
    // Add an address to track
    let payment_key = Hash::<28>::new([1u8; 28]);
    let stake_key = Hash::<28>::new([2u8; 28]);
    processor.add_wallet(payment_key, stake_key);
    
    // Create mock transaction output
    // TODO: Create actual amaru_kernel types
    // For now, test the filter logic
    
    assert!(processor.filter.is_our_payment_key(&payment_key));
    assert!(processor.filter.is_our_stake_key(&stake_key));
}

#[tokio::test]
async fn test_utxo_spending() {
    let (mut processor, _temp) = create_test_processor().await;
    
    // 1. Create a UTxO
    let tx_hash = Hash::<32>::new([1u8; 32]);
    let utxo_key = format!("{}#0", hex::encode(&tx_hash));
    
    processor.storage.utxo_tree.insert(
        &Key::from(utxo_key.as_bytes()),
        &cardano_lsm::Value::from(b"test_utxo_data"),
    ).unwrap();
    
    // 2. Verify it exists
    assert!(processor.storage.utxo_tree.get(&Key::from(utxo_key.as_bytes())).unwrap().is_some());
    
    // 3. Spend it
    processor.storage.utxo_tree.delete(&Key::from(utxo_key.as_bytes())).unwrap();
    
    // 4. Verify it's gone
    assert!(processor.storage.utxo_tree.get(&Key::from(utxo_key.as_bytes())).unwrap().is_none());
}

#[tokio::test]
async fn test_balance_updates() {
    let (mut processor, _temp) = create_test_processor().await;
    
    let address = vec![1, 2, 3, 4];
    let balance_key = Key::from(&address);
    
    // Initial balance should be 0
    let initial = processor.storage.balance_tree.get(&balance_key).unwrap();
    assert_eq!(initial, 0);
    
    // Add 10 ADA
    processor.storage.balance_tree.insert(&balance_key, &10_000_000).unwrap();
    
    // Verify
    let balance = processor.storage.balance_tree.get(&balance_key).unwrap();
    assert_eq!(balance, 10_000_000);
    
    // Add 5 more ADA (monoidal!)
    let current = processor.storage.balance_tree.get(&balance_key).unwrap();
    processor.storage.balance_tree.insert(&balance_key, &(current + 5_000_000)).unwrap();
    
    // Should be 15 ADA total
    let final_balance = processor.storage.balance_tree.get(&balance_key).unwrap();
    assert_eq!(final_balance, 15_000_000);
}

// ===== Delegation Tests =====

#[tokio::test]
async fn test_delegation_tracking() {
    let (mut processor, _temp) = create_test_processor().await;
    
    let stake_key = vec![1, 2, 3, 4];
    let pool_id = vec![5, 6, 7, 8];
    let slot = 100;
    
    // Record delegation
    processor.storage.rewards_tracker.record_delegation(
        &stake_key,
        &pool_id,
        slot,
    ).unwrap();
    
    // Verify delegation is stored
    let current_delegation = processor.storage.rewards_tracker
        .get_current_delegation(&stake_key)
        .unwrap();
    
    assert_eq!(current_delegation, Some(pool_id));
}

#[tokio::test]
async fn test_delegation_changes() {
    let (mut processor, _temp) = create_test_processor().await;
    
    let stake_key = vec![1, 2, 3, 4];
    let pool1 = vec![5, 6, 7, 8];
    let pool2 = vec![9, 10, 11, 12];
    
    // Delegate to pool1 at slot 100
    processor.storage.rewards_tracker.record_delegation(&stake_key, &pool1, 100).unwrap();
    
    // Verify
    let delegation = processor.storage.rewards_tracker.get_current_delegation(&stake_key).unwrap();
    assert_eq!(delegation, Some(pool1.clone()));
    
    // Change to pool2 at slot 200
    processor.storage.rewards_tracker.record_delegation(&stake_key, &pool2, 200).unwrap();
    
    // Should now be pool2 (latest)
    let delegation = processor.storage.rewards_tracker.get_current_delegation(&stake_key).unwrap();
    assert_eq!(delegation, Some(pool2));
}

// ===== Withdrawal Tests =====

#[tokio::test]
async fn test_withdrawal_recording() {
    let (mut processor, _temp) = create_test_processor().await;
    
    let stake_key = vec![1, 2, 3, 4];
    let epoch = 100;
    let slot = 43_200_000;
    let amount = 5_000_000;
    let tx_hash = vec![0xaa; 32];
    
    // Record withdrawal
    processor.storage.rewards_tracker.record_withdrawal(
        &stake_key,
        epoch,
        slot,
        amount,
        tx_hash,
    ).unwrap();
    
    // Verify
    let total_withdrawn = processor.storage.rewards_tracker
        .get_epoch_withdrawals(&stake_key, epoch)
        .unwrap();
    
    assert_eq!(total_withdrawn, amount);
}

#[tokio::test]
async fn test_multiple_withdrawals() {
    let (mut processor, _temp) = create_test_processor().await;
    
    let stake_key = vec![1, 2, 3, 4];
    let epoch = 100;
    
    // Multiple withdrawals in same epoch
    processor.storage.rewards_tracker.record_withdrawal(
        &stake_key, epoch, 1000, 2_000_000, vec![0xaa; 32]
    ).unwrap();
    
    processor.storage.rewards_tracker.record_withdrawal(
        &stake_key, epoch, 2000, 3_000_000, vec![0xbb; 32]
    ).unwrap();
    
    // Total should be 5 ADA
    let total = processor.storage.rewards_tracker
        .get_epoch_withdrawals(&stake_key, epoch)
        .unwrap();
    
    assert_eq!(total, 5_000_000);
}

// ===== Reward Snapshot Tests =====

#[tokio::test]
async fn test_reward_snapshots() {
    let (mut processor, _temp) = create_test_processor().await;
    
    let stake_key = vec![1, 2, 3, 4];
    
    // Snapshot epoch 100: 10 ADA
    processor.storage.rewards_tracker.snapshot_rewards(
        &stake_key,
        100,
        10_000_000,
        None,
    ).unwrap();
    
    // Snapshot epoch 101: 12 ADA
    processor.storage.rewards_tracker.snapshot_rewards(
        &stake_key,
        101,
        12_000_000,
        None,
    ).unwrap();
    
    // Verify snapshots
    let snap100 = processor.storage.rewards_tracker
        .get_snapshot(&stake_key, 100)
        .unwrap()
        .unwrap();
    assert_eq!(snap100.balance, 10_000_000);
    
    let snap101 = processor.storage.rewards_tracker
        .get_snapshot(&stake_key, 101)
        .unwrap()
        .unwrap();
    assert_eq!(snap101.balance, 12_000_000);
}

#[tokio::test]
async fn test_epoch_reward_calculation() {
    let (mut processor, _temp) = create_test_processor().await;
    
    let stake_key = vec![1, 2, 3, 4];
    
    // Epoch 100: 10 ADA rewards
    processor.storage.rewards_tracker.snapshot_rewards(&stake_key, 100, 10_000_000, None).unwrap();
    
    // Epoch 101: 15 ADA rewards (earned 5 ADA)
    processor.storage.rewards_tracker.snapshot_rewards(&stake_key, 101, 15_000_000, None).unwrap();
    
    // Calculate earned
    let earned = processor.storage.rewards_tracker
        .calculate_epoch_reward(&stake_key, 101)
        .unwrap()
        .unwrap();
    
    assert_eq!(earned, 5_000_000);
}

#[tokio::test]
async fn test_reward_calculation_with_withdrawal() {
    let (mut processor, _temp) = create_test_processor().await;
    
    let stake_key = vec![1, 2, 3, 4];
    
    // Epoch 100: 10 ADA rewards
    processor.storage.rewards_tracker.snapshot_rewards(&stake_key, 100, 10_000_000, None).unwrap();
    
    // Epoch 101: Withdraw 6 ADA, leaving 4 ADA, but earned 8 ADA
    processor.storage.rewards_tracker.record_withdrawal(
        &stake_key, 101, 43_200_000, 6_000_000, vec![0xaa; 32]
    ).unwrap();
    
    // Snapshot after withdrawal
    processor.storage.rewards_tracker.snapshot_rewards(&stake_key, 101, 12_000_000, None).unwrap();
    
    // Calculate: (12 + 6) - 10 = 8 ADA earned
    let earned = processor.storage.rewards_tracker
        .calculate_epoch_reward(&stake_key, 101)
        .unwrap()
        .unwrap();
    
    assert_eq!(earned, 8_000_000);
}

#[tokio::test]
async fn test_lifetime_rewards() {
    let (mut processor, _temp) = create_test_processor().await;
    
    let stake_key = vec![1, 2, 3, 4];
    
    // Current balance: 5 ADA
    let current_balance = 5_000_000;
    
    // Withdrew 10 ADA in epoch 100
    processor.storage.rewards_tracker.record_withdrawal(
        &stake_key, 100, 1000, 10_000_000, vec![0xaa; 32]
    ).unwrap();
    
    // Withdrew 8 ADA in epoch 101
    processor.storage.rewards_tracker.record_withdrawal(
        &stake_key, 101, 2000, 8_000_000, vec![0xbb; 32]
    ).unwrap();
    
    // Lifetime = 5 (current) + 10 + 8 = 23 ADA
    let lifetime = processor.storage.rewards_tracker
        .get_lifetime_rewards(&stake_key, current_balance)
        .unwrap();
    
    assert_eq!(lifetime, 23_000_000);
}

// ===== Nonce Tests =====

#[tokio::test]
async fn test_nonce_storage() {
    let (mut processor, _temp) = create_test_processor().await;
    
    let epoch = 100;
    let nonce = [0xab; 32];
    
    // Store nonce
    processor.storage.store_nonce(epoch, &nonce).unwrap();
    
    // Retrieve nonce
    let stored_nonce = processor.storage.get_nonce(epoch).unwrap();
    assert_eq!(stored_nonce, Some(nonce.to_vec()));
}

#[tokio::test]
async fn test_nonce_per_epoch() {
    let (mut processor, _temp) = create_test_processor().await;
    
    // Store nonces for multiple epochs
    for epoch in 100..110 {
        let nonce = [epoch as u8; 32];
        processor.storage.store_nonce(epoch, &nonce).unwrap();
    }
    
    // Verify each epoch has correct nonce
    for epoch in 100..110 {
        let nonce = processor.storage.get_nonce(epoch).unwrap().unwrap();
        assert_eq!(nonce[0], epoch as u8);
    }
}

// ===== Epoch Boundary Tests =====

#[test]
fn test_epoch_calculation() {
    use hayate::indexer::block_processor::{slot_to_epoch, epoch_to_slot, is_epoch_boundary};
    
    assert_eq!(slot_to_epoch(0), 0);
    assert_eq!(slot_to_epoch(432_000), 1);
    assert_eq!(slot_to_epoch(864_000), 2);
    
    assert_eq!(epoch_to_slot(0), 0);
    assert_eq!(epoch_to_slot(1), 432_000);
    
    assert!(is_epoch_boundary(0));
    assert!(is_epoch_boundary(432_000));
    assert!(!is_epoch_boundary(1));
}

#[tokio::test]
async fn test_epoch_boundary_detection() {
    let (mut processor, _temp) = create_test_processor().await;
    
    // Simulate blocks across epoch boundary
    // Epoch 0: slots 0 - 431,999
    // Epoch 1: slots 432,000+
    
    processor.current_epoch = 0;
    
    // Process block at slot 431,999 (still epoch 0)
    let epoch = hayate::indexer::block_processor::slot_to_epoch(431_999);
    assert_eq!(epoch, 0);
    
    // Process block at slot 432,000 (epoch 1!)
    let epoch = hayate::indexer::block_processor::slot_to_epoch(432_000);
    assert_eq!(epoch, 1);
}

// ===== Wallet Filter Tests =====

#[test]
fn test_wallet_filter_payment_keys() {
    use hayate::indexer::block_processor::WalletFilter;
    
    let mut filter = WalletFilter::new();
    
    let key1 = Hash::<28>::new([1u8; 28]);
    let key2 = Hash::<28>::new([2u8; 28]);
    
    filter.add_payment_key_hash(key1);
    
    assert!(filter.is_our_payment_key(&key1));
    assert!(!filter.is_our_payment_key(&key2));
}

#[test]
fn test_wallet_filter_stake_keys() {
    use hayate::indexer::block_processor::WalletFilter;
    
    let mut filter = WalletFilter::new();
    
    let stake1 = Hash::<28>::new([1u8; 28]);
    let stake2 = Hash::<28>::new([2u8; 28]);
    
    filter.add_stake_credential(stake1);
    
    assert!(filter.is_our_stake_key(&stake1));
    assert!(!filter.is_our_stake_key(&stake2));
}

// ===== Multi-Wallet Tests =====

#[tokio::test]
async fn test_multiple_wallets() {
    let (mut processor, _temp) = create_test_processor().await;
    
    // Add 3 wallets
    for i in 0..3 {
        let payment = Hash::<28>::new([i; 28]);
        let stake = Hash::<28>::new([i + 10; 28]);
        processor.add_wallet(payment, stake);
    }
    
    // Verify all tracked
    for i in 0..3 {
        let payment = Hash::<28>::new([i; 28]);
        let stake = Hash::<28>::new([i + 10; 28]);
        
        assert!(processor.filter.is_our_payment_key(&payment));
        assert!(processor.filter.is_our_stake_key(&stake));
    }
}

// ===== Governance Tests =====

#[tokio::test]
async fn test_governance_proposal_storage() {
    let (mut processor, _temp) = create_test_processor().await;
    
    let tx_hash = Hash::<32>::new([1u8; 32]);
    let proposal_id = format!("{}#0", hex::encode(&tx_hash));
    let proposal_data = b"mock_proposal_data";
    
    // Store proposal
    let key = format!("proposal:{}", proposal_id);
    processor.storage.governance_tree.insert(
        &Key::from(key.as_bytes()),
        &cardano_lsm::Value::from(proposal_data),
    ).unwrap();
    
    // Verify stored
    let stored = processor.storage.governance_tree
        .get(&Key::from(key.as_bytes()))
        .unwrap()
        .unwrap();
    
    assert_eq!(stored.as_ref(), proposal_data);
}

#[tokio::test]
async fn test_governance_merkle_proof() {
    let (mut processor, _temp) = create_test_processor().await;
    
    // Insert governance action into Merkle tree
    let action_id = b"gov_action_1";
    let action_data = b"proposal_data";
    
    let proof = processor.storage.governance_merkle.insert(action_id, action_data);
    
    // Verify proof
    let verified = processor.storage.governance_merkle.verify(&proof);
    assert!(verified.is_ok());
}

// ===== Integration Tests =====

#[tokio::test]
async fn test_full_transaction_flow() {
    let (mut processor, _temp) = create_test_processor().await;
    
    let payment_key = Hash::<28>::new([1u8; 28]);
    let stake_key = Hash::<28>::new([2u8; 28]);
    processor.add_wallet(payment_key, stake_key);
    
    // Simulate transaction flow:
    // 1. Receive 10 ADA
    // 2. Delegate stake
    // 3. Earn rewards
    // 4. Withdraw rewards
    
    // Step 1: Create UTxO
    let tx1_hash = Hash::<32>::new([1u8; 32]);
    let utxo_key = format!("{}#0", hex::encode(&tx1_hash));
    
    processor.storage.utxo_tree.insert(
        &Key::from(utxo_key.as_bytes()),
        &cardano_lsm::Value::from(b"10000000"),  // 10 ADA
    ).unwrap();
    
    // Step 2: Delegate
    let pool_id = vec![5, 6, 7, 8];
    processor.storage.rewards_tracker.record_delegation(
        &stake_key.to_vec(),
        &pool_id,
        1000,
    ).unwrap();
    
    // Step 3: Snapshot rewards (simulate epoch reward)
    processor.storage.rewards_tracker.snapshot_rewards(
        &stake_key.to_vec(),
        100,
        2_000_000,  // 2 ADA rewards
        Some(pool_id.clone()),
    ).unwrap();
    
    // Step 4: Withdraw rewards
    processor.storage.rewards_tracker.record_withdrawal(
        &stake_key.to_vec(),
        101,
        43_200_000,
        2_000_000,
        vec![0xcc; 32],
    ).unwrap();
    
    // Verify state
    let utxo_exists = processor.storage.utxo_tree
        .get(&Key::from(utxo_key.as_bytes()))
        .unwrap()
        .is_some();
    assert!(utxo_exists);
    
    let current_delegation = processor.storage.rewards_tracker
        .get_current_delegation(&stake_key.to_vec())
        .unwrap();
    assert_eq!(current_delegation, Some(pool_id));
    
    let withdrawn = processor.storage.rewards_tracker
        .get_epoch_withdrawals(&stake_key.to_vec(), 101)
        .unwrap();
    assert_eq!(withdrawn, 2_000_000);
}

// ===== Edge Cases =====

#[tokio::test]
async fn test_rewards_before_indexing_started() {
    let temp = TempDir::new().unwrap();
    let start_epoch = 100;
    let rewards = RewardsTracker::open(temp.path(), start_epoch).unwrap();
    
    let stake_key = vec![1, 2, 3, 4];
    
    // Try to get snapshot from before we started
    let snapshot = rewards.get_snapshot(&stake_key, 50).unwrap();
    assert_eq!(snapshot, None);  // Should return None - data unavailable
}

#[tokio::test]
async fn test_concurrent_delegations() {
    let (mut processor, _temp) = create_test_processor().await;
    
    let stake_key = vec![1, 2, 3, 4];
    let pool1 = vec![0xaa; 28];
    let pool2 = vec![0xbb; 28];
    
    // Delegate to pool1 at slot 1000
    processor.storage.rewards_tracker.record_delegation(&stake_key, &pool1, 1000).unwrap();
    
    // Delegate to pool2 at slot 2000
    processor.storage.rewards_tracker.record_delegation(&stake_key, &pool2, 2000).unwrap();
    
    // Should use latest (pool2)
    let current = processor.storage.rewards_tracker
        .get_current_delegation(&stake_key)
        .unwrap();
    
    assert_eq!(current, Some(pool2));
}

#[tokio::test]
async fn test_empty_withdrawal_epoch() {
    let (processor, _temp) = create_test_processor().await;

    let stake_key = vec![1, 2, 3, 4];
    let epoch = 100;

    // No withdrawals in epoch
    let total = processor.storage.rewards_tracker
        .get_epoch_withdrawals(&stake_key, epoch)
        .unwrap();
    
    assert_eq!(total, 0);
}

// ===== Performance Tests =====

#[tokio::test]
async fn test_bulk_utxo_operations() {
    let (mut processor, _temp) = create_test_processor().await;
    
    // Create 1000 UTxOs
    for i in 0u32..1000 {
        let tx_hash = Hash::<32>::new([i as u8; 32]);
        let utxo_key = format!("{}#0", hex::encode(&tx_hash));
        
        processor.storage.utxo_tree.insert(
            &Key::from(utxo_key.as_bytes()),
            &cardano_lsm::Value::from(&i.to_le_bytes()),
        ).unwrap();
    }
    
    // Verify all exist
    for i in 0u32..1000 {
        let tx_hash = Hash::<32>::new([i as u8; 32]);
        let utxo_key = format!("{}#0", hex::encode(&tx_hash));
        
        assert!(processor.storage.utxo_tree
            .get(&Key::from(utxo_key.as_bytes()))
            .unwrap()
            .is_some()
        );
    }
}

#[tokio::test]
async fn test_bulk_reward_snapshots() {
    let (mut processor, _temp) = create_test_processor().await;
    
    let stake_key = vec![1, 2, 3, 4];
    
    // Snapshot 100 epochs
    for epoch in 0..100 {
        let balance = epoch * 1_000_000;  // Linear growth for testing
        processor.storage.rewards_tracker.snapshot_rewards(
            &stake_key,
            epoch,
            balance,
            None,
        ).unwrap();
    }
    
    // Verify all snapshots
    for epoch in 0..100 {
        let snapshot = processor.storage.rewards_tracker
            .get_snapshot(&stake_key, epoch)
            .unwrap()
            .unwrap();
        
        assert_eq!(snapshot.balance, epoch * 1_000_000);
    }
}
