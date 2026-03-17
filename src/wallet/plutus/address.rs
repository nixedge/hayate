// Script address calculation and utilities

use crate::wallet::plutus::{PlutusError, PlutusResult};
use pallas_crypto::hash::{Hash, Hasher};

/// Cardano network for address encoding
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Network {
    Mainnet,
    Testnet,
}

impl Network {
    pub fn from_magic(magic: u32) -> PlutusResult<Self> {
        match magic {
            764824073 => Ok(Network::Mainnet),
            1 | 2 | 4 => Ok(Network::Testnet), // testnet, preview, preprod
            _ => Err(PlutusError::InvalidNetwork(format!(
                "Unknown network magic: {}",
                magic
            ))),
        }
    }

    pub fn to_header_byte(&self) -> u8 {
        match self {
            Network::Testnet => 0x70, // 0b0111_0000 - script address, no stake, testnet (network bit = 0)
            Network::Mainnet => 0x71, // 0b0111_0001 - script address, no stake, mainnet (network bit = 1)
        }
    }
}

/// Calculate Plutus script hash (28 bytes) using version-tagged hash
///
/// Cardano uses tagged hashes for Plutus scripts to prevent collisions:
/// - PlutusV1: tag 1
/// - PlutusV2: tag 2
/// - PlutusV3: tag 3
///
/// This matches the pallas-txbuilder implementation
pub fn script_hash_versioned(script_cbor: &[u8], version_tag: u8) -> [u8; 28] {
    let hash: Hash<28> = Hasher::<224>::hash_tagged(script_cbor, version_tag);
    let mut out = [0u8; 28];
    out.copy_from_slice(hash.as_ref());
    out
}

/// Calculate script hash (policy ID) from Plutus script CBOR using BLAKE2b-224
///
/// DEPRECATED: This uses untagged hash which is incorrect for Plutus scripts.
/// Use script_hash_versioned with the appropriate version tag instead.
#[deprecated(note = "Use script_hash_versioned with version tag (1 for V1, 2 for V2, 3 for V3)")]
pub fn script_hash(script_cbor: &[u8]) -> [u8; 28] {
    let hash: Hash<28> = Hasher::<224>::hash(script_cbor);
    let mut out = [0u8; 28];
    out.copy_from_slice(hash.as_ref());
    out
}

/// Calculate script address from Plutus script CBOR
///
/// The script address is constructed as:
/// - Header byte: 0b0111_000n where n is network bit (0=testnet, 1=mainnet)
/// - Payment credential: BLAKE2b-224 hash of script CBOR (28 bytes)
/// - No stake credential
///
/// Total: 29 bytes
///
/// DEPRECATED: Use PlutusScript::address() which uses version-tagged hash
#[deprecated(note = "Use PlutusScript::address() which uses version-tagged hash")]
#[allow(deprecated)]
pub fn script_address(script_cbor: &[u8], network: Network) -> PlutusResult<Vec<u8>> {
    if script_cbor.is_empty() {
        return Err(PlutusError::InvalidScript(
            "Script CBOR cannot be empty".to_string(),
        ));
    }

    let hash = script_hash(script_cbor);
    let mut addr = Vec::with_capacity(29);
    addr.push(network.to_header_byte());
    addr.extend_from_slice(&hash);

    Ok(addr)
}

/// Verify a script hash matches the expected script
///
/// DEPRECATED: Use PlutusScript::verify_hash() which uses version-tagged hash
#[deprecated(note = "Use PlutusScript::verify_hash() which uses version-tagged hash")]
#[allow(deprecated)]
pub fn verify_script_hash(script_cbor: &[u8], expected_hash: &[u8]) -> bool {
    if expected_hash.len() != 28 {
        return false;
    }

    let actual_hash = script_hash(script_cbor);
    actual_hash.as_slice() == expected_hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_script_hash_length() {
        let script = vec![0x01, 0x02, 0x03, 0x04];
        let hash = script_hash_versioned(&script, 2); // V2 script
        assert_eq!(hash.len(), 28, "Script hash must be 28 bytes");
    }

    #[test]
    fn test_script_hash_deterministic() {
        let script = vec![0x01, 0x02, 0x03, 0x04];
        let hash1 = script_hash_versioned(&script, 2); // V2 script
        let hash2 = script_hash_versioned(&script, 2);
        assert_eq!(hash1, hash2, "Script hash must be deterministic");
    }

    #[test]
    #[allow(deprecated)]
    fn test_script_address_testnet() {
        let script = vec![0x01, 0x02, 0x03, 0x04];
        let addr = script_address(&script, Network::Testnet).unwrap();

        assert_eq!(addr.len(), 29, "Address must be 29 bytes");
        assert_eq!(addr[0], 0x70, "Header byte must be 0x70 for testnet");
    }

    #[test]
    #[allow(deprecated)]
    fn test_script_address_mainnet() {
        let script = vec![0x01, 0x02, 0x03, 0x04];
        let addr = script_address(&script, Network::Mainnet).unwrap();

        assert_eq!(addr.len(), 29, "Address must be 29 bytes");
        assert_eq!(addr[0], 0x71, "Header byte must be 0x71 for mainnet");
    }

    #[test]
    #[allow(deprecated)]
    fn test_verify_script_hash() {
        let script = vec![0x01, 0x02, 0x03, 0x04];
        let hash = script_hash(&script);

        assert!(
            verify_script_hash(&script, &hash),
            "Verification should succeed for matching hash"
        );
    }

    #[test]
    #[allow(deprecated)]
    fn test_verify_script_hash_mismatch() {
        let script = vec![0x01, 0x02, 0x03, 0x04];
        let wrong_hash = [0u8; 28];

        assert!(
            !verify_script_hash(&script, &wrong_hash),
            "Verification should fail for wrong hash"
        );
    }

    #[test]
    #[allow(deprecated)]
    fn test_empty_script() {
        let result = script_address(&[], Network::Testnet);
        assert!(result.is_err(), "Empty script should return error");
    }

    #[test]
    fn test_network_from_magic() {
        assert_eq!(Network::from_magic(764824073).unwrap(), Network::Mainnet);
        assert_eq!(Network::from_magic(1).unwrap(), Network::Testnet);
        assert_eq!(Network::from_magic(2).unwrap(), Network::Testnet); // preview
        assert_eq!(Network::from_magic(4).unwrap(), Network::Testnet); // preprod
        assert!(Network::from_magic(999).is_err()); // invalid
    }

    #[test]
    fn test_different_scripts_different_hashes() {
        let script1 = vec![0x01, 0x02, 0x03, 0x04];
        let script2 = vec![0x05, 0x06, 0x07, 0x08];
        let hash1 = script_hash_versioned(&script1, 2);
        let hash2 = script_hash_versioned(&script2, 2);

        assert_ne!(hash1, hash2, "Different scripts should have different hashes");
    }
}
