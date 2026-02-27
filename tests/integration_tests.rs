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

// ===== Multi-Network Storage Isolation Tests =====

#[tokio::test]
async fn test_networks_have_separate_storage_paths() {
    let temp_dir = TempDir::new().unwrap();
    let indexer = Arc::new(HayateIndexer::new(temp_dir.path().to_path_buf(), 20).unwrap());

    // Add multiple networks
    indexer.add_network(Network::Mainnet, temp_dir.path().to_path_buf()).await.unwrap();
    indexer.add_network(Network::Preprod, temp_dir.path().to_path_buf()).await.unwrap();
    indexer.add_network(Network::Preview, temp_dir.path().to_path_buf()).await.unwrap();

    // Verify each network has its own directory
    assert!(temp_dir.path().join("mainnet").exists());
    assert!(temp_dir.path().join("preprod").exists());
    assert!(temp_dir.path().join("preview").exists());

    // Verify subdirectories exist
    for network in &["mainnet", "preprod", "preview"] {
        let net_path = temp_dir.path().join(network);
        assert!(net_path.join("utxos").exists());
        assert!(net_path.join("balances").exists());
        assert!(net_path.join("governance").exists());
        assert!(net_path.join("rewards").exists());
        assert!(net_path.join("nonces").exists());
    }
}

#[tokio::test]
async fn test_network_storage_does_not_interfere() {
    let temp_dir = TempDir::new().unwrap();
    let indexer = Arc::new(HayateIndexer::new(temp_dir.path().to_path_buf(), 20).unwrap());

    // Add two networks
    indexer.add_network(Network::Mainnet, temp_dir.path().to_path_buf()).await.unwrap();
    indexer.add_network(Network::Preprod, temp_dir.path().to_path_buf()).await.unwrap();

    let networks = indexer.networks.read().await;

    // Verify networks are independent
    let mainnet = networks.get(&Network::Mainnet).unwrap();
    let preprod = networks.get(&Network::Preprod).unwrap();

    assert_eq!(mainnet.network, Network::Mainnet);
    assert_eq!(preprod.network, Network::Preprod);
}

#[tokio::test]
async fn test_custom_network_storage_isolation() {
    let temp_dir = TempDir::new().unwrap();
    let indexer = Arc::new(HayateIndexer::new(temp_dir.path().to_path_buf(), 20).unwrap());

    let custom1 = Network::Custom("testnet1".to_string());
    let custom2 = Network::Custom("testnet2".to_string());

    indexer.add_network(custom1.clone(), temp_dir.path().to_path_buf()).await.unwrap();
    indexer.add_network(custom2.clone(), temp_dir.path().to_path_buf()).await.unwrap();

    // Verify custom networks have separate storage
    assert!(temp_dir.path().join("testnet1").exists());
    assert!(temp_dir.path().join("testnet2").exists());
}

#[tokio::test]
async fn test_network_removal_does_not_affect_others() {
    let temp_dir = TempDir::new().unwrap();
    let indexer = Arc::new(HayateIndexer::new(temp_dir.path().to_path_buf(), 20).unwrap());

    // Add multiple networks
    indexer.add_network(Network::Mainnet, temp_dir.path().to_path_buf()).await.unwrap();
    indexer.add_network(Network::Preprod, temp_dir.path().to_path_buf()).await.unwrap();

    // Remove one network
    {
        let mut networks = indexer.networks.write().await;
        networks.remove(&Network::Preprod);
    }

    // Mainnet should still exist
    let networks = indexer.networks.read().await;
    assert!(networks.contains_key(&Network::Mainnet));
    assert!(!networks.contains_key(&Network::Preprod));
}

#[tokio::test]
async fn test_simultaneous_network_operations() {
    let temp_dir = TempDir::new().unwrap();
    let indexer = Arc::new(HayateIndexer::new(temp_dir.path().to_path_buf(), 20).unwrap());

    // Add networks
    indexer.add_network(Network::Mainnet, temp_dir.path().to_path_buf()).await.unwrap();
    indexer.add_network(Network::Preprod, temp_dir.path().to_path_buf()).await.unwrap();

    // Perform operations on different networks concurrently
    let indexer1 = indexer.clone();
    let indexer2 = indexer.clone();

    let handle1 = tokio::spawn(async move {
        // Simulate operations on mainnet
        let networks = indexer1.networks.read().await;
        networks.get(&Network::Mainnet).is_some()
    });

    let handle2 = tokio::spawn(async move {
        // Simulate operations on preprod
        let networks = indexer2.networks.read().await;
        networks.get(&Network::Preprod).is_some()
    });

    let (result1, result2) = tokio::join!(handle1, handle2);
    assert!(result1.unwrap());
    assert!(result2.unwrap());
}

#[tokio::test]
async fn test_network_storage_persistence() {
    let temp_dir = TempDir::new().unwrap();

    {
        // Create indexer and add network
        let indexer = Arc::new(HayateIndexer::new(temp_dir.path().to_path_buf(), 20).unwrap());
        indexer.add_network(Network::Mainnet, temp_dir.path().to_path_buf()).await.unwrap();

        // Write some data (simulate)
        let mut networks = indexer.networks.write().await;
        let mainnet = networks.get_mut(&Network::Mainnet).unwrap();
        let key = cardano_lsm::Key::from(b"test_key");
        let value = cardano_lsm::Value::from(b"test_value");
        mainnet.utxo_tree.insert(&key, &value).unwrap();
    }

    {
        // Create new indexer with same path
        let indexer = Arc::new(HayateIndexer::new(temp_dir.path().to_path_buf(), 20).unwrap());
        indexer.add_network(Network::Mainnet, temp_dir.path().to_path_buf()).await.unwrap();

        // Verify data persisted
        let networks = indexer.networks.read().await;
        let mainnet = networks.get(&Network::Mainnet).unwrap();
        let key = cardano_lsm::Key::from(b"test_key");
        let result = mainnet.utxo_tree.get(&key).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().as_ref(), b"test_value");
    }
}

#[tokio::test]
async fn test_different_indexing_start_epochs_per_network() {
    let temp_dir = TempDir::new().unwrap();
    let indexer = Arc::new(HayateIndexer::new(temp_dir.path().to_path_buf(), 20).unwrap());

    indexer.add_network(Network::Mainnet, temp_dir.path().to_path_buf()).await.unwrap();
    indexer.add_network(Network::Preprod, temp_dir.path().to_path_buf()).await.unwrap();

    let networks = indexer.networks.read().await;

    // Each network tracks its own indexing start epoch
    let mainnet = networks.get(&Network::Mainnet).unwrap();
    let preprod = networks.get(&Network::Preprod).unwrap();

    // Both should start at epoch 0 by default
    assert_eq!(mainnet.indexing_start_epoch, 0);
    assert_eq!(preprod.indexing_start_epoch, 0);
}

#[tokio::test]
async fn test_network_rewards_tracker_isolation() {
    let temp_dir = TempDir::new().unwrap();
    let indexer = Arc::new(HayateIndexer::new(temp_dir.path().to_path_buf(), 20).unwrap());

    indexer.add_network(Network::Mainnet, temp_dir.path().to_path_buf()).await.unwrap();
    indexer.add_network(Network::Preprod, temp_dir.path().to_path_buf()).await.unwrap();

    let stake_key = vec![1, 2, 3, 4];

    // Record withdrawal in mainnet
    {
        let mut networks = indexer.networks.write().await;
        let mainnet = networks.get_mut(&Network::Mainnet).unwrap();
        mainnet.rewards_tracker.record_withdrawal(&stake_key, 100, 10000, 1_000_000, vec![0; 32]).unwrap();
    }

    let networks = indexer.networks.read().await;
    let mainnet = networks.get(&Network::Mainnet).unwrap();
    let preprod = networks.get(&Network::Preprod).unwrap();

    // Preprod should not see it
    let preprod_withdrawals = preprod.rewards_tracker.get_epoch_withdrawals(&stake_key, 100).unwrap();
    assert_eq!(preprod_withdrawals, 0);

    // Mainnet should see it
    let mainnet_withdrawals = mainnet.rewards_tracker.get_epoch_withdrawals(&stake_key, 100).unwrap();
    assert_eq!(mainnet_withdrawals, 1_000_000);
}

#[tokio::test]
async fn test_network_balance_tree_isolation() {
    let temp_dir = TempDir::new().unwrap();
    let indexer = Arc::new(HayateIndexer::new(temp_dir.path().to_path_buf(), 20).unwrap());

    indexer.add_network(Network::Mainnet, temp_dir.path().to_path_buf()).await.unwrap();
    indexer.add_network(Network::Preprod, temp_dir.path().to_path_buf()).await.unwrap();

    let address = cardano_lsm::Key::from(b"addr_test123");

    // Add balance to mainnet
    {
        let mut networks = indexer.networks.write().await;
        let mainnet = networks.get_mut(&Network::Mainnet).unwrap();
        mainnet.balance_tree.insert(&address, &5_000_000).unwrap();
    }

    let networks = indexer.networks.read().await;
    let mainnet = networks.get(&Network::Mainnet).unwrap();
    let preprod = networks.get(&Network::Preprod).unwrap();

    // Preprod should have 0
    let preprod_balance = preprod.balance_tree.get(&address).unwrap();
    assert_eq!(preprod_balance, 0);

    // Mainnet should have the balance
    let mainnet_balance = mainnet.balance_tree.get(&address).unwrap();
    assert_eq!(mainnet_balance, 5_000_000);
}

#[tokio::test]
async fn test_network_nonce_storage_isolation() {
    let temp_dir = TempDir::new().unwrap();
    let indexer = Arc::new(HayateIndexer::new(temp_dir.path().to_path_buf(), 20).unwrap());

    indexer.add_network(Network::Mainnet, temp_dir.path().to_path_buf()).await.unwrap();
    indexer.add_network(Network::Preprod, temp_dir.path().to_path_buf()).await.unwrap();

    let mut networks = indexer.networks.write().await;

    let mainnet = networks.get_mut(&Network::Mainnet).unwrap();
    let epoch = 100;
    let nonce = vec![1, 2, 3, 4, 5, 6, 7, 8];

    // Store nonce in mainnet
    mainnet.store_nonce(epoch, &nonce).unwrap();

    // Mainnet should have it
    let retrieved = mainnet.get_nonce(epoch).unwrap();
    assert_eq!(retrieved, Some(nonce.clone()));

    // Preprod should not have it
    let preprod = networks.get(&Network::Preprod).unwrap();
    let preprod_nonce = preprod.get_nonce(epoch).unwrap();
    assert_eq!(preprod_nonce, None);
}
