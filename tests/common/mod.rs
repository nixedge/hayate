// Common test infrastructure for Hayate integration tests

use anyhow::Result;
use std::path::PathBuf;
use tempfile::TempDir;

pub mod fixtures;

// Re-export commonly used types
pub use hayate::indexer::{NetworkStorage, StorageHandle, StorageManager};
pub use hayate::indexer::storage_manager::StorageCommand;
pub use hayate::indexer::Network;

/// Create a test storage with temporary directory
/// Returns (TempDir, StorageHandle)
/// The TempDir must be kept alive for the duration of the test
pub async fn create_test_storage(network: Network) -> Result<(TempDir, StorageHandle)> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().to_path_buf();

    // Create storage
    let storage = NetworkStorage::open(db_path.clone(), network)?;

    // Create storage manager
    let (manager, handle) = StorageManager::new(storage);

    // Spawn storage manager task
    tokio::spawn(async move {
        manager.run().await;
    });

    Ok((temp_dir, handle))
}

/// Create test storage with a specific database path
pub async fn create_test_storage_at_path(db_path: PathBuf, network: Network) -> Result<StorageHandle> {
    std::fs::create_dir_all(&db_path)?;

    let storage = NetworkStorage::open(db_path, network)?;
    let (manager, handle) = StorageManager::new(storage);

    tokio::spawn(async move {
        manager.run().await;
    });

    Ok(handle)
}

/// Generate a mock address (hex-encoded bytes)
pub fn generate_mock_address(network: Network, index: u32) -> String {
    // Simple mock address: network byte + 28 random bytes
    let network_byte = match network {
        Network::Mainnet => 0x01,
        Network::Preprod => 0x00,
        Network::Preview => 0x00,
        Network::SanchoNet => 0x00,
        Network::Custom(_) => 0xFF,
    };

    let mut bytes = vec![network_byte];
    // Use index to generate deterministic "random" bytes
    for i in 0..28 {
        bytes.push(((index * 7 + i * 13) % 256) as u8);
    }

    hex::encode(bytes)
}

/// Generate a mock transaction hash
pub fn generate_mock_tx_hash(index: u32) -> String {
    let mut bytes = vec![0u8; 32];
    let index_bytes = index.to_le_bytes();
    bytes[0..4].copy_from_slice(&index_bytes);
    hex::encode(bytes)
}

/// Generate a mock UTxO key (tx_hash#output_index format)
pub fn generate_mock_utxo_key(tx_index: u32, output_index: u32) -> String {
    format!("{}#{}", generate_mock_tx_hash(tx_index), output_index)
}

/// Generate mock UTxO data (simplified serialized format)
pub fn generate_mock_utxo_data(address: &str, amount: u64) -> Vec<u8> {
    // Simple format: address_hex + amount (little-endian u64)
    let mut data = hex::decode(address).unwrap_or_default();
    data.extend_from_slice(&amount.to_le_bytes());
    data
}

/// Generate mock UTxO data as JSON (for API tests)
pub fn generate_mock_utxo_json(tx_hash: &str, output_index: u32, address: &str, amount: u64) -> Vec<u8> {
    let json = serde_json::json!({
        "tx_hash": tx_hash,
        "output_index": output_index,
        "address": address,
        "amount": amount,
        "assets": {}
    });
    serde_json::to_vec(&json).unwrap()
}

/// Wait for a condition to be true (with timeout)
pub async fn wait_for_condition<F>(mut check: F, timeout_ms: u64) -> Result<()>
where
    F: FnMut() -> bool,
{
    let start = tokio::time::Instant::now();
    let timeout = tokio::time::Duration::from_millis(timeout_ms);

    loop {
        if check() {
            return Ok(());
        }

        if start.elapsed() > timeout {
            anyhow::bail!("Timeout waiting for condition");
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_mock_address() {
        let addr1 = generate_mock_address(Network::Mainnet, 0);
        let addr2 = generate_mock_address(Network::Mainnet, 1);
        let addr3 = generate_mock_address(Network::Mainnet, 0);

        // Same index = same address
        assert_eq!(addr1, addr3);

        // Different index = different address
        assert_ne!(addr1, addr2);

        // Mainnet addresses start with 01
        assert!(addr1.starts_with("01"));
    }

    #[test]
    fn test_generate_mock_tx_hash() {
        let tx1 = generate_mock_tx_hash(0);
        let tx2 = generate_mock_tx_hash(1);
        let tx3 = generate_mock_tx_hash(0);

        assert_eq!(tx1, tx3);
        assert_ne!(tx1, tx2);
        assert_eq!(tx1.len(), 64); // 32 bytes = 64 hex chars
    }

    #[test]
    fn test_generate_mock_utxo_key() {
        let key = generate_mock_utxo_key(123, 0);
        assert!(key.contains("#0"));
    }
}
