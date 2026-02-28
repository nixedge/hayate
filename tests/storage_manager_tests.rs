// Storage Manager Integration Tests
// Tests for src/indexer/storage_manager.rs (777 LOC)
// Target: 90% coverage, 15 tests

mod common;

use common::*;
use anyhow::Result;

// Test 1: Basic chain tip storage and retrieval
#[tokio::test]
async fn test_store_and_retrieve_chain_tip() -> Result<()> {
    let (_temp, handle) = create_test_storage(Network::Preview).await?;

    // Store chain tip
    let slot = 1000u64;
    let hash = vec![1, 2, 3, 4];
    let timestamp = 1234567890u64;

    handle.store_chain_tip(slot, hash.clone(), timestamp).await?;

    // Retrieve chain tip
    let chain_tip = handle.get_chain_tip().await?;
    assert!(chain_tip.is_some());

    let tip = chain_tip.unwrap();
    assert_eq!(tip.slot, slot);
    assert_eq!(tip.hash, hash);
    assert_eq!(tip.timestamp, timestamp);

    Ok(())
}

// Test 2: Wallet tip storage and retrieval
#[tokio::test]
async fn test_wallet_tip_operations() -> Result<()> {
    let (_temp, handle) = create_test_storage(Network::Preview).await?;

    let wallet_id = "wallet1".to_string();
    let slot = 2000u64;
    let hash = vec![5, 6, 7, 8];
    let timestamp = 1234567900u64;

    // Store wallet tip
    handle.store_wallet_tip(wallet_id.clone(), slot, hash.clone(), timestamp).await?;

    // Retrieve wallet tip
    let wallet_tip = handle.get_wallet_tip(wallet_id.clone()).await?;
    assert!(wallet_tip.is_some());

    let tip = wallet_tip.unwrap();
    assert_eq!(tip.slot, slot);
    assert_eq!(tip.hash, hash);

    Ok(())
}

// Test 3: Multiple wallet tips - get minimum tip
#[tokio::test]
async fn test_get_min_wallet_tip() -> Result<()> {
    let (_temp, handle) = create_test_storage(Network::Preview).await?;

    // Store multiple wallet tips at different slots
    handle.store_wallet_tip("wallet1".to_string(), 1000, vec![1], 100).await?;
    handle.store_wallet_tip("wallet2".to_string(), 1500, vec![2], 150).await?;
    handle.store_wallet_tip("wallet3".to_string(), 800, vec![3], 80).await?; // Minimum

    // Get minimum wallet tip
    let wallet_ids = vec!["wallet1".to_string(), "wallet2".to_string(), "wallet3".to_string()];
    let min_tip = handle.get_min_wallet_tip(wallet_ids).await?;

    assert!(min_tip.is_some());
    let tip = min_tip.unwrap();
    assert_eq!(tip.slot, 800); // wallet3 has the minimum slot
    assert_eq!(tip.hash, vec![3]);

    Ok(())
}

// Test 4: UTxO insertion and retrieval
#[tokio::test]
async fn test_utxo_operations() -> Result<()> {
    let (_temp, handle) = create_test_storage(Network::Preview).await?;

    let utxo_key = generate_mock_utxo_key(0, 0);
    let address = generate_mock_address(Network::Preview, 1);
    let utxo_data = generate_mock_utxo_data(&address, 10_000_000);

    // Insert UTxO
    handle.insert_utxo(utxo_key.clone(), utxo_data.clone()).await?;

    // Retrieve UTxO
    let retrieved = handle.get_utxo(utxo_key.clone()).await?;
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap(), utxo_data);

    // Delete UTxO
    handle.delete_utxo(utxo_key.clone()).await?;

    // Verify deletion
    let after_delete = handle.get_utxo(utxo_key).await?;
    assert!(after_delete.is_none());

    Ok(())
}

// Test 5: Address indexing - UTxO tracking
#[tokio::test]
async fn test_address_utxo_indexing() -> Result<()> {
    let (_temp, handle) = create_test_storage(Network::Preview).await?;

    let address = generate_mock_address(Network::Preview, 1);
    let utxo_key1 = generate_mock_utxo_key(0, 0);
    let utxo_key2 = generate_mock_utxo_key(1, 0);

    // Add UTxOs to address index
    handle.add_utxo_to_address_index(address.clone(), utxo_key1.clone()).await?;
    handle.add_utxo_to_address_index(address.clone(), utxo_key2.clone()).await?;

    // Get UTxOs for address
    let utxos = handle.get_utxos_for_address(address.clone()).await?;
    assert_eq!(utxos.len(), 2);
    assert!(utxos.contains(&utxo_key1));
    assert!(utxos.contains(&utxo_key2));

    // Remove one UTxO
    handle.remove_utxo_from_address_index(address.clone(), utxo_key1.clone()).await?;

    // Verify removal
    let after_removal = handle.get_utxos_for_address(address).await?;
    assert_eq!(after_removal.len(), 1);
    assert!(after_removal.contains(&utxo_key2));

    Ok(())
}

// Test 6: Balance tracking
#[tokio::test]
async fn test_balance_operations() -> Result<()> {
    let (_temp, handle) = create_test_storage(Network::Preview).await?;

    let address = generate_mock_address(Network::Preview, 1);

    // Initial balance should be 0
    let initial_balance = handle.get_balance(address.clone()).await?;
    assert_eq!(initial_balance, 0);

    // Update balance
    handle.update_balance(address.clone(), 50_000_000).await?;

    // Verify update
    let updated_balance = handle.get_balance(address.clone()).await?;
    assert_eq!(updated_balance, 50_000_000);

    // Update again
    handle.update_balance(address.clone(), 75_000_000).await?;

    // Verify second update
    let final_balance = handle.get_balance(address).await?;
    assert_eq!(final_balance, 75_000_000);

    Ok(())
}

// Test 7: Transaction history tracking
#[tokio::test]
async fn test_transaction_history() -> Result<()> {
    let (_temp, handle) = create_test_storage(Network::Preview).await?;

    let address = generate_mock_address(Network::Preview, 1);
    let tx1 = generate_mock_tx_hash(0);
    let tx2 = generate_mock_tx_hash(1);
    let tx3 = generate_mock_tx_hash(2);

    // Add transactions to history
    handle.add_tx_to_address_history(address.clone(), tx1.clone()).await?;
    handle.add_tx_to_address_history(address.clone(), tx2.clone()).await?;
    handle.add_tx_to_address_history(address.clone(), tx3.clone()).await?;

    // Get transaction history
    let history = handle.get_tx_history_for_address(address).await?;
    assert_eq!(history.len(), 3);
    assert!(history.contains(&tx1));
    assert!(history.contains(&tx2));
    assert!(history.contains(&tx3));

    Ok(())
}

// Test 8: Policy indexing (multi-asset support)
#[tokio::test]
async fn test_policy_indexing() -> Result<()> {
    let (_temp, handle) = create_test_storage(Network::Preview).await?;

    let policy_id = format!("{:056x}", 123);
    let tx1 = generate_mock_tx_hash(0);
    let tx2 = generate_mock_tx_hash(1);

    // Add transactions to policy index
    handle.add_tx_to_policy_index(policy_id.clone(), tx1.clone()).await?;
    handle.add_tx_to_policy_index(policy_id.clone(), tx2.clone()).await?;

    // Get transactions for policy
    let txs = handle.get_txs_for_policy(policy_id).await?;
    assert_eq!(txs.len(), 2);
    assert!(txs.contains(&tx1));
    assert!(txs.contains(&tx2));

    Ok(())
}

// Test 9: Asset indexing (specific token tracking)
#[tokio::test]
async fn test_asset_indexing() -> Result<()> {
    let (_temp, handle) = create_test_storage(Network::Preview).await?;

    let policy_id = format!("{:056x}", 123);
    let asset_name = hex::encode("TOKEN1");
    let tx1 = generate_mock_tx_hash(0);

    // Add transaction to asset index
    handle.add_tx_to_asset_index(policy_id.clone(), asset_name.clone(), tx1.clone()).await?;

    // Get transactions for asset
    let txs = handle.get_txs_for_asset(policy_id, asset_name).await?;
    assert_eq!(txs.len(), 1);
    assert_eq!(txs[0], tx1);

    Ok(())
}

// Test 10: Spent UTxO tracking (for rollback support)
#[tokio::test]
async fn test_spent_utxo_tracking() -> Result<()> {
    let (_temp, handle) = create_test_storage(Network::Preview).await?;

    let utxo_key = generate_mock_utxo_key(0, 0);
    let spend_event = vec![1, 2, 3, 4]; // Mock spend event data

    // Insert spent UTxO record
    handle.insert_spent_utxo(utxo_key.clone(), spend_event.clone()).await?;

    // For rollback testing, we would retrieve and restore
    // (Note: get_spent_utxo not in StorageHandle, would need to add)

    // Delete spent UTxO record (after confirmed)
    handle.delete_spent_utxo(utxo_key).await?;

    Ok(())
}

// Test 11: Block event storage
#[tokio::test]
async fn test_block_event_operations() -> Result<()> {
    let (_temp, handle) = create_test_storage(Network::Preview).await?;

    let event_key = "event:1000".to_string();
    let event_data = vec![10, 20, 30, 40];

    // Insert block event
    handle.insert_block_event(event_key.clone(), event_data.clone()).await?;

    // Retrieve block event
    let retrieved = handle.get_block_event(event_key.clone()).await?;
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap(), event_data);

    // Delete block event
    handle.delete_block_event(event_key.clone()).await?;

    // Verify deletion
    let after_delete = handle.get_block_event(event_key).await?;
    assert!(after_delete.is_none());

    Ok(())
}

// Test 12: Block metadata storage
#[tokio::test]
async fn test_block_metadata() -> Result<()> {
    let (_temp, handle) = create_test_storage(Network::Preview).await?;

    let block_hash = vec![1, 2, 3, 4, 5, 6, 7, 8];
    let slot = 1000u64;
    let timestamp = 1234567890u64;
    let prev_hash = Some(vec![0, 1, 2, 3, 4, 5, 6, 7]);

    // Store block metadata
    handle.store_block_metadata(block_hash.clone(), slot, timestamp, prev_hash.clone()).await?;

    // Retrieve block metadata
    let metadata = handle.get_block_metadata(block_hash.clone()).await?;
    assert!(metadata.is_some());

    let (retrieved_slot, retrieved_timestamp, retrieved_prev_hash) = metadata.unwrap();
    assert_eq!(retrieved_slot, slot);
    assert_eq!(retrieved_timestamp, timestamp);
    assert_eq!(retrieved_prev_hash, prev_hash);

    // Delete block metadata
    handle.delete_block_metadata(block_hash.clone()).await?;

    // Verify deletion
    let after_delete = handle.get_block_metadata(block_hash).await?;
    assert!(after_delete.is_none());

    Ok(())
}

// Test 13: Block event scanning by prefix (range queries)
#[tokio::test]
async fn test_scan_block_events_by_prefix() -> Result<()> {
    let (_temp, handle) = create_test_storage(Network::Preview).await?;

    // Insert multiple events with common prefix
    handle.insert_block_event("block:1000:event1".to_string(), vec![1]).await?;
    handle.insert_block_event("block:1000:event2".to_string(), vec![2]).await?;
    handle.insert_block_event("block:1000:event3".to_string(), vec![3]).await?;
    handle.insert_block_event("block:2000:event1".to_string(), vec![4]).await?;

    // Scan with prefix "block:1000"
    let events = handle.scan_block_events_by_prefix("block:1000".to_string()).await?;
    assert_eq!(events.len(), 3);

    // Verify event keys
    let keys: Vec<String> = events.iter().map(|(k, _)| k.clone()).collect();
    assert!(keys.contains(&"block:1000:event1".to_string()));
    assert!(keys.contains(&"block:1000:event2".to_string()));
    assert!(keys.contains(&"block:1000:event3".to_string()));

    Ok(())
}

// Test 14: Concurrent operations (stress test)
#[tokio::test]
async fn test_concurrent_operations() -> Result<()> {
    let (_temp, handle) = create_test_storage(Network::Preview).await?;

    // Spawn multiple concurrent operations
    let mut handles_vec = Vec::new();

    for i in 0..10 {
        let h = handle.clone();
        let task = tokio::spawn(async move {
            let address = generate_mock_address(Network::Preview, i);
            h.update_balance(address, i as u64 * 1_000_000).await
        });
        handles_vec.push(task);
    }

    // Wait for all operations to complete
    for task in handles_vec {
        task.await??;
    }

    // Verify all balances were stored correctly
    for i in 0..10 {
        let address = generate_mock_address(Network::Preview, i);
        let balance = handle.get_balance(address).await?;
        assert_eq!(balance, i as u64 * 1_000_000);
    }

    Ok(())
}

// Test 15: Snapshot operations
#[tokio::test]
async fn test_snapshot_operations() -> Result<()> {
    let (_temp, handle) = create_test_storage(Network::Preview).await?;

    // Store some data
    let slot = 5000u64;
    let hash = vec![9, 8, 7, 6];
    handle.store_chain_tip(slot, hash.clone(), 1234567890).await?;

    // Save snapshot
    handle.save_snapshots(slot).await?;

    // Verify data is still accessible after snapshot
    let chain_tip = handle.get_chain_tip().await?;
    assert!(chain_tip.is_some());
    let tip = chain_tip.unwrap();
    assert_eq!(tip.slot, slot);
    assert_eq!(tip.hash, hash);

    Ok(())
}
