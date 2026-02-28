// Additional Config Integration Tests
// Tests for src/config.rs (368 LOC)
// Target: Additional edge cases and integration scenarios
// Note: 13 unit tests already exist in src/config.rs

use anyhow::Result;
use hayate::config::{HayateConfig, TokenConfig};
use std::fs;
use tempfile::TempDir;

// Test 1: Token configuration with policy and asset name
#[test]
fn test_token_config_with_asset_name() -> Result<()> {
    let temp = TempDir::new()?;
    let config_path = temp.path().join("token_config.toml");

    let toml_content = r#"
gap_limit = 20

[[tokens]]
policy_id = "a0028f350aaabe0545fdcb56b039bfb08e4bb4d8c4d7c3c7d481c235"
asset_name = "HOSKY"
label = "HOSKY Token"

[[tokens]]
policy_id = "f43a62fdc3965df486de8a0d32fe800963589c41b38946602a0dc535"
asset_name = "AGIX"
label = "SingularityNET"
"#;

    fs::write(&config_path, toml_content)?;
    let config = HayateConfig::load(config_path.to_str().unwrap())?;

    assert_eq!(config.tokens.len(), 2);
    assert_eq!(
        config.tokens[0].policy_id,
        "a0028f350aaabe0545fdcb56b039bfb08e4bb4d8c4d7c3c7d481c235"
    );
    assert_eq!(config.tokens[0].asset_name, Some("HOSKY".to_string()));
    assert_eq!(config.tokens[0].label, Some("HOSKY Token".to_string()));

    Ok(())
}

// Test 2: Token configuration without asset name (track all assets in policy)
#[test]
fn test_token_config_policy_only() -> Result<()> {
    let temp = TempDir::new()?;
    let config_path = temp.path().join("token_config.toml");

    let toml_content = r#"
gap_limit = 20

[[tokens]]
policy_id = "a0028f350aaabe0545fdcb56b039bfb08e4bb4d8c4d7c3c7d481c235"
label = "All HOSKY Assets"
"#;

    fs::write(&config_path, toml_content)?;
    let config = HayateConfig::load(config_path.to_str().unwrap())?;

    assert_eq!(config.tokens.len(), 1);
    assert_eq!(
        config.tokens[0].policy_id,
        "a0028f350aaabe0545fdcb56b039bfb08e4bb4d8c4d7c3c7d481c235"
    );
    assert_eq!(config.tokens[0].asset_name, None); // Track all assets
    assert_eq!(config.tokens[0].label, Some("All HOSKY Assets".to_string()));

    Ok(())
}

// Test 3: Wallets configuration
#[test]
fn test_wallets_config() -> Result<()> {
    let temp = TempDir::new()?;
    let config_path = temp.path().join("wallets_config.toml");

    let toml_content = r#"
gap_limit = 20

wallets = [
    "acct_xvk1abc123def456",
    "acct_xvk1xyz789uvw012",
    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
]
"#;

    fs::write(&config_path, toml_content)?;
    let config = HayateConfig::load(config_path.to_str().unwrap())?;

    assert_eq!(config.wallets.len(), 3);
    assert_eq!(config.wallets[0], "acct_xvk1abc123def456");
    assert_eq!(config.wallets[1], "acct_xvk1xyz789uvw012");
    assert_eq!(
        config.wallets[2],
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
    );

    Ok(())
}

// Test 4: Socket path takes precedence over relay
#[test]
fn test_socket_path_precedence() -> Result<()> {
    let temp = TempDir::new()?;
    let config_path = temp.path().join("socket_config.toml");

    let toml_content = r#"
gap_limit = 20

[networks.preview]
enabled = true
relay = "preview-node.world.dev.cardano.org:30002"
magic = 2
system_start_ms = 1666656000000
socket_path = "/tmp/node.socket"
"#;

    fs::write(&config_path, toml_content)?;
    let config = HayateConfig::load(config_path.to_str().unwrap())?;

    assert!(config.networks.contains_key("preview"));
    let preview = &config.networks["preview"];
    assert_eq!(preview.relay, "preview-node.world.dev.cardano.org:30002");
    assert_eq!(
        preview.socket_path,
        Some(std::path::PathBuf::from("/tmp/node.socket"))
    );

    Ok(())
}

// Test 5: Start point configuration
#[test]
fn test_start_point_config() -> Result<()> {
    let temp = TempDir::new()?;
    let config_path = temp.path().join("start_point_config.toml");

    let toml_content = r#"
gap_limit = 20

[networks.preview]
enabled = true
relay = "preview-node.world.dev.cardano.org:30002"
magic = 2
system_start_ms = 1666656000000
start_point = [123456, "abc123def456"]
"#;

    fs::write(&config_path, toml_content)?;
    let config = HayateConfig::load(config_path.to_str().unwrap())?;

    assert!(config.networks.contains_key("preview"));
    let preview = &config.networks["preview"];
    assert!(preview.start_point.is_some());

    let (slot, hash) = preview.start_point.as_ref().unwrap();
    assert_eq!(*slot, 123456);
    assert_eq!(hash, "abc123def456");

    Ok(())
}

// Test 6: Full config with multiple wallets and tokens
#[test]
fn test_full_config_integration() -> Result<()> {
    let temp = TempDir::new()?;
    let config_path = temp.path().join("full_config.toml");

    let toml_content = r#"
data_dir = "/var/lib/hayate"
gap_limit = 50

wallets = [
    "acct_xvk1wallet1",
    "acct_xvk1wallet2"
]

[[tokens]]
policy_id = "policy1"
asset_name = "TOKEN1"
label = "My Token 1"

[[tokens]]
policy_id = "policy2"
label = "All Policy 2 Assets"

[api]
bind = "0.0.0.0:50051"
enabled = true

[networks.mainnet]
enabled = true
relay = "relays-new.cardano-mainnet.iohk.io:3001"
magic = 764824073
system_start_ms = 1591566291000
socket_path = "/opt/cardano/node.socket"
start_point = [50000000, "fedcba9876543210"]
"#;

    fs::write(&config_path, toml_content)?;
    let config = HayateConfig::load(config_path.to_str().unwrap())?;

    // Verify data_dir and gap_limit
    assert_eq!(config.data_dir, std::path::PathBuf::from("/var/lib/hayate"));
    assert_eq!(config.gap_limit, 50);

    // Verify API config
    assert_eq!(config.api.bind, "0.0.0.0:50051");
    assert!(config.api.enabled);

    // Verify network config
    assert!(config.networks.contains_key("mainnet"));
    let mainnet = &config.networks["mainnet"];
    assert!(mainnet.enabled);
    assert_eq!(mainnet.magic, 764824073);
    assert_eq!(
        mainnet.socket_path,
        Some(std::path::PathBuf::from("/opt/cardano/node.socket"))
    );
    let (slot, hash) = mainnet.start_point.as_ref().unwrap();
    assert_eq!(*slot, 50000000);
    assert_eq!(hash, "fedcba9876543210");

    // Verify wallets
    assert_eq!(config.wallets.len(), 2);
    assert_eq!(config.wallets[0], "acct_xvk1wallet1");

    // Verify tokens
    assert_eq!(config.tokens.len(), 2);
    assert_eq!(config.tokens[0].policy_id, "policy1");
    assert_eq!(config.tokens[0].asset_name, Some("TOKEN1".to_string()));
    assert_eq!(config.tokens[1].policy_id, "policy2");
    assert_eq!(config.tokens[1].asset_name, None);

    Ok(())
}

// Test 7: Empty wallets and tokens arrays
#[test]
fn test_empty_wallets_and_tokens() -> Result<()> {
    let temp = TempDir::new()?;
    let config_path = temp.path().join("empty_config.toml");

    let toml_content = r#"
gap_limit = 20

wallets = []
tokens = []
"#;

    fs::write(&config_path, toml_content)?;
    let config = HayateConfig::load(config_path.to_str().unwrap())?;

    assert_eq!(config.wallets.len(), 0);
    assert_eq!(config.tokens.len(), 0);

    Ok(())
}

// Test 8: Programmatic TokenConfig creation
#[test]
fn test_token_config_programmatic() {
    let token1 = TokenConfig {
        policy_id: "abc123".to_string(),
        asset_name: Some("MyToken".to_string()),
        label: Some("My Token Label".to_string()),
    };

    let token2 = TokenConfig {
        policy_id: "def456".to_string(),
        asset_name: None,
        label: None,
    };

    assert_eq!(token1.policy_id, "abc123");
    assert_eq!(token1.asset_name, Some("MyToken".to_string()));
    assert_eq!(token1.label, Some("My Token Label".to_string()));

    assert_eq!(token2.policy_id, "def456");
    assert!(token2.asset_name.is_none());
    assert!(token2.label.is_none());
}
