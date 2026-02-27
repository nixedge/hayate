// API service tests - basic functionality and error handling

use hayate::{HayateIndexer, Network};
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_indexer_creation() {
    let temp_dir = TempDir::new().unwrap();
    let gap_limit = 20;

    let indexer = HayateIndexer::new(temp_dir.path().to_path_buf(), gap_limit);
    assert!(indexer.is_ok());

    let indexer = indexer.unwrap();
    assert_eq!(indexer.gap_limit, gap_limit);
}

#[tokio::test]
async fn test_add_network_to_indexer() {
    let temp_dir = TempDir::new().unwrap();
    let indexer = Arc::new(HayateIndexer::new(temp_dir.path().to_path_buf(), 20).unwrap());

    // Add a network
    let result = indexer
        .add_network(Network::Preprod, temp_dir.path().to_path_buf())
        .await;
    assert!(result.is_ok());

    // Check network was added
    let networks = indexer.networks.read().await;
    assert!(networks.contains_key(&Network::Preprod));
}

#[tokio::test]
async fn test_add_multiple_networks() {
    let temp_dir = TempDir::new().unwrap();
    let indexer = Arc::new(HayateIndexer::new(temp_dir.path().to_path_buf(), 20).unwrap());

    // Add multiple networks
    indexer
        .add_network(Network::Mainnet, temp_dir.path().to_path_buf())
        .await
        .unwrap();
    indexer
        .add_network(Network::Preprod, temp_dir.path().to_path_buf())
        .await
        .unwrap();
    indexer
        .add_network(Network::Preview, temp_dir.path().to_path_buf())
        .await
        .unwrap();

    let networks = indexer.networks.read().await;
    assert_eq!(networks.len(), 3);
    assert!(networks.contains_key(&Network::Mainnet));
    assert!(networks.contains_key(&Network::Preprod));
    assert!(networks.contains_key(&Network::Preview));
}

#[tokio::test]
async fn test_network_storage_paths_isolated() {
    let temp_dir = TempDir::new().unwrap();
    let indexer = Arc::new(HayateIndexer::new(temp_dir.path().to_path_buf(), 20).unwrap());

    // Add networks with same base path
    indexer
        .add_network(Network::Mainnet, temp_dir.path().to_path_buf())
        .await
        .unwrap();
    indexer
        .add_network(Network::Preprod, temp_dir.path().to_path_buf())
        .await
        .unwrap();

    // Check that storage paths are different
    let networks = indexer.networks.read().await;
    let mainnet_storage = networks.get(&Network::Mainnet).unwrap();
    let preprod_storage = networks.get(&Network::Preprod).unwrap();

    assert_eq!(mainnet_storage.network, Network::Mainnet);
    assert_eq!(preprod_storage.network, Network::Preprod);

    // Verify directories were created
    assert!(temp_dir.path().join("mainnet").exists());
    assert!(temp_dir.path().join("preprod").exists());
}

#[tokio::test]
async fn test_get_chain_tip_returns_default() {
    let temp_dir = TempDir::new().unwrap();
    let indexer = Arc::new(HayateIndexer::new(temp_dir.path().to_path_buf(), 20).unwrap());

    let tip = indexer.get_chain_tip().await;
    assert!(tip.is_ok());

    let tip = tip.unwrap();
    assert_eq!(tip.height, 0);
    assert_eq!(tip.slot, 0);
    assert_eq!(tip.hash.len(), 0);
}

#[tokio::test]
async fn test_custom_network_handling() {
    let temp_dir = TempDir::new().unwrap();
    let indexer = Arc::new(HayateIndexer::new(temp_dir.path().to_path_buf(), 20).unwrap());

    let custom_network = Network::Custom("my-testnet".to_string());
    let result = indexer
        .add_network(custom_network.clone(), temp_dir.path().to_path_buf())
        .await;

    assert!(result.is_ok());

    let networks = indexer.networks.read().await;
    assert!(networks.contains_key(&custom_network));
}

#[tokio::test]
async fn test_concurrent_network_additions() {
    let temp_dir = TempDir::new().unwrap();
    let indexer = Arc::new(HayateIndexer::new(temp_dir.path().to_path_buf(), 20).unwrap());

    // Add networks concurrently
    let indexer1 = indexer.clone();
    let indexer2 = indexer.clone();
    let indexer3 = indexer.clone();
    let base_path = temp_dir.path().to_path_buf();

    let handle1 = tokio::spawn({
        let base_path = base_path.clone();
        async move { indexer1.add_network(Network::Mainnet, base_path).await }
    });

    let handle2 = tokio::spawn({
        let base_path = base_path.clone();
        async move { indexer2.add_network(Network::Preprod, base_path).await }
    });

    let handle3 = tokio::spawn({
        let base_path = base_path.clone();
        async move { indexer3.add_network(Network::Preview, base_path).await }
    });

    let results = tokio::try_join!(handle1, handle2, handle3);
    assert!(results.is_ok());

    let networks = indexer.networks.read().await;
    assert_eq!(networks.len(), 3);
}

#[tokio::test]
async fn test_duplicate_network_addition() {
    let temp_dir = TempDir::new().unwrap();
    let indexer = Arc::new(HayateIndexer::new(temp_dir.path().to_path_buf(), 20).unwrap());

    // Add same network twice
    indexer
        .add_network(Network::Preprod, temp_dir.path().to_path_buf())
        .await
        .unwrap();

    // Second addition should replace the first
    indexer
        .add_network(Network::Preprod, temp_dir.path().to_path_buf())
        .await
        .unwrap();

    let networks = indexer.networks.read().await;
    assert_eq!(networks.len(), 1);
}

#[tokio::test]
async fn test_indexer_with_different_gap_limits() {
    let temp_dir = TempDir::new().unwrap();

    for gap_limit in [1, 10, 20, 50, 100] {
        let indexer = HayateIndexer::new(temp_dir.path().to_path_buf(), gap_limit);
        assert!(indexer.is_ok());
        assert_eq!(indexer.unwrap().gap_limit, gap_limit);
    }
}

#[tokio::test]
async fn test_network_storage_subdirectories_created() {
    let temp_dir = TempDir::new().unwrap();
    let indexer = Arc::new(HayateIndexer::new(temp_dir.path().to_path_buf(), 20).unwrap());

    indexer
        .add_network(Network::Mainnet, temp_dir.path().to_path_buf())
        .await
        .unwrap();

    // Check that expected subdirectories were created
    let mainnet_path = temp_dir.path().join("mainnet");
    assert!(mainnet_path.exists());
    assert!(mainnet_path.join("utxos").exists());
    assert!(mainnet_path.join("balances").exists());
    assert!(mainnet_path.join("governance").exists());
    assert!(mainnet_path.join("rewards").exists());
    assert!(mainnet_path.join("nonces").exists());
}

#[test]
fn test_network_magic_numbers_valid() {
    // Ensure magic numbers match Cardano specifications
    assert_eq!(Network::Mainnet.magic(), 764824073);
    assert_eq!(Network::Preprod.magic(), 1);
    assert_eq!(Network::Preview.magic(), 2);
    assert_eq!(Network::SanchoNet.magic(), 4);

    // Custom networks should return 0
    let custom = Network::Custom("test".to_string());
    assert_eq!(custom.magic(), 0);
}

#[test]
fn test_network_equality() {
    assert_eq!(Network::Mainnet, Network::Mainnet);
    assert_ne!(Network::Mainnet, Network::Preprod);

    let custom1 = Network::Custom("test".to_string());
    let custom2 = Network::Custom("test".to_string());
    let custom3 = Network::Custom("other".to_string());

    assert_eq!(custom1, custom2);
    assert_ne!(custom1, custom3);
}

#[tokio::test]
async fn test_add_account_to_indexer() {
    let temp_dir = TempDir::new().unwrap();
    let indexer = Arc::new(HayateIndexer::new(temp_dir.path().to_path_buf(), 20).unwrap());

    let xpub = "xpub661MyMwAqRbcFtXgS5sYJABqqG9YLmC4Q1Rdap9gSE8NqtwybGhePY2gZ29ESFjqJoCu1Rupje8YtGqsefD265TMg7usUDFdp6W1EGMcet8".to_string();

    let result = indexer.add_account(xpub.clone()).await;
    assert!(result.is_ok());

    // Verify account was added
    let accounts = indexer.account_xpubs.read().await;
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0], xpub);
}

#[tokio::test]
async fn test_add_multiple_accounts() {
    let temp_dir = TempDir::new().unwrap();
    let indexer = Arc::new(HayateIndexer::new(temp_dir.path().to_path_buf(), 20).unwrap());

    let xpub1 = "xpub1".to_string();
    let xpub2 = "xpub2".to_string();
    let xpub3 = "xpub3".to_string();

    indexer.add_account(xpub1.clone()).await.unwrap();
    indexer.add_account(xpub2.clone()).await.unwrap();
    indexer.add_account(xpub3.clone()).await.unwrap();

    let accounts = indexer.account_xpubs.read().await;
    assert_eq!(accounts.len(), 3);
    assert!(accounts.contains(&xpub1));
    assert!(accounts.contains(&xpub2));
    assert!(accounts.contains(&xpub3));
}
