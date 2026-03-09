// Types for unified transaction building

use crate::wallet::plutus::{DatumOption, PlutusScript, Redeemer};
use crate::wallet::utxorpc_client::{AssetData, UtxoData};

/// Transaction output variants
#[derive(Debug, Clone)]
pub enum TxOutput {
    /// Simple payment to address
    Payment {
        address: Vec<u8>,
        lovelace: u64,
        assets: Vec<AssetData>,
    },

    /// Payment to script with datum
    ScriptPayment {
        address: Vec<u8>,
        lovelace: u64,
        assets: Vec<AssetData>,
        datum: DatumOption,
        script_ref: Option<PlutusScript>,
    },
}

/// Minting operation specification
#[derive(Debug, Clone)]
pub struct MintOperation {
    pub policy_id: [u8; 28],
    pub asset_name: Vec<u8>,
    pub amount: i64, // positive = mint, negative = burn

    // For Plutus policies
    pub policy_script: Option<PlutusScript>,
    pub redeemer: Option<Redeemer>,

    // For native policies
    pub native_script: Option<Vec<u8>>,
}

/// Script input specification
#[derive(Debug, Clone)]
pub struct ScriptInputSpec {
    pub utxo: UtxoData,
    pub script: PlutusScript,
    pub redeemer: Redeemer,
    pub datum: Option<Vec<u8>>, // if not inline
}

/// Fee calculation strategy
#[derive(Debug, Clone, Copy)]
pub enum FeeStrategy {
    /// Use fixed fee amount (lovelace)
    Fixed(u64),

    /// Automatic calculation from protocol parameters
    Automatic,
}

impl Default for FeeStrategy {
    fn default() -> Self {
        FeeStrategy::Automatic
    }
}

/// Built transaction result
#[derive(Debug, Clone)]
pub struct BuiltTransaction {
    /// Transaction CBOR bytes (unsigned)
    pub tx_bytes: Vec<u8>,

    /// Transaction hash
    pub tx_hash: Vec<u8>,

    /// Fee paid (lovelace)
    pub fee_paid: u64,

    /// UTxOs used as inputs
    pub inputs_used: Vec<UtxoData>,

    /// Change amount sent back to wallet (lovelace)
    pub change_amount: u64,

    /// Number of outputs in the transaction
    pub output_count: usize,
}

/// Transaction preview for fee estimation
#[derive(Debug, Clone)]
pub struct TxPreview {
    /// Estimated fee (lovelace)
    pub estimated_fee: u64,

    /// Total input amount (lovelace)
    pub total_input: u64,

    /// Total output amount (lovelace)
    pub total_output: u64,

    /// Estimated change (lovelace)
    pub estimated_change: u64,

    /// Number of inputs needed
    pub inputs_needed: usize,
}
