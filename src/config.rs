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

    /// Wallet xpubs to index (account-level extended public keys)
    /// Can be hex-encoded or bech32-encoded (acct_xvk...)
    #[serde(default)]
    pub wallets: Vec<String>,

    /// Native tokens to track
    /// Indexes all transactions containing these tokens, regardless of wallet
    #[serde(default)]
    pub tokens: Vec<TokenConfig>,

    /// Arbitrary addresses to index (bech32 format)
    /// Useful for tracking script addresses or external addresses
    #[serde(default)]
    pub addresses: Vec<String>,
}

/// Configuration for tracking a native token
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TokenConfig {
    /// Policy ID (hex-encoded)
    pub policy_id: String,

    /// Optional: Asset name (UTF-8 string)
    /// If None, tracks all assets under this policy
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_name: Option<String>,

    /// Optional: Human-friendly label for this token
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
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

    /// Node-to-client connection (host:port or unix socket path)
    /// Examples: "localhost:3001" or "/path/to/node.socket"
    pub relay: String,

    /// Network magic number
    pub magic: u64,

    /// System start time (Unix timestamp in milliseconds)
    /// This is the network's genesis time used for slot-to-timestamp conversion
    pub system_start_ms: u64,

    /// Optional: Path to genesis file for custom networks
    pub genesis_file: Option<PathBuf>,

    /// Optional: Starting point (slot, hash) for sync
    pub start_point: Option<(u64, String)>,

    /// Optional: Unix socket path (alternative to relay)
    /// When set, this takes precedence over relay field
    pub socket_path: Option<PathBuf>,
}

fn default_enabled() -> bool {
    false
}

impl Default for HayateConfig {
    fn default() -> Self {
        let mut networks = HashMap::new();
        
        // Mainnet - Shelley mainnet launch (July 29, 2020 21:44:51 UTC)
        networks.insert("mainnet".to_string(), NetworkConfig {
            enabled: false,
            relay: "relays-new.cardano-mainnet.iohk.io:3001".to_string(),
            magic: 764824073,
            system_start_ms: 1591566291000,
            genesis_file: None,
            start_point: None,
            socket_path: None,
        });

        // Preprod - (June 1, 2022 00:00:00 UTC)
        networks.insert("preprod".to_string(), NetworkConfig {
            enabled: true,
            relay: "preprod-node.world.dev.cardano.org:30000".to_string(),
            magic: 1,
            system_start_ms: 1654041600000,
            genesis_file: None,
            start_point: None,
            socket_path: None,
        });

        // Preview - (June 1, 2022 00:00:00 UTC)
        networks.insert("preview".to_string(), NetworkConfig {
            enabled: false,
            relay: "preview-node.world.dev.cardano.org:30002".to_string(),
            magic: 2,
            system_start_ms: 1666656000000,
            genesis_file: None,
            start_point: None,
            socket_path: None,
        });

        // SanchoNet - (June 15, 2023 00:30:00 UTC)
        networks.insert("sanchonet".to_string(), NetworkConfig {
            enabled: false,
            relay: "sanchonet-node.world.dev.cardano.org:30004".to_string(),
            magic: 4,
            system_start_ms: 1686790200000,
            genesis_file: None,
            start_point: None,
            socket_path: None,
        });
        
        Self {
            data_dir: default_data_dir(),
            gap_limit: default_gap_limit(),
            api: ApiConfig::default(),
            networks,
            wallets: Vec::new(),
            tokens: Vec::new(),
            addresses: Vec::new(),
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
        system_start_ms: u64,
        genesis_file: Option<PathBuf>,
    ) {
        self.networks.insert(name.clone(), NetworkConfig {
            enabled: true,
            relay,
            magic,
            system_start_ms,
            genesis_file,
            start_point: None,
            socket_path: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

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
            1654041600000, // system_start_ms
            Some(PathBuf::from("./genesis.json")),
        );

        assert!(config.networks.contains_key("my-testnet"));
        assert_eq!(config.networks["my-testnet"].magic, 42);
    }

    #[test]
    fn test_save_and_load_config() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test_config.toml");

        // Create and save a config
        let mut config = HayateConfig::default();
        config.gap_limit = 42;
        config.data_dir = PathBuf::from("/custom/path");
        config.save(config_path.to_str().unwrap()).unwrap();

        // Load it back
        let loaded = HayateConfig::load(config_path.to_str().unwrap()).unwrap();
        assert_eq!(loaded.gap_limit, 42);
        assert_eq!(loaded.data_dir, PathBuf::from("/custom/path"));
        assert_eq!(loaded.api.bind, config.api.bind);
    }

    #[test]
    fn test_load_nonexistent_file() {
        let result = HayateConfig::load("/nonexistent/path/config.toml");
        assert!(result.is_err());
    }

    #[test]
    fn test_load_invalid_toml() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("invalid.toml");

        // Write invalid TOML
        fs::write(&config_path, "this is { not [ valid toml").unwrap();

        let result = HayateConfig::load(config_path.to_str().unwrap());
        assert!(result.is_err());
    }

    #[test]
    fn test_load_partial_config() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("partial.toml");

        // Write minimal valid TOML (should use defaults for missing fields)
        fs::write(&config_path, r#"
gap_limit = 50

[api]
bind = "0.0.0.0:9090"
"#).unwrap();

        let loaded = HayateConfig::load(config_path.to_str().unwrap()).unwrap();
        assert_eq!(loaded.gap_limit, 50);
        assert_eq!(loaded.api.bind, "0.0.0.0:9090");
        // Partial config without networks section will have empty networks
        assert_eq!(loaded.networks.len(), 0);
        // Data dir should use default
        assert_eq!(loaded.data_dir, PathBuf::from("./hayate-db"));
    }

    #[test]
    fn test_generate_default_config() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("generated.toml");

        HayateConfig::generate_default(config_path.to_str().unwrap()).unwrap();

        // Verify file was created and is valid
        assert!(config_path.exists());
        let loaded = HayateConfig::load(config_path.to_str().unwrap()).unwrap();
        assert_eq!(loaded.gap_limit, 20);
    }

    #[test]
    fn test_network_config_relay_formats() {
        let config = HayateConfig::default();

        // Mainnet should have valid relay
        let mainnet = &config.networks["mainnet"];
        assert!(mainnet.relay.contains(":"));
        assert!(mainnet.relay.contains("relays-new.cardano-mainnet.iohk.io")
             || mainnet.relay.contains("backbone.cardano.iog.io")
             || mainnet.relay.starts_with("localhost"));

        // Magic numbers should be correct
        assert_eq!(mainnet.magic, 764824073);
        assert_eq!(config.networks["preprod"].magic, 1);
        assert_eq!(config.networks["preview"].magic, 2);
        assert_eq!(config.networks["sanchonet"].magic, 4);
    }

    #[test]
    fn test_all_default_networks_present() {
        let config = HayateConfig::default();
        let networks: Vec<&str> = config.networks.keys().map(|s| s.as_str()).collect();

        assert!(networks.contains(&"mainnet"));
        assert!(networks.contains(&"preprod"));
        assert!(networks.contains(&"preview"));
        assert!(networks.contains(&"sanchonet"));
    }

    #[test]
    fn test_api_config_defaults() {
        let config = HayateConfig::default();

        // API should bind to localhost by default
        assert!(config.api.bind.starts_with("127.0.0.1")
             || config.api.bind.starts_with("0.0.0.0"));
        assert!(config.api.bind.contains(":"));
    }

    #[test]
    fn test_data_dir_default() {
        let config = HayateConfig::default();

        // Should have a default data directory
        assert!(!config.data_dir.as_os_str().is_empty());
    }
}
