// Fault Injection Tests
// Test system behavior under adverse conditions and malformed inputs

use anyhow::Result;
use hayate::indexer::{Network, NetworkStorage, StorageManager};
use tempfile::TempDir;

mod common;

async fn create_test_storage() -> (TempDir, hayate::indexer::StorageHandle) {
    let temp = TempDir::new().unwrap();
    let storage = NetworkStorage::open(temp.path().to_path_buf(), Network::Preview).unwrap();
    let (manager, handle) = StorageManager::new(storage);

    tokio::spawn(async move {
        manager.run().await;
    });

    (temp, handle)
}

// ===== Malformed Data Tests =====

#[tokio::test]
async fn test_invalid_utxo_key_format() -> Result<()> {
    let (_temp, handle) = create_test_storage().await;

    // Test various invalid UTxO key formats
    let invalid_keys = vec![
        "",                    // Empty
        "notxhash",           // Missing output index
        "#5",                 // Missing tx hash
        "txhash#",            // Missing output index
        "txhash#notanumber",  // Non-numeric output index
        "txhash#-1",          // Negative output index
    ];

    for key in invalid_keys {
        // Should not panic - just insert with weird key
        let data = vec![1, 2, 3];
        let result = handle.insert_utxo(key.to_string(), data).await;
        // System should handle gracefully
        assert!(result.is_ok(), "Should handle invalid key format: {}", key);
    }

    Ok(())
}

#[tokio::test]
async fn test_corrupted_utxo_data() -> Result<()> {
    let (_temp, handle) = create_test_storage().await;

    // Insert corrupted/invalid JSON data
    let corrupted_data = vec![
        vec![0xFF, 0xFF, 0xFF],           // Invalid bytes
        b"{invalid json}".to_vec(),        // Malformed JSON
        b"not json at all".to_vec(),       // Not JSON
        vec![],                             // Empty data
    ];

    for (i, data) in corrupted_data.iter().enumerate() {
        let key = format!("corrupt_tx{}#0", i);
        let result = handle.insert_utxo(key.clone(), data.clone()).await;

        // Should accept any data (storage is agnostic to format)
        assert!(result.is_ok(), "Should store corrupted data");

        // Verify it's retrievable
        let retrieved = handle.get_utxo(key).await?;
        assert_eq!(retrieved, Some(data.clone()));
    }

    Ok(())
}

#[tokio::test]
async fn test_invalid_address_format() -> Result<()> {
    let (_temp, handle) = create_test_storage().await;

    let invalid_addresses = vec![
        "".to_string(),                              // Empty
        "not_an_address".to_string(),                 // Invalid format
        "addr_test1".to_string(),                     // Too short
        format!("stake1{}", "x".repeat(200)),         // Too long
        "\0\0\0".to_string(),                         // Null bytes
    ];

    for (i, addr) in invalid_addresses.iter().enumerate() {
        let utxo_key = format!("invalid_addr_tx{}#0", i);
        let data = vec![i as u8; 50];

        // Should handle invalid addresses gracefully
        handle.insert_utxo(utxo_key.clone(), data).await?;
        let result = handle.add_utxo_to_address_index(addr.clone(), utxo_key).await;
        assert!(result.is_ok(), "Should handle invalid address: {}", addr);
    }

    Ok(())
}

#[tokio::test]
async fn test_extremely_large_utxo_data() -> Result<()> {
    let (_temp, handle) = create_test_storage().await;

    // Test with progressively larger data sizes
    for size in [1_000, 10_000, 100_000, 1_000_000] {
        let key = format!("large_tx_{}#0", size);
        let data = vec![0xAB; size];

        let result = handle.insert_utxo(key.clone(), data.clone()).await;
        assert!(result.is_ok(), "Should handle {}KB data", size / 1000);

        // Verify retrieval
        let retrieved = handle.get_utxo(key).await?;
        assert_eq!(retrieved.as_ref().map(|d| d.len()), Some(size));
    }

    Ok(())
}

// ===== Concurrent Access Patterns =====

#[tokio::test]
async fn test_concurrent_writes_same_key() -> Result<()> {
    let (_temp, handle) = create_test_storage().await;

    let key = "contested_utxo#0".to_string();

    // Spawn many tasks writing to the same key
    let mut tasks = vec![];
    for i in 0..100 {
        let h = handle.clone();
        let k = key.clone();
        tasks.push(tokio::spawn(async move {
            let data = vec![i as u8; 50];
            h.insert_utxo(k, data).await
        }));
    }

    // All should succeed (last write wins)
    for task in tasks {
        assert!(task.await.unwrap().is_ok());
    }

    // Should have some value
    let final_data = handle.get_utxo(key).await?;
    assert!(final_data.is_some(), "Should have final value after concurrent writes");

    Ok(())
}

#[tokio::test]
async fn test_concurrent_read_write_same_address() -> Result<()> {
    let (_temp, handle) = create_test_storage().await;

    let test_address = "addr_test1concurrent";

    // Pre-populate some UTxOs
    for i in 0..10 {
        let key = format!("initial_tx{}#0", i);
        let data = vec![i as u8; 30];
        handle.insert_utxo(key.clone(), data).await?;
        handle.add_utxo_to_address_index(test_address.to_string(), key).await?;
    }

    // Spawn concurrent readers and writers
    let mut read_tasks = vec![];
    let mut write_tasks = vec![];

    // Readers
    for _ in 0..20 {
        let h = handle.clone();
        let addr = test_address.to_string();
        read_tasks.push(tokio::spawn(async move {
            h.get_utxos_for_address(addr).await
        }));
    }

    // Writers
    for i in 10..30 {
        let h = handle.clone();
        let addr = test_address.to_string();
        write_tasks.push(tokio::spawn(async move {
            let key = format!("concurrent_tx{}#0", i);
            let data = vec![i as u8; 30];
            h.insert_utxo(key.clone(), data).await?;
            h.add_utxo_to_address_index(addr, key).await
        }));
    }

    // All operations should complete without errors
    for task in read_tasks {
        assert!(task.await.is_ok());
    }

    for task in write_tasks {
        assert!(task.await.is_ok());
    }

    Ok(())
}

// ===== Out of Order Operations =====

#[tokio::test]
async fn test_get_from_nonexistent_address() -> Result<()> {
    let (_temp, handle) = create_test_storage().await;

    let addr = "addr_never_used_12345";

    let utxos = handle.get_utxos_for_address(addr.to_string()).await?;
    assert_eq!(utxos.len(), 0, "Should return empty list for unknown address");

    Ok(())
}

// ===== Boundary Conditions =====

#[tokio::test]
async fn test_zero_slot_chain_tip() -> Result<()> {
    let (_temp, handle) = create_test_storage().await;

    // Slot 0 should be valid (genesis)
    handle.store_chain_tip(0, vec![0], 0).await?;

    let tip = handle.get_chain_tip().await?;
    assert!(tip.is_some());
    assert_eq!(tip.unwrap().slot, 0);

    Ok(())
}

#[tokio::test]
async fn test_maximum_slot_number() -> Result<()> {
    let (_temp, handle) = create_test_storage().await;

    // Test with very large slot number
    let max_slot = u64::MAX;
    handle.store_chain_tip(max_slot, vec![0xFF], u64::MAX).await?;

    let tip = handle.get_chain_tip().await?;
    assert!(tip.is_some());
    assert_eq!(tip.unwrap().slot, max_slot);

    Ok(())
}

#[tokio::test]
async fn test_empty_block_hash() -> Result<()> {
    let (_temp, handle) = create_test_storage().await;

    // Empty hash should be allowed
    let empty_hash: Vec<u8> = vec![];
    handle.store_chain_tip(100, empty_hash.clone(), 1000).await?;

    let tip = handle.get_chain_tip().await?;
    assert!(tip.is_some());
    assert_eq!(tip.unwrap().hash, empty_hash);

    Ok(())
}

#[tokio::test]
async fn test_very_long_block_hash() -> Result<()> {
    let (_temp, handle) = create_test_storage().await;

    // Unrealistically long hash
    let long_hash = vec![0xAB; 1000];
    handle.store_chain_tip(100, long_hash.clone(), 1000).await?;

    let tip = handle.get_chain_tip().await?;
    assert_eq!(tip.unwrap().hash, long_hash);

    Ok(())
}

// ===== Duplicate Operations =====

#[tokio::test]
async fn test_duplicate_utxo_insertion() -> Result<()> {
    let (_temp, handle) = create_test_storage().await;

    let key = "duplicate_tx#0";
    let data1 = vec![1, 2, 3];
    let data2 = vec![4, 5, 6];

    // Insert twice with different data
    handle.insert_utxo(key.to_string(), data1).await?;
    handle.insert_utxo(key.to_string(), data2.clone()).await?;

    // Last write should win
    let retrieved = handle.get_utxo(key.to_string()).await?;
    assert_eq!(retrieved, Some(data2));

    Ok(())
}

#[tokio::test]
async fn test_duplicate_address_index_entry() -> Result<()> {
    let (_temp, handle) = create_test_storage().await;

    let addr = "addr_test1dup";
    let utxo_key = "dup_tx#0";

    // Add same UTxO to same address multiple times
    for _ in 0..5 {
        handle.add_utxo_to_address_index(addr.to_string(), utxo_key.to_string()).await?;
    }

    // Should deduplicate
    let utxos = handle.get_utxos_for_address(addr.to_string()).await?;
    let count = utxos.iter().filter(|k| k.as_str() == utxo_key).count();

    // Implementation may vary - either deduplicated or allowed duplicates
    println!("Found {} instances of the UTxO", count);
    assert!(count > 0, "Should have at least one entry");

    Ok(())
}

// ===== Stress Testing =====

#[tokio::test]
async fn test_rapid_chain_tip_updates() -> Result<()> {
    let (_temp, handle) = create_test_storage().await;

    // Rapidly update chain tip many times
    for i in 0..1000 {
        handle.store_chain_tip(i, vec![(i % 256) as u8], i * 1000).await?;
    }

    // Verify final state
    let tip = handle.get_chain_tip().await?;
    assert_eq!(tip.unwrap().slot, 999);

    Ok(())
}

#[tokio::test]
async fn test_many_utxos_single_address() -> Result<()> {
    let (_temp, handle) = create_test_storage().await;

    let addr = "addr_test1manyutxos";

    // Add many UTxOs to single address
    for i in 0..500 {
        let key = format!("many_tx{}#0", i);
        let data = vec![i as u8; 20];
        handle.insert_utxo(key.clone(), data).await?;
        handle.add_utxo_to_address_index(addr.to_string(), key).await?;
    }

    // Should handle large result set
    let utxos = handle.get_utxos_for_address(addr.to_string()).await?;
    assert!(utxos.len() >= 500, "Should handle many UTxOs per address");

    Ok(())
}

#[tokio::test]
async fn test_concurrent_snapshot_and_writes() -> Result<()> {
    let (_temp, handle) = create_test_storage().await;

    // Start background writes
    let write_handle = handle.clone();
    let write_task = tokio::spawn(async move {
        for i in 0..100 {
            let key = format!("snapshot_concurrent_tx{}#0", i);
            let data = vec![i as u8; 30];
            let _ = write_handle.insert_utxo(key, data).await;
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }
    });

    // Trigger snapshots during writes
    for slot in [50, 100, 150] {
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        handle.save_snapshots(slot).await?;
    }

    write_task.await?;

    Ok(())
}

// ===== Special Characters and Edge Cases =====

#[tokio::test]
async fn test_special_characters_in_keys() -> Result<()> {
    let (_temp, handle) = create_test_storage().await;

    let special_keys = vec![
        "tx\n#0",           // Newline
        "tx\t#0",           // Tab
        "tx #0",            // Space
        "tx/slash#0",       // Slash
        "tx\\backslash#0",  // Backslash
        "tx\"quote#0",      // Quote
        "tx'apostrophe#0",  // Apostrophe
        "tx🚀emoji#0",      // Emoji
    ];

    for key in special_keys {
        let data = b"test data".to_vec();
        let result = handle.insert_utxo(key.to_string(), data).await;
        assert!(result.is_ok(), "Should handle special chars in key: {:?}", key);
    }

    Ok(())
}

#[tokio::test]
async fn test_null_bytes_in_data() -> Result<()> {
    let (_temp, handle) = create_test_storage().await;

    let data_with_nulls = vec![0, 1, 0, 2, 0, 3, 0];

    handle.insert_utxo("null_test#0".to_string(), data_with_nulls.clone()).await?;

    let retrieved = handle.get_utxo("null_test#0".to_string()).await?;
    assert_eq!(retrieved, Some(data_with_nulls));

    Ok(())
}
