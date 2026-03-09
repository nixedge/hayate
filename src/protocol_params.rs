// Cardano Protocol Parameters
//
// Provides access to current protocol parameters for fee calculation,
// min-utxo requirements, and Plutus execution pricing.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProtocolParamError {
    #[error("Failed to query protocol parameters: {0}")]
    QueryFailed(String),

    #[error("Invalid protocol parameter format: {0}")]
    InvalidFormat(String),

    #[error("Network not supported: {0}")]
    UnsupportedNetwork(String),
}

pub type Result<T> = std::result::Result<T, ProtocolParamError>;

/// Plutus execution units (memory and CPU steps)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ExUnits {
    pub mem: u64,
    pub steps: u64,
}

/// Rational number represented as numerator/denominator
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Rational {
    pub numerator: u64,
    pub denominator: u64,
}

impl Rational {
    pub fn new(numerator: u64, denominator: u64) -> Self {
        Self {
            numerator,
            denominator,
        }
    }

    pub fn to_f64(&self) -> f64 {
        self.numerator as f64 / self.denominator as f64
    }
}

/// Cardano protocol parameters
///
/// These parameters define the rules for transaction fees, UTxO constraints,
/// and Plutus script execution costs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolParameters {
    // === Fee Parameters ===
    /// Linear coefficient for fee calculation (minFeeA)
    /// Mainnet: 44, Preprod: 44, Preview: 44
    pub min_fee_a: u64,

    /// Constant base fee (minFeeB)
    /// Mainnet: 155,381, Preprod: 155,381, Preview: 155,381
    pub min_fee_b: u64,

    /// Maximum transaction size in bytes
    pub max_tx_size: u64,

    /// Maximum block body size in bytes
    pub max_block_body_size: u64,

    // === UTxO Parameters ===
    /// Cost per byte of UTxO storage (ada_per_utxo_byte in Conway)
    /// Used to calculate minimum ADA for outputs
    pub utxo_cost_per_byte: u64,

    /// Legacy minimum UTxO value (deprecated in favor of utxo_cost_per_byte)
    pub min_utxo_lovelace: Option<u64>,

    // === Plutus Parameters ===
    /// Price per unit of memory for Plutus scripts
    pub price_memory: Option<Rational>,

    /// Price per CPU step for Plutus scripts
    pub price_steps: Option<Rational>,

    /// Maximum execution units per transaction
    pub max_tx_execution_units: Option<ExUnits>,

    /// Maximum execution units per block
    pub max_block_execution_units: Option<ExUnits>,

    // === Stake Parameters ===
    /// Deposit required for stake key registration (lovelace)
    pub key_deposit: u64,

    /// Deposit required for pool registration (lovelace)
    pub pool_deposit: u64,

    /// Minimum fixed cost per pool per epoch (lovelace)
    pub min_pool_cost: u64,

    // === Metadata ===
    /// Epoch these parameters are valid for
    pub epoch: u64,
}

impl ProtocolParameters {
    /// Calculate minimum fee for a transaction
    ///
    /// Formula: min_fee_a * tx_size_bytes + min_fee_b
    ///
    /// # Arguments
    /// * `tx_size_bytes` - Transaction size in bytes
    ///
    /// # Returns
    /// Minimum fee in lovelace
    pub fn calculate_min_fee(&self, tx_size_bytes: u64) -> u64 {
        self.min_fee_a
            .saturating_mul(tx_size_bytes)
            .saturating_add(self.min_fee_b)
    }

    /// Calculate minimum ADA required for a UTxO
    ///
    /// Uses the utxo_cost_per_byte parameter to calculate the minimum
    /// ADA required to store a UTxO of the given size.
    ///
    /// Formula: utxo_cost_per_byte * output_size_bytes
    ///
    /// # Arguments
    /// * `output_size_bytes` - Size of the output in bytes
    ///
    /// # Returns
    /// Minimum lovelace required for the output
    pub fn calculate_min_utxo(&self, output_size_bytes: u64) -> u64 {
        // Use utxo_cost_per_byte if available, otherwise fall back to min_utxo_lovelace
        if let Some(min_utxo) = self.min_utxo_lovelace {
            min_utxo.max(self.utxo_cost_per_byte.saturating_mul(output_size_bytes))
        } else {
            self.utxo_cost_per_byte.saturating_mul(output_size_bytes)
        }
    }

    /// Calculate Plutus execution cost
    ///
    /// Formula: (mem * price_memory) + (steps * price_steps)
    ///
    /// # Arguments
    /// * `ex_units` - Execution units (memory and steps)
    ///
    /// # Returns
    /// Cost in lovelace, or None if Plutus pricing is not available
    pub fn calculate_execution_cost(&self, ex_units: &ExUnits) -> Option<u64> {
        let mem_price = self.price_memory?;
        let step_price = self.price_steps?;

        // Calculate cost as (mem * price_mem) + (steps * price_steps)
        let mem_cost = (ex_units.mem * mem_price.numerator) / mem_price.denominator;
        let step_cost = (ex_units.steps * step_price.numerator) / step_price.denominator;

        Some(mem_cost.saturating_add(step_cost))
    }

    /// Get default parameters for Cardano mainnet
    pub fn mainnet_defaults() -> Self {
        Self {
            min_fee_a: 44,
            min_fee_b: 155_381,
            max_tx_size: 16_384,
            max_block_body_size: 90_112,
            utxo_cost_per_byte: 4_310,
            min_utxo_lovelace: None,
            price_memory: Some(Rational::new(577, 10_000)),
            price_steps: Some(Rational::new(721, 10_000_000)),
            max_tx_execution_units: Some(ExUnits {
                mem: 14_000_000,
                steps: 10_000_000_000,
            }),
            max_block_execution_units: Some(ExUnits {
                mem: 62_000_000,
                steps: 40_000_000_000,
            }),
            key_deposit: 2_000_000,
            pool_deposit: 500_000_000,
            min_pool_cost: 340_000_000,
            epoch: 0,
        }
    }

    /// Get default parameters for Preprod testnet
    pub fn preprod_defaults() -> Self {
        Self {
            min_fee_a: 44,
            min_fee_b: 155_381,
            max_tx_size: 16_384,
            max_block_body_size: 90_112,
            utxo_cost_per_byte: 4_310,
            min_utxo_lovelace: None,
            price_memory: Some(Rational::new(577, 10_000)),
            price_steps: Some(Rational::new(721, 10_000_000)),
            max_tx_execution_units: Some(ExUnits {
                mem: 14_000_000,
                steps: 10_000_000_000,
            }),
            max_block_execution_units: Some(ExUnits {
                mem: 62_000_000,
                steps: 40_000_000_000,
            }),
            key_deposit: 2_000_000,
            pool_deposit: 500_000_000,
            min_pool_cost: 340_000_000,
            epoch: 0,
        }
    }

    /// Get default parameters for Preview testnet
    pub fn preview_defaults() -> Self {
        Self {
            min_fee_a: 44,
            min_fee_b: 155_381,
            max_tx_size: 16_384,
            max_block_body_size: 90_112,
            utxo_cost_per_byte: 4_310,
            min_utxo_lovelace: None,
            price_memory: Some(Rational::new(577, 10_000)),
            price_steps: Some(Rational::new(721, 10_000_000)),
            max_tx_execution_units: Some(ExUnits {
                mem: 14_000_000,
                steps: 10_000_000_000,
            }),
            max_block_execution_units: Some(ExUnits {
                mem: 62_000_000,
                steps: 40_000_000_000,
            }),
            key_deposit: 2_000_000,
            pool_deposit: 500_000_000,
            min_pool_cost: 340_000_000,
            epoch: 0,
        }
    }

    /// Get default parameters for SanchoNet testnet
    pub fn sanchonet_defaults() -> Self {
        Self {
            min_fee_a: 44,
            min_fee_b: 155_381,
            max_tx_size: 16_384,
            max_block_body_size: 90_112,
            utxo_cost_per_byte: 4_310,
            min_utxo_lovelace: None,
            price_memory: Some(Rational::new(577, 10_000)),
            price_steps: Some(Rational::new(721, 10_000_000)),
            max_tx_execution_units: Some(ExUnits {
                mem: 14_000_000,
                steps: 10_000_000_000,
            }),
            max_block_execution_units: Some(ExUnits {
                mem: 62_000_000,
                steps: 40_000_000_000,
            }),
            key_deposit: 2_000_000,
            pool_deposit: 500_000_000,
            min_pool_cost: 340_000_000,
            epoch: 0,
        }
    }

    /// Get default parameters for a given network name
    pub fn for_network(network: &str) -> Result<Self> {
        match network.to_lowercase().as_str() {
            "mainnet" => Ok(Self::mainnet_defaults()),
            "preprod" => Ok(Self::preprod_defaults()),
            "preview" => Ok(Self::preview_defaults()),
            "sanchonet" => Ok(Self::sanchonet_defaults()),
            _ => Err(ProtocolParamError::UnsupportedNetwork(network.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_min_fee() {
        let params = ProtocolParameters::mainnet_defaults();

        // Simple transaction ~300 bytes
        let fee = params.calculate_min_fee(300);
        assert_eq!(fee, 44 * 300 + 155_381); // 168,581 lovelace

        // Large transaction 10KB
        let fee = params.calculate_min_fee(10_000);
        assert_eq!(fee, 44 * 10_000 + 155_381); // 595,381 lovelace
    }

    #[test]
    fn test_calculate_min_utxo() {
        let params = ProtocolParameters::mainnet_defaults();

        // Simple output ~50 bytes
        let min_ada = params.calculate_min_utxo(50);
        assert_eq!(min_ada, 4_310 * 50); // 215,500 lovelace

        // Larger output with datum ~200 bytes
        let min_ada = params.calculate_min_utxo(200);
        assert_eq!(min_ada, 4_310 * 200); // 862,000 lovelace
    }

    #[test]
    fn test_calculate_execution_cost() {
        let params = ProtocolParameters::mainnet_defaults();

        let ex_units = ExUnits {
            mem: 1_000_000,
            steps: 1_000_000_000,
        };

        let cost = params.calculate_execution_cost(&ex_units).unwrap();

        // mem_cost = 1_000_000 * 577 / 10_000 = 57_700
        // step_cost = 1_000_000_000 * 721 / 10_000_000 = 72_100
        // total = 129_800 lovelace
        assert_eq!(cost, 57_700 + 72_100);
    }

    #[test]
    fn test_network_defaults() {
        let mainnet = ProtocolParameters::for_network("mainnet").unwrap();
        assert_eq!(mainnet.min_fee_a, 44);

        let preprod = ProtocolParameters::for_network("preprod").unwrap();
        assert_eq!(preprod.min_fee_a, 44);

        let unknown = ProtocolParameters::for_network("unknown");
        assert!(unknown.is_err());
    }

    #[test]
    fn test_rational_conversion() {
        let rational = Rational::new(577, 10_000);
        let f = rational.to_f64();
        assert!((f - 0.0577).abs() < 0.0001);
    }
}
