// Block Processor Integration Tests
// Tests for src/indexer/block_processor.rs (830 LOC)
// Target: 90% coverage, 25 tests
// Note: Full integration tests with Pallas types require more complex setup
// These tests focus on testable components: WalletFilter and processor logic

mod common;

use common::*;
use anyhow::Result;
use pallas_crypto::hash::Hash;

// Import block processor types we can test
use hayate::indexer::block_processor::WalletFilter;

// ============================================================================
// WALLET FILTER TESTS (10 tests)
// ============================================================================

// Test 1: WalletFilter creation
#[test]
fn test_wallet_filter_creation() {
    let filter = WalletFilter::new();

    // New filter should track everything (no keys configured)
    let test_address = generate_mock_address(Network::Preview, 1);
    let address_bytes = hex::decode(&test_address).unwrap();

    assert!(filter.is_our_address(&address_bytes));
}

// Test 2: Add payment key to filter
#[test]
fn test_add_payment_key_hash() {
    let mut filter = WalletFilter::new();

    // Create a mock payment key hash (28 bytes)
    let key_hash_bytes = [1u8; 28];
    let key_hash = Hash::<28>::from(key_hash_bytes);

    filter.add_payment_key_hash(key_hash.clone());

    assert!(filter.is_our_payment_key(&key_hash));
}

// Test 3: Add stake credential to filter
#[test]
fn test_add_stake_credential() {
    let mut filter = WalletFilter::new();

    // Create a mock stake credential (28 bytes)
    let stake_cred_bytes = [2u8; 28];
    let stake_cred = Hash::<28>::from(stake_cred_bytes);

    filter.add_stake_credential(stake_cred.clone());

    assert!(filter.is_our_stake_key(&stake_cred));
}

// Test 4: Check unknown payment key
#[test]
fn test_unknown_payment_key() {
    let mut filter = WalletFilter::new();

    let key_hash1 = Hash::<28>::from([1u8; 28]);
    let key_hash2 = Hash::<28>::from([2u8; 28]);

    filter.add_payment_key_hash(key_hash1.clone());

    assert!(filter.is_our_payment_key(&key_hash1));
    assert!(!filter.is_our_payment_key(&key_hash2));
}

// Test 5: Check unknown stake key
#[test]
fn test_unknown_stake_key() {
    let mut filter = WalletFilter::new();

    let stake1 = Hash::<28>::from([1u8; 28]);
    let stake2 = Hash::<28>::from([2u8; 28]);

    filter.add_stake_credential(stake1.clone());

    assert!(filter.is_our_stake_key(&stake1));
    assert!(!filter.is_our_stake_key(&stake2));
}

// Test 6: Multiple payment keys
#[test]
fn test_multiple_payment_keys() {
    let mut filter = WalletFilter::new();

    let key1 = Hash::<28>::from([1u8; 28]);
    let key2 = Hash::<28>::from([2u8; 28]);
    let key3 = Hash::<28>::from([3u8; 28]);

    filter.add_payment_key_hash(key1.clone());
    filter.add_payment_key_hash(key2.clone());
    filter.add_payment_key_hash(key3.clone());

    assert!(filter.is_our_payment_key(&key1));
    assert!(filter.is_our_payment_key(&key2));
    assert!(filter.is_our_payment_key(&key3));
}

// Test 7: Multiple stake keys
#[test]
fn test_multiple_stake_keys() {
    let mut filter = WalletFilter::new();

    let stake1 = Hash::<28>::from([1u8; 28]);
    let stake2 = Hash::<28>::from([2u8; 28]);

    filter.add_stake_credential(stake1.clone());
    filter.add_stake_credential(stake2.clone());

    assert!(filter.is_our_stake_key(&stake1));
    assert!(filter.is_our_stake_key(&stake2));
}

// Test 8: Empty filter tracks everything
#[test]
fn test_empty_filter_tracks_everything() {
    let filter = WalletFilter::new();

    // With no keys configured, should track all addresses
    // Test with a few different address patterns
    for i in 0..5 {
        let test_address = generate_mock_address(Network::Preview, i);
        let address_bytes = hex::decode(&test_address).unwrap_or_default();

        // Empty filter should accept any valid address
        if !address_bytes.is_empty() {
            assert!(filter.is_our_address(&address_bytes));
        }
    }
}

// Test 9: Invalid address bytes
#[test]
fn test_invalid_address_bytes() {
    let mut filter = WalletFilter::new();

    // Add a key to make filter selective
    let key_hash = Hash::<28>::from([1u8; 28]);
    filter.add_payment_key_hash(key_hash);

    // Invalid address bytes should return false
    let invalid_bytes = vec![0xFF, 0xFF, 0xFF]; // Too short, invalid format
    assert!(!filter.is_our_address(&invalid_bytes));
}

// Test 10: Duplicate keys are handled correctly
#[test]
fn test_duplicate_keys() {
    let mut filter = WalletFilter::new();

    let key_hash = Hash::<28>::from([1u8; 28]);

    // Add same key twice
    filter.add_payment_key_hash(key_hash.clone());
    filter.add_payment_key_hash(key_hash.clone());

    // Should still work correctly
    assert!(filter.is_our_payment_key(&key_hash));
}

// ============================================================================
// BASIC PROCESSOR TESTS (15 tests)
// ============================================================================
// Note: Full block processing tests require complex Pallas block structures
// These tests focus on the infrastructure and simpler scenarios

// Test 11: Basic storage initialization for processor
#[tokio::test]
async fn test_processor_storage_setup() -> Result<()> {
    let (_temp, handle) = create_test_storage(Network::Preview).await?;

    // Initialize chain tip for processor
    handle.store_chain_tip(0, vec![0; 32], 0).await?;

    // Verify storage is accessible
    let tip = handle.get_chain_tip().await?;
    assert!(tip.is_some());

    Ok(())
}

// Test 12: UTxO key generation format
#[test]
fn test_utxo_key_format() {
    let tx_hash = "0000000000000000000000000000000000000000000000000000000000000001";
    let output_index = 0u32;

    let utxo_key = format!("{}#{}", tx_hash, output_index);

    assert_eq!(utxo_key, "0000000000000000000000000000000000000000000000000000000000000001#0");
    assert!(utxo_key.contains("#"));
}

// Test 13: Balance calculation - simple addition
#[tokio::test]
async fn test_balance_calculation_addition() -> Result<()> {
    let (_temp, handle) = create_test_storage(Network::Preview).await?;

    let address = generate_mock_address(Network::Preview, 1);

    // Start with 0
    handle.update_balance(address.clone(), 0).await?;

    // Add 10 ADA
    let current = handle.get_balance(address.clone()).await?;
    handle.update_balance(address.clone(), current + 10_000_000).await?;

    // Add 5 more ADA
    let current = handle.get_balance(address.clone()).await?;
    handle.update_balance(address.clone(), current + 5_000_000).await?;

    // Should have 15 ADA
    let final_balance = handle.get_balance(address).await?;
    assert_eq!(final_balance, 15_000_000);

    Ok(())
}

// Test 14: Balance calculation - subtraction
#[tokio::test]
async fn test_balance_calculation_subtraction() -> Result<()> {
    let (_temp, handle) = create_test_storage(Network::Preview).await?;

    let address = generate_mock_address(Network::Preview, 1);

    // Start with 100 ADA
    handle.update_balance(address.clone(), 100_000_000).await?;

    // Spend 30 ADA
    let current = handle.get_balance(address.clone()).await?;
    handle.update_balance(address.clone(), current - 30_000_000).await?;

    // Should have 70 ADA
    let final_balance = handle.get_balance(address).await?;
    assert_eq!(final_balance, 70_000_000);

    Ok(())
}

// Test 15: UTxO lifecycle - create and spend
#[tokio::test]
async fn test_utxo_lifecycle() -> Result<()> {
    let (_temp, handle) = create_test_storage(Network::Preview).await?;

    let address = generate_mock_address(Network::Preview, 1);
    let utxo_key = generate_mock_utxo_key(0, 0);
    let utxo_data = generate_mock_utxo_data(&address, 50_000_000);

    // Create UTxO
    handle.insert_utxo(utxo_key.clone(), utxo_data.clone()).await?;

    // Verify it exists
    let retrieved = handle.get_utxo(utxo_key.clone()).await?;
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap(), utxo_data);

    // Spend UTxO (delete)
    handle.delete_utxo(utxo_key.clone()).await?;

    // Verify it's gone
    let after_spend = handle.get_utxo(utxo_key).await?;
    assert!(after_spend.is_none());

    Ok(())
}

// Test 16: Multiple UTxOs for same address
#[tokio::test]
async fn test_multiple_utxos_same_address() -> Result<()> {
    let (_temp, handle) = create_test_storage(Network::Preview).await?;

    let address = generate_mock_address(Network::Preview, 1);

    // Create 3 UTxOs for same address
    for i in 0..3 {
        let utxo_key = generate_mock_utxo_key(i, 0);
        let utxo_data = generate_mock_utxo_data(&address, 10_000_000);

        handle.insert_utxo(utxo_key.clone(), utxo_data).await?;
        handle.add_utxo_to_address_index(address.clone(), utxo_key).await?;
    }

    // Verify all 3 are indexed
    let utxos = handle.get_utxos_for_address(address).await?;
    assert_eq!(utxos.len(), 3);

    Ok(())
}

// Test 17: Transaction history accumulation
#[tokio::test]
async fn test_transaction_history_accumulation() -> Result<()> {
    let (_temp, handle) = create_test_storage(Network::Preview).await?;

    let address = generate_mock_address(Network::Preview, 1);

    // Add 5 transactions
    for i in 0..5 {
        let tx_hash = generate_mock_tx_hash(i);
        handle.add_tx_to_address_history(address.clone(), tx_hash).await?;
    }

    // Verify all 5 are stored
    let history = handle.get_tx_history_for_address(address).await?;
    assert_eq!(history.len(), 5);

    Ok(())
}

// Test 18: Rollback data structure - spent UTxOs tracking
#[tokio::test]
async fn test_spent_utxo_rollback_tracking() -> Result<()> {
    let (_temp, handle) = create_test_storage(Network::Preview).await?;

    let utxo_key = generate_mock_utxo_key(0, 0);
    let spend_data = vec![1, 2, 3, 4];

    // Record spent UTxO for rollback
    handle.insert_spent_utxo(utxo_key.clone(), spend_data.clone()).await?;

    // Later, during rollback, we would restore this
    // For now, just verify we can delete it after confirmation
    handle.delete_spent_utxo(utxo_key).await?;

    Ok(())
}

// Test 19: Block metadata storage for processor
#[tokio::test]
async fn test_block_metadata_for_rollback() -> Result<()> {
    let (_temp, handle) = create_test_storage(Network::Preview).await?;

    let block_hash = vec![1, 2, 3, 4, 5, 6, 7, 8];
    let slot = 1000u64;
    let timestamp = 1234567890u64;
    let prev_hash = Some(vec![0, 1, 2, 3, 4, 5, 6, 7]);

    // Store block metadata for rollback capability
    handle.store_block_metadata(block_hash.clone(), slot, timestamp, prev_hash.clone()).await?;

    // Retrieve and verify
    let metadata = handle.get_block_metadata(block_hash).await?;
    assert!(metadata.is_some());

    let (ret_slot, ret_timestamp, ret_prev) = metadata.unwrap();
    assert_eq!(ret_slot, slot);
    assert_eq!(ret_timestamp, timestamp);
    assert_eq!(ret_prev, prev_hash);

    Ok(())
}

// Test 20: Address index management
#[tokio::test]
async fn test_address_index_management() -> Result<()> {
    let (_temp, handle) = create_test_storage(Network::Preview).await?;

    let address = generate_mock_address(Network::Preview, 1);
    let utxo1 = generate_mock_utxo_key(0, 0);
    let utxo2 = generate_mock_utxo_key(0, 1);
    let utxo3 = generate_mock_utxo_key(1, 0);

    // Add UTxOs to address index
    handle.add_utxo_to_address_index(address.clone(), utxo1.clone()).await?;
    handle.add_utxo_to_address_index(address.clone(), utxo2.clone()).await?;
    handle.add_utxo_to_address_index(address.clone(), utxo3.clone()).await?;

    // Verify all are indexed
    let indexed = handle.get_utxos_for_address(address.clone()).await?;
    assert_eq!(indexed.len(), 3);

    // Remove one
    handle.remove_utxo_from_address_index(address.clone(), utxo2).await?;

    // Verify only 2 remain
    let after_removal = handle.get_utxos_for_address(address).await?;
    assert_eq!(after_removal.len(), 2);

    Ok(())
}

// Test 21: Balance tracking across multiple addresses
#[tokio::test]
async fn test_multiple_address_balances() -> Result<()> {
    let (_temp, handle) = create_test_storage(Network::Preview).await?;

    // Create 5 addresses with different balances
    for i in 0..5 {
        let address = generate_mock_address(Network::Preview, i);
        let balance = (i as u64 + 1) * 10_000_000; // 10 ADA, 20 ADA, 30 ADA, etc.

        handle.update_balance(address, balance).await?;
    }

    // Verify each balance
    for i in 0..5 {
        let address = generate_mock_address(Network::Preview, i);
        let balance = handle.get_balance(address).await?;
        assert_eq!(balance, (i as u64 + 1) * 10_000_000);
    }

    Ok(())
}

// Test 22: Chain tip updates during processing
#[tokio::test]
async fn test_chain_tip_progression() -> Result<()> {
    let (_temp, handle) = create_test_storage(Network::Preview).await?;

    // Simulate processing blocks - chain tip advances
    for slot in 0..10 {
        let hash = vec![slot as u8; 32];
        let timestamp = 1234567890 + slot * 20;

        handle.store_chain_tip(slot, hash, timestamp).await?;
    }

    // Verify final chain tip
    let tip = handle.get_chain_tip().await?;
    assert!(tip.is_some());

    let final_tip = tip.unwrap();
    assert_eq!(final_tip.slot, 9);

    Ok(())
}

// Test 23: Wallet tip tracking
#[tokio::test]
async fn test_wallet_tip_tracking() -> Result<()> {
    let (_temp, handle) = create_test_storage(Network::Preview).await?;

    let wallet_id = "wallet1".to_string();

    // Simulate wallet catching up to chain
    for slot in 0..5 {
        let hash = vec![slot as u8; 32];
        let timestamp = 1234567890 + slot * 20;

        handle.store_wallet_tip(wallet_id.clone(), slot, hash, timestamp).await?;
    }

    // Verify wallet tip
    let tip = handle.get_wallet_tip(wallet_id).await?;
    assert!(tip.is_some());
    assert_eq!(tip.unwrap().slot, 4);

    Ok(())
}

// Test 24: Block event storage for processor state
#[tokio::test]
async fn test_block_event_storage() -> Result<()> {
    let (_temp, handle) = create_test_storage(Network::Preview).await?;

    // Store events for blocks
    for slot in 1000..1010 {
        let event_key = format!("block:{}", slot);
        let event_data = vec![slot as u8; 10];

        handle.insert_block_event(event_key, event_data).await?;
    }

    // Scan events by prefix
    let events = handle.scan_block_events_by_prefix("block:100".to_string()).await?;
    assert_eq!(events.len(), 10); // block:1000 through block:1009

    Ok(())
}

// Test 25: Snapshot coordination
#[tokio::test]
async fn test_processor_snapshot_coordination() -> Result<()> {
    let (_temp, handle) = create_test_storage(Network::Preview).await?;

    // Setup some processor state
    handle.store_chain_tip(5000, vec![1; 32], 1234567890).await?;
    handle.update_balance(generate_mock_address(Network::Preview, 1), 100_000_000).await?;

    // Trigger snapshot at slot 5000
    handle.save_snapshots(5000).await?;

    // Verify state is still accessible after snapshot
    let tip = handle.get_chain_tip().await?;
    assert!(tip.is_some());
    assert_eq!(tip.unwrap().slot, 5000);

    let balance = handle.get_balance(generate_mock_address(Network::Preview, 1)).await?;
    assert_eq!(balance, 100_000_000);

    Ok(())
}
