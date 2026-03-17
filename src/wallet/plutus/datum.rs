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

/// VersionedMultisig datum for governance contracts (mainnet-compatible)
///
/// CBOR structure (matches mainnet):
/// ```text
/// [
///   [
///     total_signers: Int,
///     signers: Map { [0, bytes(28)] -> bytes(32) }
///   ],
///   logic_round: Int
/// ]
/// ```
///
/// Note: The Aiken validator calculates the actual 2/3 threshold as:
/// `(2 * total_signers + 2) / 3`
#[derive(Debug, Clone)]
pub struct VersionedMultisig {
    /// Total number of signers in the multisig
    /// Validator calculates actual 2/3 threshold: (2 * total_signers + 2) / 3
    pub total_signers: u32,
    pub members: Vec<GovernanceMember>,
    pub logic_round: u32,
}

impl VersionedMultisig {
    /// Create a new VersionedMultisig datum
    pub fn new(total_signers: u32, members: Vec<GovernanceMember>) -> Self {
        Self {
            total_signers,
            members,
            logic_round: 0,
        }
    }

    /// Encode to CBOR bytes matching Aiken governance datum structure
    ///
    /// Structure: [[total_signers, signers_list], logic_round]
    /// - No constructor tags (raw Plutus data)
    /// - Signers is a LIST of (wrapped_key, sr25519_key) pairs
    /// - Wrapped keys are CBOR-encoded as [0, bytes(28)]
    /// - Validator calculates 2/3 threshold from total_signers
    pub fn to_cbor(&self) -> PlutusResult<Vec<u8>> {
        let mut buffer = Vec::new();
        let mut encoder = minicbor::Encoder::new(&mut buffer);

        // Outer array: [multisig_data, logic_round]
        encoder
            .array(2)
            .map_err(|e| PlutusError::CborEncode(e.to_string()))?;

        // Inner array: [total_signers, signers_list]
        encoder
            .array(2)
            .map_err(|e| PlutusError::CborEncode(e.to_string()))?;

        // Total signers
        encoder
            .u32(self.total_signers)
            .map_err(|e| PlutusError::CborEncode(e.to_string()))?;

        // Signers list (array of pairs, NOT a map)
        encoder
            .array(self.members.len() as u64)
            .map_err(|e| PlutusError::CborEncode(e.to_string()))?;

        // Sort members by cardano_hash for deterministic encoding
        let mut sorted_members = self.members.clone();
        sorted_members.sort_by_key(|a| a.cardano_hash);

        for member in sorted_members {
            // Each pair is a 2-element array
            encoder
                .array(2)
                .map_err(|e| PlutusError::CborEncode(e.to_string()))?;

            // First element: CBOR-wrapped as [0, bytes(28)]
            // This encodes to: 8200581c<28 bytes>
            let mut key_buffer = Vec::new();
            let mut key_encoder = minicbor::Encoder::new(&mut key_buffer);
            key_encoder
                .array(2)
                .map_err(|e| PlutusError::CborEncode(e.to_string()))?;
            key_encoder
                .u32(0)
                .map_err(|e| PlutusError::CborEncode(e.to_string()))?;
            key_encoder
                .bytes(&member.cardano_hash)
                .map_err(|e| PlutusError::CborEncode(e.to_string()))?;

            // Encode the wrapped key as bytes (first element of pair)
            encoder
                .bytes(&key_buffer)
                .map_err(|e| PlutusError::CborEncode(e.to_string()))?;

            // Second element: sr25519_key (32 bytes)
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

    /// Decode VersionedMultisig from CBOR bytes
    pub fn from_cbor(cbor: &[u8]) -> PlutusResult<Self> {
        use pallas_codec::minicbor::Decoder;

        let mut decoder = Decoder::new(cbor);

        // Outer array: [multisig_data, logic_round]
        let outer_len = decoder.array()
            .map_err(|e| PlutusError::CborDecode(format!("Expected outer array: {}", e)))?
            .ok_or_else(|| PlutusError::CborDecode("Outer array length missing".to_string()))?;

        if outer_len != 2 {
            return Err(PlutusError::CborDecode(format!("Expected outer array of length 2, got {}", outer_len)));
        }

        // Inner array: [total_signers, signers_map]
        let inner_len = decoder.array()
            .map_err(|e| PlutusError::CborDecode(format!("Expected inner array: {}", e)))?
            .ok_or_else(|| PlutusError::CborDecode("Inner array length missing".to_string()))?;

        if inner_len != 2 {
            return Err(PlutusError::CborDecode(format!("Expected inner array of length 2, got {}", inner_len)));
        }

        // total_signers
        let total_signers = decoder.u32()
            .map_err(|e| PlutusError::CborDecode(format!("Failed to decode total_signers: {}", e)))?;

        // signers list (array of pairs)
        let array_len = decoder.array()
            .map_err(|e| PlutusError::CborDecode(format!("Expected array: {}", e)))?
            .ok_or_else(|| PlutusError::CborDecode("Array length missing".to_string()))?;

        let mut members = Vec::new();
        for _ in 0..array_len {
            // Each element is a 2-element array (pair)
            let pair_len = decoder.array()
                .map_err(|e| PlutusError::CborDecode(format!("Expected pair array: {}", e)))?
                .ok_or_else(|| PlutusError::CborDecode("Pair length missing".to_string()))?;

            if pair_len != 2 {
                return Err(PlutusError::CborDecode(format!("Expected pair of length 2, got {}", pair_len)));
            }

            // First element: wrapped Cardano key hash (CBOR bytes)
            let wrapped_key = decoder.bytes()
                .map_err(|e| PlutusError::CborDecode(format!("Failed to decode wrapped key: {}", e)))?;

            // Unwrap: skip first 4 bytes (82 00 58 1c) to get the actual hash
            if wrapped_key.len() < 32 {
                return Err(PlutusError::CborDecode(format!("Wrapped key too short: {}", wrapped_key.len())));
            }
            let cardano_hash_bytes = &wrapped_key[4..32];
            let mut cardano_hash = [0u8; 28];
            cardano_hash.copy_from_slice(cardano_hash_bytes);

            // Second element: sr25519 key (32 bytes)
            let sr25519_bytes = decoder.bytes()
                .map_err(|e| PlutusError::CborDecode(format!("Failed to decode sr25519 key: {}", e)))?;

            if sr25519_bytes.len() != 32 {
                return Err(PlutusError::CborDecode(format!("Invalid sr25519 key length: {}", sr25519_bytes.len())));
            }
            let mut sr25519_key = [0u8; 32];
            sr25519_key.copy_from_slice(sr25519_bytes);

            members.push(GovernanceMember {
                cardano_hash,
                sr25519_key,
            });
        }

        // logic_round
        let logic_round = decoder.u32()
            .map_err(|e| PlutusError::CborDecode(format!("Failed to decode logic_round: {}", e)))?;

        Ok(Self {
            total_signers,
            members,
            logic_round,
        })
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
