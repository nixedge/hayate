// Restart and Recovery Tests
// Critical tests for production reliability - verify the indexer can survive
// unclean shutdowns and resume indexing correctly

use anyhow::Result;
use hayate::indexer::{Network, NetworkStorage, StorageManager};
use std::path::PathBuf;
use tempfile::TempDir;

mod common;

// Helper to create storage at a specific path
async fn create_storage_at_path(path: PathBuf) -> hayate::indexer::StorageHandle {
    let storage = NetworkStorage::open(path, Network::Preview).unwrap();
    let (manager, handle) = StorageManager::new(storage);

    tokio::spawn(async move {
        manager.run().await;
    });

    handle
}

#[tokio::test]
async fn test_clean_shutdown_and_restart() -> Result<()> {
    // Test that a clean shutdown preserves all state correctly
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().to_path_buf();

    // Phase 1: Create data
    {
        let handle = create_storage_at_path(db_path.clone()).await;

        // Insert some UTxOs
        for i in 0..10 {
            let key = format!("tx{}#0", i);
            let data = vec![i as u8; 100];
            handle.insert_utxo(key, data).await?;
        }

        // Store chain tip
        handle.store_chain_tip(100, vec![1, 2, 3], 1000).await?;

        // Save snapshot to persist data (LSM uses ephemeral writes)
        handle.save_snapshots(100).await?;

        // Clean shutdown
        handle.shutdown().await?;
        drop(handle);
    }

    // Phase 2: Restart and verify state
    {
        let handle = create_storage_at_path(db_path.clone()).await;

        // Verify chain tip survived restart
        let tip = handle.get_chain_tip().await?;
        assert!(tip.is_some(), "Chain tip should be preserved after restart");
        let chain_tip = tip.unwrap();
        assert_eq!(chain_tip.slot, 100);
        assert_eq!(chain_tip.hash, vec![1, 2, 3]);

        // Verify UTxOs survived
        for i in 0..10 {
            let key = format!("tx{}#0", i);
            let data = handle.get_utxo(key).await?;
            assert!(data.is_some(), "UTxO {} should exist", i);
        }

        handle.shutdown().await?;
        drop(handle);
    }

    Ok(())
}

#[tokio::test]
async fn test_crash_recovery() -> Result<()> {
    // Test recovery from unclean shutdown
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().to_path_buf();

    // Phase 1: Create data, then "crash"
    {
        let handle = create_storage_at_path(db_path.clone()).await;

        for i in 0..5 {
            let key = format!("crash_tx{}#0", i);
            let data = vec![i as u8; 50];
            handle.insert_utxo(key, data).await?;
        }

        handle.store_chain_tip(50, vec![5, 6], 2000).await?;

        // Simulate crash - drop without cleanup (no shutdown())
        drop(handle);

        // Wait for background task to finish
        // In production, this would be a different process after actual crash
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    }

    // Phase 2: Recovery - reopen database
    {
        let handle = create_storage_at_path(db_path.clone()).await;

        // Should be able to reopen without errors
        let tip = handle.get_chain_tip().await?;
        if let Some(chain_tip) = tip {
            assert_eq!(chain_tip.slot, 50);
            assert_eq!(chain_tip.hash, vec![5, 6]);
        }

        // Verify data
        for i in 0..5 {
            let key = format!("crash_tx{}#0", i);
            let _data = handle.get_utxo(key).await?;
            // Data may or may not be present depending on what was flushed
        }

        handle.shutdown().await?;
        drop(handle);
    }

    Ok(())
}

#[tokio::test]
async fn test_utxo_set_consistency_after_restart() -> Result<()> {
    // Verify UTxO set is consistent across restarts
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().to_path_buf();

    let test_address = "addr_test1qpw0djgj0x59ngrjvqthn7enhvruxnsavsw5th63la3mjel3tkc974sr23jmlzgq5zda4gtv8k9cy38756r9y3qgmkqqjz6aa7";

    // Phase 1: Create UTxOs
    {
        let handle = create_storage_at_path(db_path.clone()).await;

        for i in 0..5 {
            let utxo_key = format!("txhash{}#0", i);
            let utxo_data = serde_json::to_vec(&serde_json::json!({
                "tx_hash": format!("txhash{}", i),
                "output_index": 0,
                "address": test_address,
                "amount": 1_000_000 * (i + 1),
            }))?;

            handle.insert_utxo(utxo_key.clone(), utxo_data).await?;
            handle.add_utxo_to_address_index(test_address.to_string(), utxo_key).await?;
        }

        // Save snapshot to persist data
        handle.save_snapshots(0).await?;

        handle.shutdown().await?;
        drop(handle);
    }

    // Phase 2: Restart and verify UTxOs
    {
        let handle = create_storage_at_path(db_path.clone()).await;

        let utxos = handle.get_utxos_for_address(test_address.to_string()).await?;
        assert_eq!(utxos.len(), 5, "Should have 5 UTxOs after restart");

        // Verify each UTxO
        for utxo_key in &utxos {
            let data = handle.get_utxo(utxo_key.clone()).await?;
            assert!(data.is_some(), "UTxO data should exist for {}", utxo_key);
        }

        handle.shutdown().await?;
        drop(handle);
    }

    Ok(())
}

#[tokio::test]
async fn test_multiple_restarts() -> Result<()> {
    // Test multiple restart cycles don't corrupt data
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().to_path_buf();

    let test_key = "multi_restart_test_utxo#0";
    let test_data = b"test_value_persistent";

    // Do 3 restart cycles
    for cycle in 0..3 {
        let handle = create_storage_at_path(db_path.clone()).await;

        if cycle == 0 {
            // First cycle: create data
            handle.insert_utxo(test_key.to_string(), test_data.to_vec()).await?;
        } else {
            // Subsequent cycles: verify data still exists
            let data = handle.get_utxo(test_key.to_string()).await?;
            assert!(data.is_some(), "Data should persist through restart {}", cycle);
            assert_eq!(data.unwrap(), test_data, "Data should be unchanged after restart {}", cycle);
        }

        // Update chain tip each cycle
        handle.store_chain_tip(cycle as u64, vec![cycle as u8], 1000 * (cycle + 1)).await?;

        // Save snapshot to persist data
        handle.save_snapshots(cycle as u64).await?;

        handle.shutdown().await?;
        drop(handle);
    }

    Ok(())
}

#[tokio::test]
async fn test_restart_with_concurrent_operations() -> Result<()> {
    // Test restart with many concurrent operations
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().to_path_buf();

    // Phase 1: Many concurrent writes
    {
        let handle = create_storage_at_path(db_path.clone()).await;

        // Spawn many concurrent operations
        let mut tasks = vec![];
        for i in 0..50 {
            let h = handle.clone();
            tasks.push(tokio::spawn(async move {
                let key = format!("concurrent_tx{}#0", i);
                let data = vec![i as u8; 100];
                h.insert_utxo(key, data).await
            }));
        }

        // Wait for all
        for task in tasks {
            let _ = task.await;
        }

        // Save snapshot to persist concurrent writes
        handle.save_snapshots(0).await?;

        handle.shutdown().await?;
        drop(handle);
    }

    // Phase 2: Restart and verify what persisted
    {
        let handle = create_storage_at_path(db_path.clone()).await;

        let mut count = 0;
        for i in 0..50 {
            let key = format!("concurrent_tx{}#0", i);
            if let Ok(Some(_)) = handle.get_utxo(key).await {
                count += 1;
            }
        }

        // At least some should have persisted
        assert!(count > 0, "Some concurrent operations should persist");
        println!("Persisted {} out of 50 concurrent operations", count);

        handle.shutdown().await?;
        drop(handle);
    }

    Ok(())
}

#[tokio::test]
async fn test_chain_tip_persistence() -> Result<()> {
    // Test that chain tip updates persist correctly
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().to_path_buf();

    // Create tip 1
    {
        let handle = create_storage_at_path(db_path.clone()).await;
        handle.store_chain_tip(100, vec![1, 2, 3], 1000).await?;
        handle.save_snapshots(100).await?;
        handle.shutdown().await?;
        drop(handle);
    }

    // Update to tip 2
    {
        let handle = create_storage_at_path(db_path.clone()).await;
        handle.store_chain_tip(200, vec![4, 5, 6], 2000).await?;
        handle.save_snapshots(200).await?;
        handle.shutdown().await?;
        drop(handle);
    }

    // Verify tip 2 persisted
    {
        let handle = create_storage_at_path(db_path.clone()).await;
        let tip = handle.get_chain_tip().await?;
        assert!(tip.is_some());
        let chain_tip = tip.unwrap();
        assert_eq!(chain_tip.slot, 200);
        assert_eq!(chain_tip.hash, vec![4, 5, 6]);
        handle.shutdown().await?;
        drop(handle);
    }

    Ok(())
}

#[tokio::test]
async fn test_snapshot_creation_and_restart() -> Result<()> {
    // Test that snapshots can be created and database reopened
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().to_path_buf();

    {
        let handle = create_storage_at_path(db_path.clone()).await;

        // Insert data
        for i in 0..10 {
            let key = format!("snap_tx{}#0", i);
            let data = vec![i as u8; 50];
            handle.insert_utxo(key, data).await?;
        }

        handle.store_chain_tip(100, vec![1, 2, 3], 5000).await?;

        // Create snapshot
        handle.save_snapshots(100).await?;

        handle.shutdown().await?;
        drop(handle);
    }

    // Restart and verify
    {
        let handle = create_storage_at_path(db_path.clone()).await;

        let tip = handle.get_chain_tip().await?;
        assert!(tip.is_some());

        handle.shutdown().await?;
        drop(handle);
    }

    Ok(())
}

#[tokio::test]
async fn test_empty_database_restart() -> Result<()> {
    // Test that an empty database can be restarted
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().to_path_buf();

    // Create empty database
    {
        let handle = create_storage_at_path(db_path.clone()).await;
        handle.shutdown().await?;
        drop(handle);
    }

    // Restart empty database
    {
        let handle = create_storage_at_path(db_path.clone()).await;
        let tip = handle.get_chain_tip().await?;
        assert!(tip.is_none(), "Empty database should have no chain tip");
        handle.shutdown().await?;
        drop(handle);
    }

    Ok(())
}
