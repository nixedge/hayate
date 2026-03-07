// Datum construction and utilities

use crate::wallet::plutus::{PlutusError, PlutusResult};
use pallas_codec::minicbor;
use pallas_crypto::hash::{Hash, Hasher};

/// Datum option - either a hash or inline data
#[derive(Debug, Clone)]
pub enum DatumOption {
    /// Datum hash only (datum stored separately off-chain)
    Hash(Vec<u8>),
    /// Inline datum (Babbage+)
    Inline(Vec<u8>),
}

impl DatumOption {
    /// Create a datum hash from raw datum bytes
    pub fn from_datum_bytes(datum_bytes: &[u8]) -> Self {
        let hash = datum_hash(datum_bytes);
        DatumOption::Hash(hash.to_vec())
    }

    /// Create an inline datum
    pub fn inline(datum_bytes: Vec<u8>) -> Self {
        DatumOption::Inline(datum_bytes)
    }

    /// Get datum bytes for transaction building
    pub fn bytes(&self) -> Vec<u8> {
        match self {
            DatumOption::Hash(hash) => hash.clone(),
            DatumOption::Inline(data) => data.clone(),
        }
    }

    /// Check if this is an inline datum
    pub fn is_inline(&self) -> bool {
        matches!(self, DatumOption::Inline(_))
    }
}

/// Calculate datum hash using BLAKE2b-256
pub fn datum_hash(datum_bytes: &[u8]) -> [u8; 32] {
    let hash: Hash<32> = Hasher::<256>::hash(datum_bytes);
    let mut out = [0u8; 32];
    out.copy_from_slice(hash.as_ref());
    out
}

/// Governance member entry
#[derive(Debug, Clone)]
pub struct GovernanceMember {
    /// Cardano credential (28 bytes)
    pub cardano_hash: [u8; 28],
    /// Sr25519 public key (32 bytes)
    pub sr25519_key: [u8; 32],
}

/// VersionedMultisig datum for governance contracts
///
/// CBOR structure:
/// ```text
/// Constructor 0 [
///   Constructor 0 [
///     threshold: Int,
///     signers: Map { cardano_hash (28 bytes) -> sr25519_key (32 bytes) }
///   ],
///   logic_round: Int
/// ]
/// ```
#[derive(Debug, Clone)]
pub struct VersionedMultisig {
    pub threshold: u32,
    pub members: Vec<GovernanceMember>,
    pub logic_round: u32,
}

impl VersionedMultisig {
    /// Create a new VersionedMultisig datum
    pub fn new(threshold: u32, members: Vec<GovernanceMember>) -> Self {
        Self {
            threshold,
            members,
            logic_round: 0,
        }
    }

    /// Encode to CBOR bytes
    pub fn to_cbor(&self) -> PlutusResult<Vec<u8>> {
        let mut buffer = Vec::new();
        let mut encoder = minicbor::Encoder::new(&mut buffer);

        // VersionedMultisig constructor (121 = constructor tag, 0 = constructor index)
        encoder
            .array(2)
            .map_err(|e| PlutusError::CborEncode(e.to_string()))?;
        encoder
            .u32(121)
            .map_err(|e| PlutusError::CborEncode(e.to_string()))?; // Constructor tag
        encoder
            .array(2)
            .map_err(|e| PlutusError::CborEncode(e.to_string()))?; // [data, logic_round]

        // Multisig constructor
        encoder
            .array(2)
            .map_err(|e| PlutusError::CborEncode(e.to_string()))?;
        encoder
            .u32(121)
            .map_err(|e| PlutusError::CborEncode(e.to_string()))?; // Constructor tag
        encoder
            .array(2)
            .map_err(|e| PlutusError::CborEncode(e.to_string()))?; // [threshold, signers]

        // Threshold
        encoder
            .u32(self.threshold)
            .map_err(|e| PlutusError::CborEncode(e.to_string()))?;

        // Signers map
        encoder
            .map(self.members.len() as u64)
            .map_err(|e| PlutusError::CborEncode(e.to_string()))?;

        // Sort members by cardano_hash for deterministic encoding
        let mut sorted_members = self.members.clone();
        sorted_members.sort_by(|a, b| a.cardano_hash.cmp(&b.cardano_hash));

        for member in sorted_members {
            // Key: cardano_hash (28 bytes)
            encoder
                .bytes(&member.cardano_hash)
                .map_err(|e| PlutusError::CborEncode(e.to_string()))?;
            // Value: sr25519_key (32 bytes)
            encoder
                .bytes(&member.sr25519_key)
                .map_err(|e| PlutusError::CborEncode(e.to_string()))?;
        }

        // Logic round
        encoder
            .u32(self.logic_round)
            .map_err(|e| PlutusError::CborEncode(e.to_string()))?;

        Ok(buffer)
    }

    /// Create datum hash for this VersionedMultisig
    pub fn datum_hash(&self) -> PlutusResult<[u8; 32]> {
        let cbor = self.to_cbor()?;
        Ok(datum_hash(&cbor))
    }

    /// Create as inline datum option
    pub fn as_inline(&self) -> PlutusResult<DatumOption> {
        let cbor = self.to_cbor()?;
        Ok(DatumOption::inline(cbor))
    }

    /// Create as datum hash option
    pub fn as_hash(&self) -> PlutusResult<DatumOption> {
        let cbor = self.to_cbor()?;
        Ok(DatumOption::from_datum_bytes(&cbor))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_datum_hash_length() {
        let datum = vec![0x01, 0x02, 0x03, 0x04];
        let hash = datum_hash(&datum);
        assert_eq!(hash.len(), 32, "Datum hash must be 32 bytes");
    }

    #[test]
    fn test_datum_hash_deterministic() {
        let datum = vec![0x01, 0x02, 0x03, 0x04];
        let hash1 = datum_hash(&datum);
        let hash2 = datum_hash(&datum);
        assert_eq!(hash1, hash2, "Datum hash must be deterministic");
    }

    #[test]
    fn test_versioned_multisig_simple() {
        let member = GovernanceMember {
            cardano_hash: [1u8; 28],
            sr25519_key: [2u8; 32],
        };

        let datum = VersionedMultisig::new(1, vec![member]);
        let cbor = datum.to_cbor();
        assert!(cbor.is_ok(), "Should encode successfully");
    }

    #[test]
    fn test_versioned_multisig_multiple_members() {
        let member1 = GovernanceMember {
            cardano_hash: [1u8; 28],
            sr25519_key: [2u8; 32],
        };
        let member2 = GovernanceMember {
            cardano_hash: [3u8; 28],
            sr25519_key: [4u8; 32],
        };

        let datum = VersionedMultisig::new(2, vec![member1, member2]);
        let cbor = datum.to_cbor();
        assert!(cbor.is_ok(), "Should encode successfully");
    }

    #[test]
    fn test_versioned_multisig_datum_hash() {
        let member = GovernanceMember {
            cardano_hash: [1u8; 28],
            sr25519_key: [2u8; 32],
        };

        let datum = VersionedMultisig::new(1, vec![member]);
        let hash = datum.datum_hash();
        assert!(hash.is_ok(), "Should compute hash successfully");
        assert_eq!(hash.unwrap().len(), 32);
    }

    #[test]
    fn test_datum_option_from_bytes() {
        let datum_bytes = vec![0x01, 0x02, 0x03, 0x04];
        let option = DatumOption::from_datum_bytes(&datum_bytes);

        match option {
            DatumOption::Hash(hash) => {
                assert_eq!(hash.len(), 32);
            }
            _ => panic!("Expected Hash variant"),
        }
    }

    #[test]
    fn test_datum_option_inline() {
        let datum_bytes = vec![0x01, 0x02, 0x03, 0x04];
        let option = DatumOption::inline(datum_bytes.clone());

        match option {
            DatumOption::Inline(data) => {
                assert_eq!(data, datum_bytes);
            }
            _ => panic!("Expected Inline variant"),
        }
    }

    #[test]
    fn test_versioned_multisig_deterministic() {
        let member1 = GovernanceMember {
            cardano_hash: [3u8; 28],
            sr25519_key: [4u8; 32],
        };
        let member2 = GovernanceMember {
            cardano_hash: [1u8; 28],
            sr25519_key: [2u8; 32],
        };

        // Create with different orderings
        let datum1 = VersionedMultisig::new(2, vec![member1.clone(), member2.clone()]);
        let datum2 = VersionedMultisig::new(2, vec![member2, member1]);

        let cbor1 = datum1.to_cbor().unwrap();
        let cbor2 = datum2.to_cbor().unwrap();

        // Should be identical due to sorting
        assert_eq!(cbor1, cbor2, "Encoding should be deterministic regardless of input order");
    }
}
