// End-to-end integration tests for Hayate
// Tests complete workflows: wallet sync, rewards, governance

use hayate::indexer::{HayateIndexer, Network};
use hayate::rewards::RewardsTracker;
use tempfile::TempDir;
use std::sync::Arc;

#[tokio::test]
async fn test_indexer_initialization() {
    let temp = TempDir::new().unwrap();
    let _indexer = HayateIndexer::new(temp.path().to_path_buf(), 20).unwrap();
    
    // Should initialize without networks
    // Networks added via add_network()
}

#[tokio::test]
async fn test_multi_network_support() {
    let temp = TempDir::new().unwrap();
    let indexer = HayateIndexer::new(temp.path().to_path_buf(), 20).unwrap();
    
    // Add multiple networks
    indexer.add_network(Network::Mainnet, temp.path().to_path_buf()).await.unwrap();
    indexer.add_network(Network::Preprod, temp.path().to_path_buf()).await.unwrap();
    indexer.add_network(Network::SanchoNet, temp.path().to_path_buf()).await.unwrap();
    
    // Each network should have independent storage
    // Verify by checking locks
    let networks = indexer.networks.read().await;
    assert_eq!(networks.len(), 3);
}

#[tokio::test]
async fn test_account_upload() {
    let temp = TempDir::new().unwrap();
    let indexer = HayateIndexer::new(temp.path().to_path_buf(), 20).unwrap();
    
    // Upload account xpub
    let xpub = "xpub1...".to_string();
    indexer.add_account(xpub.clone()).await.unwrap();
    
    // Verify stored
    let accounts = indexer.account_xpubs.read().await;
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0], xpub);
}

#[tokio::test]
async fn test_multiple_accounts() {
    let temp = TempDir::new().unwrap();
    let indexer = HayateIndexer::new(temp.path().to_path_buf(), 20).unwrap();
    
    // Upload multiple accounts
    for i in 0..10 {
        let xpub = format!("xpub_{}", i);
        indexer.add_account(xpub).await.unwrap();
    }
    
    let accounts = indexer.account_xpubs.read().await;
    assert_eq!(accounts.len(), 10);
}

// ===== Wallet Workflow Tests =====

#[tokio::test]
async fn test_complete_wallet_lifecycle() {
    let temp = TempDir::new().unwrap();
    let indexer = Arc::new(HayateIndexer::new(temp.path().to_path_buf(), 20).unwrap());
    
    // 1. Add network
    indexer.add_network(Network::Preprod, temp.path().to_path_buf()).await.unwrap();
    
    // 2. Upload account
    indexer.add_account("xpub_alice".to_string()).await.unwrap();
    
    // 3. Discover addresses (TODO: implement)
    // 4. Sync blocks (TODO: implement)
    // 5. Query balance via UTxORPC (TODO: implement)
    
    // For now, just verify setup
    assert_eq!(indexer.account_xpubs.read().await.len(), 1);
}

// ===== Governance Workflow Tests =====

#[tokio::test]
async fn test_governance_proposal_tracking() {
    let temp = TempDir::new().unwrap();
    let indexer = HayateIndexer::new(temp.path().to_path_buf(), 20).unwrap();
    
    indexer.add_network(Network::SanchoNet, temp.path().to_path_buf()).await.unwrap();
    
    // Governance proposals should be tracked even if we don't own the proposer
    // This is for community voting
}

// ===== Configuration Tests =====

#[tokio::test]
async fn test_custom_network() {
    let temp = TempDir::new().unwrap();
    let indexer = HayateIndexer::new(temp.path().to_path_buf(), 20).unwrap();
    
    // Add custom network
    let custom_network = Network::Custom("my-testnet".to_string());
    indexer.add_network(custom_network, temp.path().to_path_buf()).await.unwrap();
    
    let networks = indexer.networks.read().await;
    assert!(networks.contains_key(&Network::Custom("my-testnet".to_string())));
}

#[tokio::test]
async fn test_gap_limit_configuration() {
    let temp = TempDir::new().unwrap();
    
    // Default gap limit
    let indexer1 = HayateIndexer::new(temp.path().to_path_buf(), 20).unwrap();
    assert_eq!(indexer1.gap_limit, 20);
    
    // Custom gap limit
    let indexer2 = HayateIndexer::new(temp.path().to_path_buf(), 100).unwrap();
    assert_eq!(indexer2.gap_limit, 100);
}

// ===== Error Handling Tests =====

#[tokio::test]
async fn test_invalid_network_path() {
    let indexer = HayateIndexer::new("/invalid/path".into(), 20).unwrap();
    
    // Should fail gracefully when trying to add network
    let result = indexer.add_network(
        Network::Preprod,
        "/invalid/path".into()
    ).await;
    
    assert!(result.is_err());
}

// ===== Concurrency Tests =====

#[tokio::test]
async fn test_concurrent_account_uploads() {
    let temp = TempDir::new().unwrap();
    let indexer = Arc::new(HayateIndexer::new(temp.path().to_path_buf(), 20).unwrap());
    
    // Upload accounts concurrently
    let mut handles = vec![];
    
    for i in 0..10 {
        let indexer_clone = indexer.clone();
        let handle = tokio::spawn(async move {
            let xpub = format!("xpub_{}", i);
            indexer_clone.add_account(xpub).await.unwrap();
        });
        handles.push(handle);
    }
    
    // Wait for all
    for handle in handles {
        handle.await.unwrap();
    }
    
    // Should have all 10 accounts
    let accounts = indexer.account_xpubs.read().await;
    assert_eq!(accounts.len(), 10);
}

#[tokio::test]
async fn test_concurrent_network_operations() {
    let temp = TempDir::new().unwrap();
    let indexer = Arc::new(HayateIndexer::new(temp.path().to_path_buf(), 20).unwrap());
    
    // Add networks concurrently
    let indexer1 = indexer.clone();
    let temp_path1 = temp.path().to_path_buf();
    let handle1 = tokio::spawn(async move {
        indexer1.add_network(Network::Mainnet, temp_path1).await
    });
    
    let indexer2 = indexer.clone();
    let temp_path2 = temp.path().to_path_buf();
    let handle2 = tokio::spawn(async move {
        indexer2.add_network(Network::Preprod, temp_path2).await
    });
    
    // Both should succeed
    handle1.await.unwrap().unwrap();
    handle2.await.unwrap().unwrap();
    
    let networks = indexer.networks.read().await;
    assert_eq!(networks.len(), 2);
}

// ===== Stress Tests =====

#[tokio::test]
async fn test_large_number_of_accounts() {
    let temp = TempDir::new().unwrap();
    let indexer = HayateIndexer::new(temp.path().to_path_buf(), 20).unwrap();
    
    // Add 1000 accounts
    for i in 0..1000 {
        let xpub = format!("xpub_{:04}", i);
        indexer.add_account(xpub).await.unwrap();
    }
    
    let accounts = indexer.account_xpubs.read().await;
    assert_eq!(accounts.len(), 1000);
}

#[tokio::test]
async fn test_many_epoch_snapshots() {
    let temp = TempDir::new().unwrap();
    let mut tracker = RewardsTracker::open(temp.path(), 0).unwrap();
    
    let stake_key = vec![0xaa; 28];
    
    // Snapshot 500 epochs
    for epoch in 0..500 {
        let balance = epoch * 100_000;  // Slowly growing rewards
        tracker.snapshot_rewards(&stake_key, epoch, balance, None).unwrap();
    }
    
    // Verify random samples
    assert_eq!(tracker.get_snapshot(&stake_key, 0).unwrap().unwrap().balance, 0);
    assert_eq!(tracker.get_snapshot(&stake_key, 250).unwrap().unwrap().balance, 25_000_000);
    assert_eq!(tracker.get_snapshot(&stake_key, 499).unwrap().unwrap().balance, 49_900_000);
}
