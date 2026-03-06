// Types for Plutus transaction building

use crate::wallet::plutus::{DatumOption, PlutusScript, Redeemer};
use crate::wallet::tx_builder::{TxBuilderError, TxBuilderResult};
use crate::wallet::utxorpc_client::{AssetData, UtxoData};

/// Input for a Plutus transaction
#[derive(Debug, Clone)]
pub enum PlutusInput {
    /// Regular UTxO (vkey witness)
    Regular(UtxoData),
    /// Script UTxO with redeemer
    Script {
        utxo: UtxoData,
        script: PlutusScript,
        redeemer: Redeemer,
        datum: Option<Vec<u8>>, // If datum is not inline
    },
}

impl PlutusInput {
    /// Create a regular input
    pub fn regular(utxo: UtxoData) -> Self {
        PlutusInput::Regular(utxo)
    }

    /// Create a script input
    pub fn script(
        utxo: UtxoData,
        script: PlutusScript,
        redeemer: Redeemer,
        datum: Option<Vec<u8>>,
    ) -> Self {
        PlutusInput::Script {
            utxo,
            script,
            redeemer,
            datum,
        }
    }

    /// Get the underlying UTxO
    pub fn utxo(&self) -> &UtxoData {
        match self {
            PlutusInput::Regular(utxo) => utxo,
            PlutusInput::Script { utxo, .. } => utxo,
        }
    }

    /// Check if this is a script input
    pub fn is_script(&self) -> bool {
        matches!(self, PlutusInput::Script { .. })
    }
}

/// Output for a Plutus transaction
#[derive(Debug, Clone)]
pub struct PlutusOutput {
    /// Recipient address (29 bytes for script address, 57 bytes for payment address)
    pub address: Vec<u8>,
    /// Lovelace amount
    pub lovelace: u64,
    /// Native assets (if any)
    pub assets: Vec<AssetData>,
    /// Optional datum
    pub datum: Option<DatumOption>,
    /// Optional script reference (for future reference inputs)
    pub script_ref: Option<PlutusScript>,
}

impl PlutusOutput {
    /// Create a simple output with just lovelace
    pub fn new(address: Vec<u8>, lovelace: u64) -> Self {
        Self {
            address,
            lovelace,
            assets: Vec::new(),
            datum: None,
            script_ref: None,
        }
    }

    /// Create an output with native assets
    pub fn with_assets(address: Vec<u8>, lovelace: u64, assets: Vec<AssetData>) -> Self {
        Self {
            address,
            lovelace,
            assets,
            datum: None,
            script_ref: None,
        }
    }

    /// Create an output with an inline datum
    pub fn with_inline_datum(address: Vec<u8>, lovelace: u64, datum_bytes: Vec<u8>) -> Self {
        Self {
            address,
            lovelace,
            assets: Vec::new(),
            datum: Some(DatumOption::inline(datum_bytes)),
            script_ref: None,
        }
    }

    /// Create an output with a datum hash
    pub fn with_datum_hash(address: Vec<u8>, lovelace: u64, datum_bytes: &[u8]) -> Self {
        Self {
            address,
            lovelace,
            assets: Vec::new(),
            datum: Some(DatumOption::from_datum_bytes(datum_bytes)),
            script_ref: None,
        }
    }

    /// Add a datum to this output
    pub fn with_datum(mut self, datum: DatumOption) -> Self {
        self.datum = Some(datum);
        self
    }

    /// Add assets to this output
    pub fn add_assets(mut self, assets: Vec<AssetData>) -> Self {
        self.assets = assets;
        self
    }

    /// Add a script reference to this output
    pub fn with_script_ref(mut self, script: PlutusScript) -> Self {
        self.script_ref = Some(script);
        self
    }

    /// Create an output with assets and inline datum (for contract deployment)
    pub fn with_assets_and_inline_datum(
        address: Vec<u8>,
        lovelace: u64,
        assets: Vec<AssetData>,
        datum_bytes: Vec<u8>,
    ) -> Self {
        Self {
            address,
            lovelace,
            assets,
            datum: Some(DatumOption::inline(datum_bytes)),
            script_ref: None,
        }
    }
}

/// Error type for transaction building
#[derive(Debug, thiserror::Error)]
pub enum TransactionBuildError {
    #[error("Insufficient funds: need {need}, have {have}")]
    InsufficientFunds { need: u64, have: u64 },

    #[error("No UTxOs available")]
    NoUtxos,

    #[error("Invalid address: {0}")]
    InvalidAddress(String),

    #[error("Script error: {0}")]
    ScriptError(String),

    #[error("Datum error: {0}")]
    DatumError(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wallet::plutus::{PlutusVersion, Redeemer, RedeemerTag};

    #[test]
    fn test_plutus_input_regular() {
        let utxo = UtxoData {
            tx_hash: vec![0u8; 32],
            output_index: 0,
            address: vec![1u8; 57],
            coin: 10_000_000,
            assets: Vec::new(),
            datum_hash: None,
            datum: None,
        };

        let input = PlutusInput::regular(utxo.clone());
        assert!(!input.is_script());
        assert_eq!(input.utxo().coin, 10_000_000);
    }

    #[test]
    fn test_plutus_input_script() {
        let utxo = UtxoData {
            tx_hash: vec![0u8; 32],
            output_index: 0,
            address: vec![1u8; 29],
            coin: 10_000_000,
            assets: Vec::new(),
            datum_hash: None,
            datum: Some(vec![1, 2, 3]),
        };

        let script = PlutusScript::v2_from_cbor(vec![1, 2, 3, 4]).unwrap();
        let redeemer = Redeemer::empty(RedeemerTag::Spend, 0);

        let input = PlutusInput::script(utxo.clone(), script, redeemer, None);
        assert!(input.is_script());
        assert_eq!(input.utxo().coin, 10_000_000);
    }

    #[test]
    fn test_plutus_output_simple() {
        let output = PlutusOutput::new(vec![1u8; 57], 5_000_000);
        assert_eq!(output.lovelace, 5_000_000);
        assert!(output.assets.is_empty());
        assert!(output.datum.is_none());
    }

    #[test]
    fn test_plutus_output_with_inline_datum() {
        let datum_bytes = vec![1, 2, 3, 4];
        let output = PlutusOutput::with_inline_datum(vec![1u8; 29], 2_000_000, datum_bytes.clone());

        assert!(output.datum.is_some());
        match output.datum.unwrap() {
            DatumOption::Inline(data) => assert_eq!(data, datum_bytes),
            _ => panic!("Expected inline datum"),
        }
    }

    #[test]
    fn test_plutus_output_with_assets() {
        let asset = AssetData {
            policy_id: vec![0u8; 28],
            asset_name: b"TOKEN".to_vec(),
            amount: 100,
        };

        let output = PlutusOutput::with_assets(vec![1u8; 57], 2_000_000, vec![asset.clone()]);
        assert_eq!(output.assets.len(), 1);
        assert_eq!(output.assets[0].amount, 100);
    }

    #[test]
    fn test_plutus_output_builder_pattern() {
        let datum = DatumOption::inline(vec![1, 2, 3]);
        let output = PlutusOutput::new(vec![1u8; 57], 5_000_000)
            .with_datum(datum)
            .add_assets(vec![]);

        assert!(output.datum.is_some());
    }
}
