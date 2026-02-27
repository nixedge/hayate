// Common test utilities for Hayate integration tests

use std::path::PathBuf;
use std::sync::Arc;
use anyhow::Result;

/// Test configuration for integration tests
pub struct TestConfig {
    pub network: hayate::indexer::Network,
    pub data_dir: PathBuf,
    pub grpc_port: u16,
}

impl TestConfig {
    pub fn sanchonet() -> Self {
        Self {
            network: hayate::indexer::Network::SanchoNet,
            data_dir: PathBuf::from("./test-data/sanchonet"),
            grpc_port: 50052,
        }
    }

    pub fn preview() -> Self {
        Self {
            network: hayate::indexer::Network::Preview,
            data_dir: PathBuf::from("./test-data/preview"),
            grpc_port: 50053,
        }
    }
}

/// Setup test environment
pub async fn setup_test_indexer(config: TestConfig) -> Result<Arc<hayate::indexer::HayateIndexer>> {
    // Create test data directory
    std::fs::create_dir_all(&config.data_dir)?;

    // Initialize indexer
    let indexer = hayate::indexer::HayateIndexer::new(config.data_dir.clone(), 20)?;
    let indexer = Arc::new(indexer);

    // Add network storage
    indexer.add_network(config.network, config.data_dir).await?;

    Ok(indexer)
}

/// Cleanup test data
pub fn cleanup_test_data(data_dir: &PathBuf) -> Result<()> {
    if data_dir.exists() {
        std::fs::remove_dir_all(data_dir)?;
    }
    Ok(())
}

/// Wait for indexer to sync to a specific height
pub async fn wait_for_sync(
    indexer: &hayate::indexer::HayateIndexer,
    network: &hayate::indexer::Network,
    _target_height: u64,
    _timeout_secs: u64,
) -> Result<()> {
    // Check if network is available
    let networks = indexer.networks.read().await;
    if !networks.contains_key(network) {
        anyhow::bail!("Network not configured: {:?}", network);
    }

    // TODO: Implement sync waiting logic
    // For now, just verify network is accessible
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_creation() {
        let config = TestConfig::sanchonet();
        assert_eq!(config.network, hayate::indexer::Network::SanchoNet);
        assert_eq!(config.grpc_port, 50052);
    }
}
