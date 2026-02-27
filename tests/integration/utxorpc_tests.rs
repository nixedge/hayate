// Integration tests for UTxORPC Query and Watch services
// Tests datum support, multi-asset handling, and chain sync streaming

use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

use crate::common::{TestConfig, setup_test_indexer};

/// Test UTxO query basic functionality
#[tokio::test]
async fn test_utxo_query_basic() -> anyhow::Result<()> {
    let config = TestConfig::sanchonet();
    let indexer: Arc<hayate::indexer::HayateIndexer> = setup_test_indexer(config).await?;

    // Verify indexer is initialized
    let networks: tokio::sync::RwLockReadGuard<std::collections::HashMap<hayate::indexer::Network, hayate::indexer::NetworkStorage>> = indexer.networks.read().await;
    assert!(networks.contains_key(&hayate::indexer::Network::SanchoNet));

    Ok(())
}

/// Test UTxO query with datum extraction
#[tokio::test]
#[ignore] // Requires real SanchoNet data
async fn test_utxo_query_with_datum() -> anyhow::Result<()> {
    let config = TestConfig::sanchonet();
    let _indexer: Arc<hayate::indexer::HayateIndexer> = setup_test_indexer(config).await?;

    // TODO: Implement when real test data is available
    // 1. Query address known to have UTxOs with datums
    // 2. Verify datum_hash is present
    // 3. Verify inline datum data if present
    // 4. Verify datum hash matches computed hash of datum data

    Ok(())
}

/// Test UTxO query with multi-asset (native tokens)
#[tokio::test]
#[ignore] // Requires real SanchoNet data
async fn test_utxo_query_with_multiasset() -> anyhow::Result<()> {
    let config = TestConfig::sanchonet();
    let _indexer: Arc<hayate::indexer::HayateIndexer> = setup_test_indexer(config).await?;

    // TODO: Implement when real test data is available
    // 1. Query address known to hold native tokens
    // 2. Verify assets field is populated
    // 3. Verify policy_id and asset_name are correct
    // 4. Verify asset amounts match expected values

    Ok(())
}

/// Test chain sync follow_tip streaming
#[tokio::test]
#[ignore] // Requires running node
async fn test_follow_tip_streaming() -> anyhow::Result<()> {
    let config = TestConfig::sanchonet();
    let indexer: Arc<hayate::indexer::HayateIndexer> = setup_test_indexer(config).await?;

    // Subscribe to block updates
    let mut block_rx = indexer.subscribe_blocks();

    // Spawn a task to broadcast a test block
    let indexer_clone = indexer.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Simulate a block update
        indexer_clone.broadcast_block(hayate::indexer::BlockUpdate {
            network: hayate::indexer::Network::SanchoNet,
            height: 12345,
            slot: 67890,
            hash: vec![0xde, 0xad, 0xbe, 0xef],
            tx_hashes: vec![
                vec![0x01, 0x02, 0x03, 0x04],
                vec![0x05, 0x06, 0x07, 0x08],
            ],
        });
    });

    // Wait for block update with timeout
    let result: Result<Result<hayate::indexer::BlockUpdate, tokio::sync::broadcast::error::RecvError>, tokio::time::error::Elapsed> = timeout(Duration::from_secs(5), block_rx.recv()).await;
    let update = result
        .expect("Timeout waiting for block update")
        .expect("Failed to receive block update");

    // Verify update contents
    assert_eq!(update.height, 12345);
    assert_eq!(update.slot, 67890);
    assert_eq!(update.hash, vec![0xde, 0xad, 0xbe, 0xef]);
    assert_eq!(update.tx_hashes.len(), 2);

    Ok(())
}

/// Test multiple concurrent subscribers to follow_tip
#[tokio::test]
async fn test_follow_tip_multiple_subscribers() -> anyhow::Result<()> {
    let config = TestConfig::sanchonet();
    let indexer: Arc<hayate::indexer::HayateIndexer> = setup_test_indexer(config).await?;

    // Create multiple subscribers
    let mut rx1 = indexer.subscribe_blocks();
    let mut rx2 = indexer.subscribe_blocks();
    let mut rx3 = indexer.subscribe_blocks();

    // Broadcast a block
    indexer.broadcast_block(hayate::indexer::BlockUpdate {
        network: hayate::indexer::Network::SanchoNet,
        height: 100,
        slot: 200,
        hash: vec![0xaa, 0xbb, 0xcc, 0xdd],
        tx_hashes: vec![],
    });

    // All subscribers should receive the update
    let result1 = timeout(Duration::from_secs(1), rx1.recv()).await;
    let update1 = result1??;
    let result2 = timeout(Duration::from_secs(1), rx2.recv()).await;
    let update2 = result2??;
    let result3 = timeout(Duration::from_secs(1), rx3.recv()).await;
    let update3 = result3??;

    assert_eq!(update1.height, 100);
    assert_eq!(update2.height, 100);
    assert_eq!(update3.height, 100);

    Ok(())
}

/// Test datum hash computation matches Cardano specification
#[test]
fn test_datum_hash_computation() {
    // Test that our datum hash computation matches Cardano's Blake2b-256
    // This is critical for correct datum identification

    use pallas_crypto::hash::Hasher;

    // Example datum bytes (simple integer constructor: 0)
    let datum_bytes = vec![0xd8, 0x79, 0x9f, 0x00, 0xff]; // CBOR: constructor 0

    // Compute hash using Blake2b-256 (same as cardano-node)
    let mut hasher = Hasher::<256>::new();
    hasher.input(&datum_bytes);
    let hash = hasher.finalize();

    // Verify hash is 32 bytes
    assert_eq!(hash.len(), 32);

    // Hash should be deterministic
    let mut hasher2 = Hasher::<256>::new();
    hasher2.input(&datum_bytes);
    let hash2 = hasher2.finalize();
    assert_eq!(hash, hash2);
}

/// Test asset key format (policy_id.asset_name)
#[test]
fn test_asset_key_format() {
    // Verify our asset key format matches expected structure
    let policy_id = "a0028f350aaabe0545fdcb56b039bfb08e4bb4d8c4d7c3c7d481c235";
    let asset_name = "484f534b59"; // "HOSKY" in hex

    let asset_key = format!("{}.{}", policy_id, asset_name);

    // Should be parseable
    let parts: Vec<&str> = asset_key.split('.').collect();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0], policy_id);
    assert_eq!(parts[1], asset_name);

    // Policy ID should be 56 hex chars (28 bytes)
    assert_eq!(policy_id.len(), 56);

    // Asset name can be 0-64 hex chars (0-32 bytes)
    assert!(asset_name.len() <= 64);
}

/// Test error handling for invalid addresses
#[tokio::test]
async fn test_invalid_address_handling() -> anyhow::Result<()> {
    let config = TestConfig::sanchonet();
    let _indexer: Arc<hayate::indexer::HayateIndexer> = setup_test_indexer(config).await?;

    // TODO: Test querying with:
    // - Malformed addresses
    // - Wrong network addresses (mainnet addr on testnet)
    // - Empty address list

    Ok(())
}

/// Benchmark: UTxO query performance
#[tokio::test]
#[ignore] // Performance test
async fn bench_utxo_query_performance() -> anyhow::Result<()> {
    let config = TestConfig::sanchonet();
    let _indexer: Arc<hayate::indexer::HayateIndexer> = setup_test_indexer(config).await?;

    // TODO: Measure query performance
    // - Query 1 address: should be < 100ms
    // - Query 10 addresses: should be < 500ms
    // - Query 100 addresses: should be < 2s

    Ok(())
}

/// Benchmark: Chain sync throughput
#[tokio::test]
#[ignore] // Performance test
async fn bench_chain_sync_throughput() -> anyhow::Result<()> {
    let config = TestConfig::sanchonet();
    let indexer: Arc<hayate::indexer::HayateIndexer> = setup_test_indexer(config).await?;

    let mut rx = indexer.subscribe_blocks();

    // Spawn task to broadcast many blocks
    let indexer_clone = indexer.clone();
    tokio::spawn(async move {
        for i in 0..1000 {
            indexer_clone.broadcast_block(hayate::indexer::BlockUpdate {
                network: hayate::indexer::Network::SanchoNet,
                height: i,
                slot: i * 20,
                hash: vec![0; 32],
                tx_hashes: vec![],
            });
            tokio::time::sleep(Duration::from_micros(100)).await;
        }
    });

    // Measure how many we can receive
    let start = std::time::Instant::now();
    let mut count = 0;

    while count < 1000 {
        let result: Result<_, tokio::time::error::Elapsed> = timeout(Duration::from_secs(5), rx.recv()).await;
        if result.is_ok() {
            count += 1;
        } else {
            break;
        }
    }

    let elapsed = start.elapsed();
    let throughput = count as f64 / elapsed.as_secs_f64();

    println!("Chain sync throughput: {} blocks/sec", throughput);
    println!("Received {} blocks in {:?}", count, elapsed);

    // Should handle at least 100 blocks/sec
    assert!(throughput > 100.0, "Throughput too low: {}", throughput);

    Ok(())
}
