// Fixture loading utilities for integration tests
// Pre-recorded Cardano blocks for deterministic testing

use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A test fixture containing pre-recorded Cardano blocks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockFixture {
    pub name: String,
    pub description: String,
    pub network: String,
    pub blocks: Vec<RawBlock>,
}

/// Raw block data (simplified representation for testing)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawBlock {
    pub slot: u64,
    pub hash: String,  // hex-encoded
    pub prev_hash: Option<String>,
    pub transactions: Vec<RawTransaction>,
}

/// Raw transaction data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawTransaction {
    pub hash: String,  // hex-encoded
    pub inputs: Vec<RawTxInput>,
    pub outputs: Vec<RawTxOutput>,
    pub fee: u64,
}

/// Raw transaction input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawTxInput {
    pub tx_hash: String,
    pub output_index: u32,
}

/// Raw transaction output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawTxOutput {
    pub address: String,  // hex-encoded
    pub amount: u64,
    pub assets: Vec<(String, String, u64)>,  // (policy_id, asset_name, amount)
    pub datum_hash: Option<String>,
    pub inline_datum: Option<String>,
    pub script_ref: Option<String>,
}

/// Load a fixture from the fixtures directory
pub fn load_fixture(name: &str) -> Result<BlockFixture> {
    let fixture_path = get_fixture_path(name);

    if !fixture_path.exists() {
        // Fixture doesn't exist yet - return empty fixture for now
        // TODO: Record real fixtures from testnet/mainnet
        return Ok(BlockFixture {
            name: name.to_string(),
            description: format!("Mock fixture: {}", name),
            network: "preview".to_string(),
            blocks: generate_mock_blocks(name),
        });
    }

    let content = std::fs::read_to_string(&fixture_path)
        .with_context(|| format!("Failed to read fixture: {}", name))?;

    let fixture: BlockFixture = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse fixture: {}", name))?;

    Ok(fixture)
}

/// Get the path to a fixture file
fn get_fixture_path(name: &str) -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir)
        .join("tests")
        .join("fixtures")
        .join(format!("{}.json", name))
}

/// Generate mock blocks for testing (used when fixtures don't exist yet)
fn generate_mock_blocks(fixture_name: &str) -> Vec<RawBlock> {
    match fixture_name {
        "simple_chain" => generate_simple_chain(10),
        "utxo_lifecycle" => generate_utxo_lifecycle(),
        "rollback_simple" => generate_rollback_chain(),
        "epoch_boundary" => generate_epoch_boundary(),
        "multi_asset_tx" => generate_multi_asset_blocks(),
        "plutus_tx" => generate_plutus_blocks(),
        _ => generate_simple_chain(5),
    }
}

/// Generate a simple chain of blocks
fn generate_simple_chain(num_blocks: u64) -> Vec<RawBlock> {
    let mut blocks = Vec::new();
    let mut prev_hash = None;

    for slot in 0..num_blocks {
        let block_hash = format!("{:064x}", slot);
        let tx_hash = format!("{:064x}", slot * 1000);

        let tx = RawTransaction {
            hash: tx_hash.clone(),
            inputs: if slot > 0 {
                vec![RawTxInput {
                    tx_hash: format!("{:064x}", (slot - 1) * 1000),
                    output_index: 0,
                }]
            } else {
                vec![]  // Genesis
            },
            outputs: vec![
                RawTxOutput {
                    address: format!("01{:056x}", slot),
                    amount: 10_000_000,  // 10 ADA
                    assets: vec![],
                    datum_hash: None,
                    inline_datum: None,
                    script_ref: None,
                },
            ],
            fee: 200_000,
        };

        blocks.push(RawBlock {
            slot,
            hash: block_hash.clone(),
            prev_hash,
            transactions: vec![tx],
        });

        prev_hash = Some(block_hash);
    }

    blocks
}

/// Generate blocks demonstrating UTxO lifecycle (create -> spend)
fn generate_utxo_lifecycle() -> Vec<RawBlock> {
    let addr1 = format!("01{:056x}", 1);
    let addr2 = format!("01{:056x}", 2);

    vec![
        // Block 0: Create UTxO
        RawBlock {
            slot: 1000,
            hash: format!("{:064x}", 1000),
            prev_hash: None,
            transactions: vec![RawTransaction {
                hash: format!("{:064x}", 10000),
                inputs: vec![],
                outputs: vec![
                    RawTxOutput {
                        address: addr1.clone(),
                        amount: 50_000_000,
                        assets: vec![],
                        datum_hash: None,
                        inline_datum: None,
                        script_ref: None,
                    },
                ],
                fee: 200_000,
            }],
        },
        // Block 1: Spend UTxO
        RawBlock {
            slot: 1001,
            hash: format!("{:064x}", 1001),
            prev_hash: Some(format!("{:064x}", 1000)),
            transactions: vec![RawTransaction {
                hash: format!("{:064x}", 10001),
                inputs: vec![RawTxInput {
                    tx_hash: format!("{:064x}", 10000),
                    output_index: 0,
                }],
                outputs: vec![
                    RawTxOutput {
                        address: addr2.clone(),
                        amount: 49_800_000,
                        assets: vec![],
                        datum_hash: None,
                        inline_datum: None,
                        script_ref: None,
                    },
                ],
                fee: 200_000,
            }],
        },
    ]
}

/// Generate a chain with a rollback scenario
fn generate_rollback_chain() -> Vec<RawBlock> {
    let blocks = generate_simple_chain(15);

    // Add metadata to indicate rollback point at block 10
    // In real tests, this would be signaled by the chain sync protocol
    blocks
}

/// Generate blocks around epoch boundary
fn generate_epoch_boundary() -> Vec<RawBlock> {
    // Preview network: epoch boundary every 86400 slots
    let epoch_length = 86400;
    let boundary_slot = epoch_length - 5;

    let mut blocks = Vec::new();
    let mut prev_hash = None;

    for i in 0..10 {
        let slot = boundary_slot + i;
        let block_hash = format!("{:064x}", slot);

        blocks.push(RawBlock {
            slot,
            hash: block_hash.clone(),
            prev_hash,
            transactions: vec![],
        });

        prev_hash = Some(block_hash);
    }

    blocks
}

/// Generate blocks with multi-asset transactions
fn generate_multi_asset_blocks() -> Vec<RawBlock> {
    let addr = format!("01{:056x}", 1);
    let policy_id = format!("{:056x}", 123);

    vec![RawBlock {
        slot: 2000,
        hash: format!("{:064x}", 2000),
        prev_hash: None,
        transactions: vec![RawTransaction {
            hash: format!("{:064x}", 20000),
            inputs: vec![],
            outputs: vec![
                RawTxOutput {
                    address: addr.clone(),
                    amount: 10_000_000,
                    assets: vec![
                        (policy_id.clone(), "TOKEN1".to_string(), 1000),
                        (policy_id.clone(), "TOKEN2".to_string(), 500),
                    ],
                    datum_hash: None,
                    inline_datum: None,
                    script_ref: None,
                },
            ],
            fee: 200_000,
        }],
    }]
}

/// Generate blocks with Plutus transactions (datums, script refs)
fn generate_plutus_blocks() -> Vec<RawBlock> {
    let addr = format!("01{:056x}", 1);
    let datum = format!("{:064x}", 999);
    let script = format!("{:0128x}", 777);

    vec![RawBlock {
        slot: 3000,
        hash: format!("{:064x}", 3000),
        prev_hash: None,
        transactions: vec![RawTransaction {
            hash: format!("{:064x}", 30000),
            inputs: vec![],
            outputs: vec![
                RawTxOutput {
                    address: addr.clone(),
                    amount: 10_000_000,
                    assets: vec![],
                    datum_hash: Some(datum.clone()),
                    inline_datum: Some(datum),
                    script_ref: Some(script),
                },
            ],
            fee: 200_000,
        }],
    }]
}

/// Save a fixture to disk (for recording real blocks)
pub fn save_fixture(fixture: &BlockFixture) -> Result<()> {
    let fixture_path = get_fixture_path(&fixture.name);

    // Create parent directory if it doesn't exist
    if let Some(parent) = fixture_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let content = serde_json::to_string_pretty(fixture)?;
    std::fs::write(&fixture_path, content)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_mock_fixture() {
        let fixture = load_fixture("simple_chain").unwrap();
        assert_eq!(fixture.blocks.len(), 10);
        assert_eq!(fixture.blocks[0].slot, 0);
        assert_eq!(fixture.blocks[9].slot, 9);
    }

    #[test]
    fn test_utxo_lifecycle_fixture() {
        let fixture = load_fixture("utxo_lifecycle").unwrap();
        assert_eq!(fixture.blocks.len(), 2);

        // Block 0: Creates UTxO
        assert_eq!(fixture.blocks[0].transactions.len(), 1);
        assert_eq!(fixture.blocks[0].transactions[0].outputs.len(), 1);

        // Block 1: Spends UTxO
        assert_eq!(fixture.blocks[1].transactions.len(), 1);
        assert_eq!(fixture.blocks[1].transactions[0].inputs.len(), 1);
    }

    #[test]
    fn test_multi_asset_fixture() {
        let fixture = load_fixture("multi_asset_tx").unwrap();
        let first_output = &fixture.blocks[0].transactions[0].outputs[0];
        assert_eq!(first_output.assets.len(), 2);
    }
}
