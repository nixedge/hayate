// Plutus script wrapper and utilities

use crate::wallet::plutus::{address, Network, PlutusError, PlutusResult};
use pallas_primitives::conway::PlutusScript as PallasPlutusScript;

/// Plutus script version
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlutusVersion {
    V1,
    V2,
    V3,
}

/// Wrapper around pallas PlutusScript with wallet-specific functionality
#[derive(Debug, Clone)]
pub struct PlutusScript {
    version: PlutusVersion,
    cbor: Vec<u8>,
    hash: [u8; 28], // Cached script hash
}

impl PlutusScript {
    /// Create a PlutusScript from CBOR bytes
    ///
    /// The CBOR should be the double-encoded script:
    /// - Outer layer: CBOR byte string tag
    /// - Inner layer: The actual Plutus script
    pub fn from_cbor(cbor: Vec<u8>, version: PlutusVersion) -> PlutusResult<Self> {
        if cbor.is_empty() {
            return Err(PlutusError::InvalidScript(
                "Script CBOR cannot be empty".to_string(),
            ));
        }

        // Calculate and cache the script hash using version-tagged hash
        let version_tag = match version {
            PlutusVersion::V1 => 1,
            PlutusVersion::V2 => 2,
            PlutusVersion::V3 => 3,
        };
        let hash = address::script_hash_versioned(&cbor, version_tag);

        Ok(Self {
            version,
            cbor,
            hash,
        })
    }

    /// Create a PlutusV2 script from CBOR
    pub fn v2_from_cbor(cbor: Vec<u8>) -> PlutusResult<Self> {
        Self::from_cbor(cbor, PlutusVersion::V2)
    }

    /// Create a PlutusV3 script from CBOR
    pub fn v3_from_cbor(cbor: Vec<u8>) -> PlutusResult<Self> {
        Self::from_cbor(cbor, PlutusVersion::V3)
    }

    /// Get the script version
    pub fn version(&self) -> PlutusVersion {
        self.version
    }

    /// Get the CBOR bytes
    pub fn cbor(&self) -> &[u8] {
        &self.cbor
    }

    /// Get the cached script hash (policy ID)
    pub fn hash(&self) -> &[u8; 28] {
        &self.hash
    }

    /// Get the script address for a given network
    pub fn address(&self, network: Network) -> PlutusResult<Vec<u8>> {
        // Use the cached hash (which is version-tagged) instead of recalculating
        let mut addr = Vec::with_capacity(29);
        addr.push(network.to_header_byte());
        addr.extend_from_slice(&self.hash);
        Ok(addr)
    }

    /// Get the policy ID (same as hash)
    pub fn policy_id(&self) -> [u8; 28] {
        self.hash
    }

    /// Convert to pallas PlutusScript<2>
    pub fn to_pallas_v2(&self) -> PlutusResult<PallasPlutusScript<2>> {
        if self.version != PlutusVersion::V2 {
            return Err(PlutusError::InvalidScript(
                "Script is not PlutusV2".to_string(),
            ));
        }

        Ok(PallasPlutusScript::<2>(self.cbor.clone().into()))
    }

    /// Convert to pallas PlutusScript<3>
    pub fn to_pallas_v3(&self) -> PlutusResult<PallasPlutusScript<3>> {
        if self.version != PlutusVersion::V3 {
            return Err(PlutusError::InvalidScript(
                "Script is not PlutusV3".to_string(),
            ));
        }

        Ok(PallasPlutusScript::<3>(self.cbor.clone().into()))
    }

    /// Get CBOR bytes for use in transactions
    /// This is the format expected by pallas-txbuilder
    pub fn to_tx_bytes(&self) -> Vec<u8> {
        self.cbor.clone()
    }

    /// Verify the script hash matches
    pub fn verify_hash(&self, expected_hash: &[u8]) -> bool {
        if expected_hash.len() != 28 {
            return false;
        }

        // Compute version-tagged hash to match how the hash is cached
        let version_tag = match self.version {
            PlutusVersion::V1 => 1,
            PlutusVersion::V2 => 2,
            PlutusVersion::V3 => 3,
        };
        let actual_hash = address::script_hash_versioned(&self.cbor, version_tag);
        actual_hash.as_slice() == expected_hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_plutus_v2_script() {
        let cbor = vec![0x01, 0x02, 0x03, 0x04];
        let script = PlutusScript::v2_from_cbor(cbor.clone()).unwrap();

        assert_eq!(script.version(), PlutusVersion::V2);
        assert_eq!(script.cbor(), &cbor);
        assert_eq!(script.hash().len(), 28);
    }

    #[test]
    fn test_create_plutus_v3_script() {
        let cbor = vec![0x01, 0x02, 0x03, 0x04];
        let script = PlutusScript::v3_from_cbor(cbor.clone()).unwrap();

        assert_eq!(script.version(), PlutusVersion::V3);
        assert_eq!(script.cbor(), &cbor);
    }

    #[test]
    fn test_empty_script_fails() {
        let result = PlutusScript::v2_from_cbor(vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn test_script_hash_cached() {
        let cbor = vec![0x01, 0x02, 0x03, 0x04];
        let script = PlutusScript::v2_from_cbor(cbor.clone()).unwrap();

        // Hash should be cached and consistent
        let hash1 = script.hash();
        let hash2 = script.hash();
        assert_eq!(hash1, hash2);

        // Should match direct calculation with version tag
        let expected = address::script_hash_versioned(&cbor, 2);
        assert_eq!(hash1, &expected);
    }

    #[test]
    fn test_script_address_testnet() {
        let cbor = vec![0x01, 0x02, 0x03, 0x04];
        let script = PlutusScript::v2_from_cbor(cbor).unwrap();

        let addr = script.address(Network::Testnet).unwrap();
        assert_eq!(addr.len(), 29);
        assert_eq!(addr[0], 0x70); // Testnet header byte
    }

    #[test]
    fn test_script_address_mainnet() {
        let cbor = vec![0x01, 0x02, 0x03, 0x04];
        let script = PlutusScript::v2_from_cbor(cbor).unwrap();

        let addr = script.address(Network::Mainnet).unwrap();
        assert_eq!(addr.len(), 29);
        assert_eq!(addr[0], 0x71); // Mainnet header byte
    }

    #[test]
    fn test_policy_id_matches_hash() {
        let cbor = vec![0x01, 0x02, 0x03, 0x04];
        let script = PlutusScript::v2_from_cbor(cbor).unwrap();

        assert_eq!(script.policy_id(), *script.hash());
    }

    #[test]
    fn test_verify_hash() {
        let cbor = vec![0x01, 0x02, 0x03, 0x04];
        let script = PlutusScript::v2_from_cbor(cbor).unwrap();

        assert!(script.verify_hash(script.hash()));
        assert!(!script.verify_hash(&[0u8; 28]));
    }

    #[test]
    fn test_to_tx_bytes() {
        let cbor = vec![0x01, 0x02, 0x03, 0x04];
        let script = PlutusScript::v2_from_cbor(cbor.clone()).unwrap();

        assert_eq!(script.to_tx_bytes(), cbor);
    }

    #[test]
    fn test_to_pallas_v2_wrong_version() {
        let cbor = vec![0x01, 0x02, 0x03, 0x04];
        let script = PlutusScript::v3_from_cbor(cbor).unwrap();

        let result = script.to_pallas_v2();
        assert!(result.is_err());
    }
}
