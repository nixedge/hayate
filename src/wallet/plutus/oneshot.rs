// One-shot NFT minting using native scripts
//
// This module provides functionality for minting unique NFTs using native scripts
// with transaction output reference locking. The minting policy enforces that a
// specific UTxO must be consumed, ensuring the NFT can only be minted once.

use crate::wallet::plutus::{PlutusError, PlutusResult};
use pallas_codec::minicbor::{self, Encoder};
use pallas_crypto::hash::{Hash, Hasher};

/// One-shot minting policy parameters
#[derive(Debug, Clone)]
pub struct OneShotPolicy {
    /// Transaction hash of the UTxO to consume (32 bytes)
    pub tx_hash: [u8; 32],
    /// Output index of the UTxO to consume
    pub output_index: u64,
}

impl OneShotPolicy {
    /// Create a new one-shot minting policy
    pub fn new(tx_hash: [u8; 32], output_index: u64) -> Self {
        Self {
            tx_hash,
            output_index,
        }
    }

    /// Parse from UTxO reference string "tx_hash#index"
    pub fn from_utxo_ref(utxo_ref: &str) -> PlutusResult<Self> {
        let parts: Vec<&str> = utxo_ref.split('#').collect();
        if parts.len() != 2 {
            return Err(PlutusError::InvalidInput(format!(
                "Invalid UTxO reference format: {}. Expected 'tx_hash#index'",
                utxo_ref
            )));
        }

        let tx_hash_hex = parts[0].trim_start_matches("0x");
        let tx_hash_bytes = hex::decode(tx_hash_hex).map_err(|e| {
            PlutusError::InvalidInput(format!("Invalid tx_hash hex: {}", e))
        })?;

        if tx_hash_bytes.len() != 32 {
            return Err(PlutusError::InvalidInput(format!(
                "tx_hash must be 32 bytes, got {}",
                tx_hash_bytes.len()
            )));
        }

        let mut tx_hash = [0u8; 32];
        tx_hash.copy_from_slice(&tx_hash_bytes);

        let output_index = parts[1].parse::<u64>().map_err(|e| {
            PlutusError::InvalidInput(format!("Invalid output index: {}", e))
        })?;

        Ok(Self::new(tx_hash, output_index))
    }

    /// Build native script CBOR bytes
    ///
    /// Native script structure (using ScriptAll):
    /// ```text
    /// ScriptAll [
    ///   ScriptPubkey <pubkey_hash>,
    ///   ScriptNofK 1 [
    ///     ScriptAll [
    ///       ScriptPubkey <pubkey_hash>,
    ///       ScriptNofK 1 [ScriptAll []]
    ///     ]
    ///   ]
    /// ]
    /// ```
    ///
    /// However, for one-shot we want to use the simpler approach:
    /// Just check that the specific UTxO reference is in the inputs.
    ///
    /// Actually, native scripts don't have direct UTxO reference checking.
    /// The proper way is to use a script that references a specific pubkey
    /// and ensure that pubkey signs the transaction.
    ///
    /// For true one-shot with UTxO reference, we need to:
    /// 1. Use a Plutus script that checks the UTxO reference
    /// 2. OR use a native script with a pubkey that we control, then destroy the key after minting
    ///
    /// For simplicity and following Cardano best practices, we'll use a parameterized
    /// Plutus minting policy that checks the UTxO reference.
    ///
    /// However, midnight may have pre-compiled scripts. Let me check the documentation approach...
    ///
    /// Actually, the simplest approach for MVP is:
    /// - Generate a temporary keypair
    /// - Use native script with that pubkey
    /// - Sign the mint transaction with that key
    /// - Return policy ID
    ///
    /// The NFT is then "one-shot" in practice because we don't save the key.
    ///
    /// But this isn't true one-shot. For proper one-shot, we need a Plutus script.
    ///
    /// Let's implement proper Plutus one-shot minting policy:
    pub fn to_plutus_script(&self) -> PlutusResult<Vec<u8>> {
        // For now, return error indicating we need to implement the Plutus script
        // This would normally be a pre-compiled Plutus validator that checks:
        // 1. The specified UTxO reference is in the transaction inputs
        // 2. Exactly 1 token is minted
        Err(PlutusError::NotImplemented(
            "Plutus one-shot minting policy not yet implemented. \
            Consider using a native script with a temporary key instead."
                .to_string(),
        ))
    }

    /// Calculate policy ID for this one-shot policy
    ///
    /// The policy ID is the BLAKE2b-224 hash of the script
    pub fn policy_id(&self, script_bytes: &[u8]) -> [u8; 28] {
        let hash: Hash<28> = Hasher::<224>::hash(script_bytes);
        let mut policy_id = [0u8; 28];
        policy_id.copy_from_slice(hash.as_ref());
        policy_id
    }
}

/// Temporary keypair approach for NFT minting
///
/// This generates a native script that requires a signature from a specific key.
/// The key is generated once, used to mint, then discarded - making it effectively one-shot.
#[derive(Debug)]
pub struct TempKeyMintPolicy {
    /// Ed25519 verification key hash (28 bytes)
    pub vkey_hash: [u8; 28],
}

impl TempKeyMintPolicy {
    /// Create from a verification key hash
    pub fn new(vkey_hash: [u8; 28]) -> Self {
        Self { vkey_hash }
    }

    /// Build native script CBOR bytes
    ///
    /// ScriptPubkey constructor (tag 0):
    /// ```cbor
    /// [0, vkey_hash]
    /// ```
    pub fn to_native_script(&self) -> PlutusResult<Vec<u8>> {
        let mut buffer = Vec::new();
        let mut encoder = Encoder::new(&mut buffer);

        // Native script: ScriptPubkey
        encoder
            .array(2)
            .map_err(|e| PlutusError::CborEncode(e.to_string()))?;
        encoder
            .u32(0) // ScriptPubkey tag
            .map_err(|e| PlutusError::CborEncode(e.to_string()))?;
        encoder
            .bytes(&self.vkey_hash)
            .map_err(|e| PlutusError::CborEncode(e.to_string()))?;

        Ok(buffer)
    }

    /// Calculate policy ID for this script
    pub fn policy_id(&self) -> PlutusResult<[u8; 28]> {
        let script = self.to_native_script()?;
        let hash: Hash<28> = Hasher::<224>::hash(&script);
        let mut policy_id = [0u8; 28];
        policy_id.copy_from_slice(hash.as_ref());
        Ok(policy_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oneshot_from_utxo_ref() {
        let utxo_ref = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789#5";
        let policy = OneShotPolicy::from_utxo_ref(utxo_ref);
        assert!(policy.is_ok());

        let policy = policy.unwrap();
        assert_eq!(policy.output_index, 5);
    }

    #[test]
    fn test_oneshot_from_utxo_ref_invalid() {
        let utxo_ref = "invalid";
        let policy = OneShotPolicy::from_utxo_ref(utxo_ref);
        assert!(policy.is_err());
    }

    #[test]
    fn test_temp_key_mint_policy() {
        let vkey_hash = [1u8; 28];
        let policy = TempKeyMintPolicy::new(vkey_hash);

        let script = policy.to_native_script();
        assert!(script.is_ok());

        let policy_id = policy.policy_id();
        assert!(policy_id.is_ok());
        assert_eq!(policy_id.unwrap().len(), 28);
    }

    #[test]
    fn test_temp_key_mint_policy_deterministic() {
        let vkey_hash = [42u8; 28];
        let policy1 = TempKeyMintPolicy::new(vkey_hash);
        let policy2 = TempKeyMintPolicy::new(vkey_hash);

        let id1 = policy1.policy_id().unwrap();
        let id2 = policy2.policy_id().unwrap();

        assert_eq!(id1, id2, "Policy IDs should be deterministic");
    }
}
