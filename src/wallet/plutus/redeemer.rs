// Redeemer types and utilities

use crate::wallet::plutus::PlutusResult;
use pallas_primitives::conway::{ExUnits as PallasExUnits, RedeemerTag as PallasRedeemerTag};

/// Execution units for Plutus script execution
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExUnits {
    pub mem: u64,
    pub steps: u64,
}

impl ExUnits {
    /// Create new execution units
    pub fn new(mem: u64, steps: u64) -> Self {
        Self { mem, steps }
    }

    /// Default execution units for governance contracts (conservative estimates)
    pub fn governance_default() -> Self {
        Self {
            mem: 14_000_000,  // 14M memory units
            steps: 10_000_000_000, // 10B step units
        }
    }

    /// Zero execution units (for testing)
    pub fn zero() -> Self {
        Self { mem: 0, steps: 0 }
    }

    /// Convert to pallas ExUnits
    pub fn to_pallas(&self) -> PallasExUnits {
        PallasExUnits {
            mem: self.mem,
            steps: self.steps,
        }
    }
}

impl Default for ExUnits {
    fn default() -> Self {
        Self::governance_default()
    }
}

/// Redeemer tag indicating which type of script is being executed
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedeemerTag {
    /// Spending from a script address
    Spend,
    /// Minting/burning tokens
    Mint,
    /// Certificate operations
    Cert,
    /// Reward withdrawal
    Reward,
}

impl RedeemerTag {
    /// Convert to pallas RedeemerTag
    pub fn to_pallas(&self) -> PallasRedeemerTag {
        match self {
            RedeemerTag::Spend => PallasRedeemerTag::Spend,
            RedeemerTag::Mint => PallasRedeemerTag::Mint,
            RedeemerTag::Cert => PallasRedeemerTag::Cert,
            RedeemerTag::Reward => PallasRedeemerTag::Reward,
        }
    }
}

/// Redeemer for Plutus script execution
#[derive(Debug, Clone)]
pub struct Redeemer {
    /// Tag indicating script type
    pub tag: RedeemerTag,
    /// Index of the input/mint/cert/reward being spent
    pub index: u32,
    /// Redeemer data (PlutusData CBOR)
    pub data: Vec<u8>,
    /// Execution units
    pub ex_units: ExUnits,
}

impl Redeemer {
    /// Create a new redeemer
    pub fn new(tag: RedeemerTag, index: u32, data: Vec<u8>, ex_units: ExUnits) -> Self {
        Self {
            tag,
            index,
            data,
            ex_units,
        }
    }

    /// Create a spend redeemer (most common for governance)
    pub fn spend(index: u32, data: Vec<u8>) -> Self {
        Self::new(RedeemerTag::Spend, index, data, ExUnits::default())
    }

    /// Create a mint redeemer
    pub fn mint(index: u32, data: Vec<u8>) -> Self {
        Self::new(RedeemerTag::Mint, index, data, ExUnits::default())
    }

    /// Create an empty redeemer (unit value in Plutus)
    pub fn empty(tag: RedeemerTag, index: u32) -> Self {
        Self::new(tag, index, vec![], ExUnits::default())
    }

    /// Set custom execution units
    pub fn with_ex_units(mut self, ex_units: ExUnits) -> Self {
        self.ex_units = ex_units;
        self
    }

    /// Get redeemer data bytes for transaction building
    pub fn data_bytes(&self) -> &[u8] {
        &self.data
    }

    /// Get tag as pallas type
    pub fn pallas_tag(&self) -> PallasRedeemerTag {
        self.tag.to_pallas()
    }

    /// Get ex_units as pallas type
    pub fn pallas_ex_units(&self) -> PallasExUnits {
        self.ex_units.to_pallas()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ex_units_new() {
        let ex_units = ExUnits::new(1000, 2000);
        assert_eq!(ex_units.mem, 1000);
        assert_eq!(ex_units.steps, 2000);
    }

    #[test]
    fn test_ex_units_default() {
        let ex_units = ExUnits::default();
        assert!(ex_units.mem > 0);
        assert!(ex_units.steps > 0);
    }

    #[test]
    fn test_ex_units_governance_default() {
        let ex_units = ExUnits::governance_default();
        assert_eq!(ex_units.mem, 14_000_000);
        assert_eq!(ex_units.steps, 10_000_000_000);
    }

    #[test]
    fn test_ex_units_zero() {
        let ex_units = ExUnits::zero();
        assert_eq!(ex_units.mem, 0);
        assert_eq!(ex_units.steps, 0);
    }

    #[test]
    fn test_redeemer_new() {
        let data = vec![0x01, 0x02, 0x03];
        let redeemer = Redeemer::new(
            RedeemerTag::Spend,
            0,
            data.clone(),
            ExUnits::new(1000, 2000),
        );

        assert_eq!(redeemer.tag, RedeemerTag::Spend);
        assert_eq!(redeemer.index, 0);
        assert_eq!(redeemer.data, data);
        assert_eq!(redeemer.ex_units.mem, 1000);
        assert_eq!(redeemer.ex_units.steps, 2000);
    }

    #[test]
    fn test_redeemer_spend() {
        let data = vec![0x01, 0x02];
        let redeemer = Redeemer::spend(5, data.clone());

        assert_eq!(redeemer.tag, RedeemerTag::Spend);
        assert_eq!(redeemer.index, 5);
        assert_eq!(redeemer.data, data);
    }

    #[test]
    fn test_redeemer_mint() {
        let data = vec![0x01, 0x02];
        let redeemer = Redeemer::mint(3, data.clone());

        assert_eq!(redeemer.tag, RedeemerTag::Mint);
        assert_eq!(redeemer.index, 3);
        assert_eq!(redeemer.data, data);
    }

    #[test]
    fn test_redeemer_empty() {
        let redeemer = Redeemer::empty(RedeemerTag::Spend, 0);

        assert_eq!(redeemer.tag, RedeemerTag::Spend);
        assert_eq!(redeemer.index, 0);
        assert!(redeemer.data.is_empty());
    }

    #[test]
    fn test_redeemer_with_custom_ex_units() {
        let redeemer = Redeemer::spend(0, vec![])
            .with_ex_units(ExUnits::new(5000, 10000));

        assert_eq!(redeemer.ex_units.mem, 5000);
        assert_eq!(redeemer.ex_units.steps, 10000);
    }

    #[test]
    fn test_redeemer_data_bytes() {
        let data = vec![0x01, 0x02, 0x03];
        let redeemer = Redeemer::spend(0, data.clone());

        assert_eq!(redeemer.data_bytes(), &data);
    }

    #[test]
    fn test_redeemer_tags() {
        assert_eq!(RedeemerTag::Spend.to_pallas(), PallasRedeemerTag::Spend);
        assert_eq!(RedeemerTag::Mint.to_pallas(), PallasRedeemerTag::Mint);
        assert_eq!(RedeemerTag::Cert.to_pallas(), PallasRedeemerTag::Cert);
        assert_eq!(RedeemerTag::Reward.to_pallas(), PallasRedeemerTag::Reward);
    }
}
