// Configuration for Hayate indexer
// Supports standard networks and custom networks with genesis files

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HayateConfig {
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
    
    #[serde(default = "default_gap_limit")]
    pub gap_limit: u32,
    
    #[serde(default)]
    pub api: ApiConfig,
    
    #[serde(default)]
    pub networks: HashMap<String, NetworkConfig>,
}

fn default_data_dir() -> PathBuf {
    PathBuf::from("./hayate-db")
}

fn default_gap_limit() -> u32 {
    20
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApiConfig {
    #[serde(default = "default_api_bind")]
    pub bind: String,
    
    #[serde(default = "default_api_enabled")]
    pub enabled: bool,
}

fn default_api_bind() -> String {
    "127.0.0.1:50051".to_string()
}

fn default_api_enabled() -> bool {
    true
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            bind: default_api_bind(),
            enabled: true,
        }
    }
}

/// Configuration for a Cardano network
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NetworkConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    
    /// Node-to-client connection (host:port)
    pub relay: String,
    
    /// Network magic number
    pub magic: u64,
    
    /// Optional: Path to genesis file for custom networks
    pub genesis_file: Option<PathBuf>,
    
    /// Optional: Starting point (slot, hash) for sync
    pub start_point: Option<(u64, String)>,
}

fn default_enabled() -> bool {
    false
}

impl Default for HayateConfig {
    fn default() -> Self {
        let mut networks = HashMap::new();
        
        // Mainnet
        networks.insert("mainnet".to_string(), NetworkConfig {
            enabled: false,
            relay: "relays-new.cardano-mainnet.iohk.io:3001".to_string(),
            magic: 764824073,
            genesis_file: None,
            start_point: None,
        });
        
        // Preprod
        networks.insert("preprod".to_string(), NetworkConfig {
            enabled: true,
            relay: "preprod-node.world.dev.cardano.org:30000".to_string(),
            magic: 1,
            genesis_file: None,
            start_point: None,
        });
        
        // Preview
        networks.insert("preview".to_string(), NetworkConfig {
            enabled: false,
            relay: "preview-node.world.dev.cardano.org:30002".to_string(),
            magic: 2,
            genesis_file: None,
            start_point: None,
        });
        
        // SanchoNet - for Mike! 🎉
        networks.insert("sanchonet".to_string(), NetworkConfig {
            enabled: false,
            relay: "sanchonet-node.world.dev.cardano.org:30004".to_string(),
            magic: 4,
            genesis_file: None,
            start_point: None,
        });
        
        Self {
            data_dir: default_data_dir(),
            gap_limit: default_gap_limit(),
            api: ApiConfig::default(),
            networks,
        }
    }
}

impl HayateConfig {
    /// Load configuration from TOML file
    pub fn load(path: &str) -> anyhow::Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let config: HayateConfig = toml::from_str(&contents)?;
        Ok(config)
    }
    
    /// Save configuration to TOML file
    pub fn save(&self, path: &str) -> anyhow::Result<()> {
        let contents = toml::to_string_pretty(self)?;
        std::fs::write(path, contents)?;
        Ok(())
    }
    
    /// Generate default config file
    pub fn generate_default(path: &str) -> anyhow::Result<()> {
        let config = HayateConfig::default();
        config.save(path)?;
        Ok(())
    }
    
    /// Add a custom network
    pub fn add_custom_network(
        &mut self,
        name: String,
        relay: String,
        magic: u64,
        genesis_file: Option<PathBuf>,
    ) {
        self.networks.insert(name.clone(), NetworkConfig {
            enabled: true,
            relay,
            magic,
            genesis_file,
            start_point: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_config() {
        let config = HayateConfig::default();
        assert_eq!(config.gap_limit, 20);
        assert!(config.networks.contains_key("mainnet"));
        assert!(config.networks.contains_key("preprod"));
        assert!(config.networks.contains_key("preview"));
        assert!(config.networks.contains_key("sanchonet"));
    }
    
    #[test]
    fn test_custom_network() {
        let mut config = HayateConfig::default();
        config.add_custom_network(
            "my-testnet".to_string(),
            "localhost:3001".to_string(),
            42,
            Some(PathBuf::from("./genesis.json")),
        );
        
        assert!(config.networks.contains_key("my-testnet"));
        assert_eq!(config.networks["my-testnet"].magic, 42);
    }
}
