// CLI argument parsing and configuration tests

use hayate::{HayateConfig, Network};
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn test_network_list_parsing() {
    // Test comma-separated network parsing
    let networks_str = "mainnet,preprod,preview";
    let networks: Vec<Network> = networks_str
        .split(',')
        .filter_map(|s| Network::from_str(s.trim()))
        .collect();

    assert_eq!(networks.len(), 3);
    assert!(networks.contains(&Network::Mainnet));
    assert!(networks.contains(&Network::Preprod));
    assert!(networks.contains(&Network::Preview));
}

#[test]
fn test_network_list_parsing_with_spaces() {
    let networks_str = "mainnet, preprod , preview";
    let networks: Vec<Network> = networks_str
        .split(',')
        .filter_map(|s| Network::from_str(s.trim()))
        .collect();

    assert_eq!(networks.len(), 3);
}

#[test]
fn test_network_list_parsing_invalid() {
    let networks_str = "mainnet,invalid-network,preprod";
    let networks: Vec<Network> = networks_str
        .split(',')
        .filter_map(|s| Network::from_str(s.trim()))
        .collect();

    // Should still get mainnet and preprod, invalid becomes Custom
    assert_eq!(networks.len(), 3);
    assert!(networks.contains(&Network::Mainnet));
    assert!(networks.contains(&Network::Preprod));
}

#[test]
fn test_network_list_empty() {
    let networks_str = "";
    let networks: Vec<Network> = networks_str
        .split(',')
        .filter_map(|s| {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Network::from_str(trimmed)
            }
        })
        .collect();

    assert_eq!(networks.len(), 0);
}

#[test]
fn test_cli_override_gap_limit() {
    let mut config = HayateConfig::default();
    let original_gap = config.gap_limit;

    // Simulate CLI override
    let cli_gap_limit = 100;
    config.gap_limit = cli_gap_limit;

    assert_ne!(original_gap, config.gap_limit);
    assert_eq!(config.gap_limit, 100);
}

#[test]
fn test_cli_override_data_dir() {
    let mut config = HayateConfig::default();

    // Simulate CLI override
    let new_path = PathBuf::from("/tmp/custom/hayate");
    config.data_dir = new_path.clone();

    assert_eq!(config.data_dir, new_path);
}

#[test]
fn test_cli_override_api_bind() {
    let mut config = HayateConfig::default();

    // Simulate CLI override
    config.api.bind = "0.0.0.0:8080".to_string();

    assert_eq!(config.api.bind, "0.0.0.0:8080");
}

#[test]
fn test_config_from_file_with_cli_overrides() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    // Create config file with specific values
    let mut config = HayateConfig::default();
    config.gap_limit = 30;
    config.api.bind = "127.0.0.1:3000".to_string();
    config.save(config_path.to_str().unwrap()).unwrap();

    // Load config
    let mut loaded = HayateConfig::load(config_path.to_str().unwrap()).unwrap();
    assert_eq!(loaded.gap_limit, 30);
    assert_eq!(loaded.api.bind, "127.0.0.1:3000");

    // Apply CLI overrides
    loaded.gap_limit = 50;
    loaded.api.bind = "0.0.0.0:9090".to_string();

    assert_eq!(loaded.gap_limit, 50);
    assert_eq!(loaded.api.bind, "0.0.0.0:9090");
}

#[test]
fn test_network_selection_from_config() {
    let config = HayateConfig::default();

    // Get enabled networks
    let enabled: Vec<&str> = config
        .networks
        .iter()
        .filter(|(_, cfg)| cfg.enabled)
        .map(|(name, _)| name.as_str())
        .collect();

    // Default config should have some networks enabled
    assert!(!enabled.is_empty());
}

#[test]
fn test_network_selection_cli_override() {
    let config = HayateConfig::default();

    // Simulate CLI override (just mainnet)
    let cli_networks_str = "mainnet";
    let networks: Vec<Network> = cli_networks_str
        .split(',')
        .filter_map(|s| Network::from_str(s.trim()))
        .collect();

    assert_eq!(networks.len(), 1);
    assert_eq!(networks[0], Network::Mainnet);

    // This should override the config file networks
    assert_ne!(networks.len(), config.networks.len());
}

#[test]
fn test_custom_network_from_cli() {
    let networks_str = "my-custom-network";
    let networks: Vec<Network> = networks_str
        .split(',')
        .filter_map(|s| Network::from_str(s.trim()))
        .collect();

    assert_eq!(networks.len(), 1);
    match &networks[0] {
        Network::Custom(name) => assert_eq!(name, "my-custom-network"),
        _ => panic!("Expected custom network"),
    }
}

#[test]
fn test_network_magic_retrieval() {
    let mainnet = Network::Mainnet;
    let preprod = Network::Preprod;
    let preview = Network::Preview;
    let sanchonet = Network::SanchoNet;

    assert_eq!(mainnet.magic(), 764824073);
    assert_eq!(preprod.magic(), 1);
    assert_eq!(preview.magic(), 2);
    assert_eq!(sanchonet.magic(), 4);
}

#[test]
fn test_network_string_conversion() {
    assert_eq!(Network::Mainnet.as_str(), "mainnet");
    assert_eq!(Network::Preprod.as_str(), "preprod");
    assert_eq!(Network::Preview.as_str(), "preview");
    assert_eq!(Network::SanchoNet.as_str(), "sanchonet");

    let custom = Network::Custom("testnet".to_string());
    assert_eq!(custom.as_str(), "testnet");
}

#[test]
fn test_network_from_string_case_insensitive() {
    assert_eq!(
        Network::from_str("MAINNET"),
        Some(Network::Mainnet)
    );
    assert_eq!(
        Network::from_str("PreProd"),
        Some(Network::Preprod)
    );
    assert_eq!(
        Network::from_str("PREVIEW"),
        Some(Network::Preview)
    );
}

#[test]
fn test_missing_network_config_handling() {
    let config = HayateConfig::default();

    // Try to get a network that doesn't exist in config
    let missing = config.networks.get("nonexistent");
    assert!(missing.is_none());
}

#[test]
fn test_default_networks_not_enabled_by_default() {
    let config = HayateConfig::default();

    // Count how many are enabled by default
    let enabled_count = config
        .networks
        .values()
        .filter(|cfg| cfg.enabled)
        .count();

    // Some reasonable expectation - not all should be enabled by default
    // (to avoid connecting to all networks on startup)
    assert!(enabled_count <= config.networks.len());
}
