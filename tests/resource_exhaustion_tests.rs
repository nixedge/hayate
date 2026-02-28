// Resource Exhaustion Tests
// Test system behavior when approaching or exceeding resource limits

use anyhow::Result;
use hayate::indexer::{Network, NetworkStorage, StorageManager};
use tempfile::TempDir;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

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

// ===== Memory Pressure Tests =====

#[tokio::test]
async fn test_large_utxo_set() -> Result<()> {
    // Test handling of very large UTxO sets
    let (_temp, handle) = create_test_storage().await;

    let num_utxos = 10_000;
    println!("Creating {} UTxOs to test memory pressure", num_utxos);

    for i in 0..num_utxos {
        let key = format!("large_set_tx{}#{}", i / 10, i % 10);
        let data = vec![(i % 256) as u8; 100];
        handle.insert_utxo(key, data).await?;

        // Log progress
        if i % 1000 == 0 {
            println!("Inserted {} UTxOs", i);
        }
    }

    println!("Successfully created {} UTxOs", num_utxos);
    Ok(())
}

#[tokio::test]
async fn test_very_large_single_utxo() -> Result<()> {
    // Test handling of unrealistically large single UTxO (e.g., large metadata)
    let (_temp, handle) = create_test_storage().await;

    // 10MB UTxO data
    let size = 10 * 1024 * 1024;
    println!("Creating {}MB UTxO", size / (1024 * 1024));

    let large_data = vec![0xAB; size];
    let result = handle.insert_utxo("huge_utxo#0".to_string(), large_data).await;

    assert!(result.is_ok(), "Should handle very large UTxO data");
    println!("Successfully stored {}MB UTxO", size / (1024 * 1024));

    Ok(())
}

#[tokio::test]
async fn test_many_addresses_with_utxos() -> Result<()> {
    // Test memory usage with many different addresses
    let (_temp, handle) = create_test_storage().await;

    let num_addresses = 1000;
    let utxos_per_address = 10;

    println!("Creating {} addresses with {} UTxOs each", num_addresses, utxos_per_address);

    for addr_idx in 0..num_addresses {
        let address = format!("addr_test1_many_{}", addr_idx);

        for utxo_idx in 0..utxos_per_address {
            let key = format!("addr{}_tx{}#0", addr_idx, utxo_idx);
            let data = vec![(addr_idx % 256) as u8; 50];

            handle.insert_utxo(key.clone(), data).await?;
            handle.add_utxo_to_address_index(address.clone(), key).await?;
        }

        if addr_idx % 100 == 0 {
            println!("Created {} addresses", addr_idx);
        }
    }

    println!("Successfully created {} addresses with UTxOs", num_addresses);
    Ok(())
}

// ===== Concurrent Connection Limits =====

#[tokio::test]
async fn test_many_concurrent_operations() -> Result<()> {
    // Test handling of many concurrent database operations
    let (_temp, handle) = create_test_storage().await;

    let num_concurrent = 1000;
    println!("Spawning {} concurrent operations", num_concurrent);

    let mut tasks = vec![];
    let success_count = Arc::new(AtomicUsize::new(0));
    let error_count = Arc::new(AtomicUsize::new(0));

    for i in 0..num_concurrent {
        let h = handle.clone();
        let success = success_count.clone();
        let errors = error_count.clone();

        tasks.push(tokio::spawn(async move {
            let key = format!("concurrent_{}#0", i);
            let data = vec![(i % 256) as u8; 100];

            match h.insert_utxo(key, data).await {
                Ok(_) => success.fetch_add(1, Ordering::Relaxed),
                Err(_) => errors.fetch_add(1, Ordering::Relaxed),
            };
        }));
    }

    // Wait for all
    for task in tasks {
        let _ = task.await;
    }

    let final_success = success_count.load(Ordering::Relaxed);
    let final_errors = error_count.load(Ordering::Relaxed);

    println!("Concurrent operations: {} succeeded, {} failed", final_success, final_errors);
    assert!(final_success > 0, "At least some operations should succeed");

    Ok(())
}

#[tokio::test]
async fn test_sustained_high_throughput() -> Result<()> {
    // Test sustained high-throughput writes
    let (_temp, handle) = create_test_storage().await;

    let operations = 5000;
    let start = std::time::Instant::now();

    println!("Starting sustained throughput test with {} operations", operations);

    for i in 0..operations {
        let key = format!("throughput_tx{}#0", i);
        let data = vec![(i % 256) as u8; 200];
        handle.insert_utxo(key, data).await?;
    }

    let elapsed = start.elapsed();
    let ops_per_sec = operations as f64 / elapsed.as_secs_f64();

    println!("Completed {} operations in {:.2}s ({:.0} ops/sec)",
             operations, elapsed.as_secs_f64(), ops_per_sec);

    Ok(())
}

// ===== Database Growth Tests =====

#[tokio::test]
async fn test_address_with_excessive_utxos() -> Result<()> {
    // Test single address with unrealistic number of UTxOs (e.g., exchange wallet)
    let (_temp, handle) = create_test_storage().await;

    let address = "addr_test1_whale_wallet";
    let num_utxos = 5000;

    println!("Creating address with {} UTxOs (whale wallet scenario)", num_utxos);

    for i in 0..num_utxos {
        let key = format!("whale_tx{}#0", i);
        let data = vec![(i % 256) as u8; 100];

        handle.insert_utxo(key.clone(), data).await?;
        handle.add_utxo_to_address_index(address.to_string(), key).await?;

        if i % 1000 == 0 {
            println!("Added {} UTxOs to address", i);
        }
    }

    // Try to retrieve all UTxOs
    println!("Retrieving all UTxOs for address...");
    let start = std::time::Instant::now();
    let utxos = handle.get_utxos_for_address(address.to_string()).await?;
    let elapsed = start.elapsed();

    println!("Retrieved {} UTxOs in {:.3}s", utxos.len(), elapsed.as_secs_f64());
    assert!(utxos.len() >= num_utxos, "Should retrieve all UTxOs");

    Ok(())
}

#[tokio::test]
async fn test_rapid_chain_tip_churn() -> Result<()> {
    // Test rapid chain tip updates (simulating fast sync or rollbacks)
    let (_temp, handle) = create_test_storage().await;

    let updates = 10_000;
    println!("Performing {} rapid chain tip updates", updates);

    let start = std::time::Instant::now();

    for slot in 0..updates {
        let hash = vec![(slot % 256) as u8, ((slot / 256) % 256) as u8];
        handle.store_chain_tip(slot, hash, slot * 1000).await?;

        if slot % 1000 == 0 {
            println!("Chain tip at slot {}", slot);
        }
    }

    let elapsed = start.elapsed();
    println!("Completed {} chain tip updates in {:.2}s ({:.0} updates/sec)",
             updates, elapsed.as_secs_f64(), updates as f64 / elapsed.as_secs_f64());

    Ok(())
}

// ===== Snapshot Stress Tests =====

#[tokio::test]
async fn test_many_snapshots() -> Result<()> {
    // Test creating many snapshots to verify disk usage and management
    let (_temp, handle) = create_test_storage().await;

    // Add some data
    for i in 0..100 {
        let key = format!("snap_tx{}#0", i);
        let data = vec![i as u8; 100];
        handle.insert_utxo(key, data).await?;
    }

    let num_snapshots = 20;
    println!("Creating {} snapshots", num_snapshots);

    for slot in (0..num_snapshots).map(|i| i * 100) {
        handle.store_chain_tip(slot, vec![(slot % 256) as u8], slot * 1000).await?;
        handle.save_snapshots(slot).await?;
        println!("Created snapshot at slot {}", slot);
    }

    println!("Successfully created {} snapshots", num_snapshots);
    Ok(())
}

// ===== Query Load Tests =====

#[tokio::test]
async fn test_concurrent_queries_same_data() -> Result<()> {
    // Test many concurrent queries for the same data (cache pressure)
    let (_temp, handle) = create_test_storage().await;

    let address = "addr_test1_hotspot";

    // Populate data
    for i in 0..100 {
        let key = format!("hotspot_tx{}#0", i);
        let data = vec![i as u8; 50];
        handle.insert_utxo(key.clone(), data).await?;
        handle.add_utxo_to_address_index(address.to_string(), key).await?;
    }

    // Hammer with concurrent queries
    let num_queries = 500;
    println!("Executing {} concurrent queries for same address", num_queries);

    let mut tasks = vec![];
    let start = std::time::Instant::now();

    for _ in 0..num_queries {
        let h = handle.clone();
        let addr = address.to_string();
        tasks.push(tokio::spawn(async move {
            h.get_utxos_for_address(addr).await
        }));
    }

    let mut success = 0;
    for task in tasks {
        if task.await.is_ok() {
            success += 1;
        }
    }

    let elapsed = start.elapsed();
    println!("Completed {}/{} queries in {:.2}s ({:.0} queries/sec)",
             success, num_queries, elapsed.as_secs_f64(),
             num_queries as f64 / elapsed.as_secs_f64());

    assert_eq!(success, num_queries, "All queries should succeed");

    Ok(())
}

// ===== Mixed Workload Stress =====

#[tokio::test]
async fn test_mixed_heavy_workload() -> Result<()> {
    // Simulate realistic heavy load: reads, writes, indexing, tips
    let (_temp, handle) = create_test_storage().await;

    let duration_secs = 3;
    let start = std::time::Instant::now();

    let mut tasks = vec![];
    let write_count = Arc::new(AtomicUsize::new(0));
    let read_count = Arc::new(AtomicUsize::new(0));

    println!("Running mixed workload for {} seconds", duration_secs);

    // Writer tasks
    for worker_id in 0..5 {
        let h = handle.clone();
        let counter = write_count.clone();
        let start_time = start;

        tasks.push(tokio::spawn(async move {
            let mut i = 0;
            while start_time.elapsed().as_secs() < duration_secs {
                let key = format!("worker{}_tx{}#0", worker_id, i);
                let data = vec![(i % 256) as u8; 150];
                if h.insert_utxo(key, data).await.is_ok() {
                    counter.fetch_add(1, Ordering::Relaxed);
                }
                i += 1;
            }
        }));
    }

    // Reader tasks
    for _ in 0..5 {
        let h = handle.clone();
        let counter = read_count.clone();
        let start_time = start;

        tasks.push(tokio::spawn(async move {
            let mut i = 0;
            while start_time.elapsed().as_secs() < duration_secs {
                let key = format!("worker0_tx{}#0", i % 100);
                if h.get_utxo(key).await.is_ok() {
                    counter.fetch_add(1, Ordering::Relaxed);
                }
                i += 1;
            }
        }));
    }

    // Chain tip updater
    {
        let h = handle.clone();
        let start_time = start;
        tasks.push(tokio::spawn(async move {
            let mut slot = 0;
            while start_time.elapsed().as_secs() < duration_secs {
                let hash = vec![(slot % 256) as u8];
                let _ = h.store_chain_tip(slot, hash, slot * 1000).await;
                slot += 1;
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            }
        }));
    }

    // Wait for all tasks
    for task in tasks {
        let _ = task.await;
    }

    let elapsed = start.elapsed();
    let writes = write_count.load(Ordering::Relaxed);
    let reads = read_count.load(Ordering::Relaxed);

    println!("Mixed workload results over {:.2}s:", elapsed.as_secs_f64());
    println!("  Writes: {} ({:.0} writes/sec)", writes, writes as f64 / elapsed.as_secs_f64());
    println!("  Reads: {} ({:.0} reads/sec)", reads, reads as f64 / elapsed.as_secs_f64());
    println!("  Total ops: {} ({:.0} ops/sec)",
             writes + reads, (writes + reads) as f64 / elapsed.as_secs_f64());

    Ok(())
}

// ===== Boundary Growth Tests =====

#[tokio::test]
async fn test_progressive_database_growth() -> Result<()> {
    // Test database behavior as it grows progressively larger
    let (_temp, handle) = create_test_storage().await;

    let phases = vec![100, 500, 1000, 5000];

    for (phase_idx, target_size) in phases.iter().enumerate() {
        let start_size = if phase_idx == 0 { 0 } else { phases[phase_idx - 1] };

        println!("Phase {}: Growing from {} to {} UTxOs", phase_idx + 1, start_size, target_size);

        let phase_start = std::time::Instant::now();

        for i in start_size..*target_size {
            let key = format!("growth_tx{}#0", i);
            let data = vec![(i % 256) as u8; 100];
            handle.insert_utxo(key, data).await?;
        }

        let phase_elapsed = phase_start.elapsed();
        let ops = target_size - start_size;

        println!("  Phase {} completed: {} ops in {:.2}s ({:.0} ops/sec)",
                 phase_idx + 1, ops, phase_elapsed.as_secs_f64(),
                 ops as f64 / phase_elapsed.as_secs_f64());
    }

    Ok(())
}
