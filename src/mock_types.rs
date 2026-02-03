// Mock Cardano types for wallet indexer testing
// Simplified representations for development and testing

#![allow(dead_code)]

use serde::{Serialize, Deserialize};
use std::collections::HashMap;

/// Block hash
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlockHash(pub String);

/// Transaction hash  
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TxHash(pub String);

/// Cardano address
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Address(pub String);

impl Address {
    pub fn new(addr: &str) -> Self {
        Address(addr.to_string())
    }
    
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Asset (policy ID + asset name)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Asset {
    pub policy_id: String,
    pub asset_name: String,
}

/// UTxO (Unspent Transaction Output)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Utxo {
    pub tx_hash: TxHash,
    pub output_index: u16,
    pub address: Address,
    pub amount: u64,  // Lovelace
    pub assets: HashMap<Asset, u64>,  // Multi-asset tokens
    pub datum: Option<Vec<u8>>,
}

/// Transaction input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxInput {
    pub tx_hash: TxHash,
    pub output_index: u16,
}

/// Transaction output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxOutput {
    pub address: Address,
    pub amount: u64,
    pub assets: HashMap<Asset, u64>,
    pub datum: Option<Vec<u8>>,
}

/// Transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub hash: TxHash,
    pub inputs: Vec<TxInput>,
    pub outputs: Vec<TxOutput>,
    pub fee: u64,
}

/// Block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub height: u64,
    pub hash: BlockHash,
    pub prev_hash: Option<BlockHash>,
    pub transactions: Vec<Transaction>,
}

/// Governance action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GovernanceAction {
    ParameterChange { parameters: HashMap<String, String> },
    HardForkInitiation { protocol_version: (u64, u64) },
    TreasuryWithdrawal { amount: u64, recipient: Address },
    NoConfidence,
    NewCommittee { members: Vec<String> },
}

impl GovernanceAction {
    pub fn action_id(&self) -> String {
        match self {
            Self::ParameterChange { .. } => "param_change".to_string(),
            Self::HardForkInitiation { .. } => "hard_fork".to_string(),
            Self::TreasuryWithdrawal { .. } => "treasury".to_string(),
            Self::NoConfidence => "no_confidence".to_string(),
            Self::NewCommittee { .. } => "new_committee".to_string(),
        }
    }
}

// Test data generators

/// Generate mock blocks for testing
pub fn generate_mock_blocks(num_blocks: u64, txs_per_block: usize) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut prev_hash = None;
    
    for height in 0..num_blocks {
        let block_hash = BlockHash(format!("block_{:08x}", height));
        let mut transactions = Vec::new();
        
        for tx_idx in 0..txs_per_block {
            let tx_hash = TxHash(format!("tx_{}_{}", height, tx_idx));
            
            // Simple transaction: 1 input, 2 outputs (payment + change)
            let inputs = vec![TxInput {
                tx_hash: TxHash(format!("prev_tx_{}", height.saturating_sub(1))),
                output_index: 0,
            }];
            
            let outputs = vec![
                TxOutput {
                    address: Address::new(&format!("addr1_recipient_{}", tx_idx)),
                    amount: 1_000_000,  // 1 ADA
                    assets: HashMap::new(),
                    datum: None,
                },
                TxOutput {
                    address: Address::new(&format!("addr1_sender_{}", tx_idx)),
                    amount: 8_000_000,  // 8 ADA change
                    assets: HashMap::new(),
                    datum: None,
                },
            ];
            
            transactions.push(Transaction {
                hash: tx_hash,
                inputs,
                outputs,
                fee: 200_000,  // 0.2 ADA
            });
        }
        
        blocks.push(Block {
            height,
            hash: block_hash.clone(),
            prev_hash,
            transactions,
        });
        
        prev_hash = Some(block_hash);
    }
    
    blocks
}
