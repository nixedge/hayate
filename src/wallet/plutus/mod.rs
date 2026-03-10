// Plutus transaction support for Cardano
//
// This module provides types and utilities for building Plutus script transactions,
// including script address calculation, datum construction, and redeemer handling.

pub mod address;
pub mod cost_models;
pub mod datum;
pub mod oneshot;
pub mod redeemer;
pub mod script;

// Re-export commonly used types
pub use address::Network;
pub use cost_models::default_cost_model;
#[allow(unused_imports)]
pub use datum::{DatumOption, GovernanceMember, VersionedMultisig};
#[allow(unused_imports)]
pub use redeemer::{Redeemer, RedeemerTag};
pub use script::{PlutusScript, PlutusVersion};

use thiserror::Error;

#[derive(Error, Debug)]
pub enum PlutusError {
    #[error("Invalid script CBOR: {0}")]
    InvalidScript(String),

    #[error("Invalid datum: {0}")]
    InvalidDatum(String),

    #[error("Invalid redeemer: {0}")]
    InvalidRedeemer(String),

    #[error("CBOR encoding error: {0}")]
    CborEncode(String),

    #[error("CBOR decoding error: {0}")]
    CborDecode(String),

    #[error("Pallas error: {0}")]
    Pallas(String),

    #[error("Script hash mismatch")]
    ScriptHashMismatch,

    #[error("Invalid network: {0}")]
    InvalidNetwork(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Not implemented: {0}")]
    NotImplemented(String),
}

pub type PlutusResult<T> = Result<T, PlutusError>;

/// Apply parameters to a Plutus script
///
/// Creates a new script with parameters applied. This is commonly used for
/// parameterized validators like one-shot NFT policies.
///
/// # Arguments
/// * `script` - The base (unapplied) Plutus script
/// * `params` - List of CBOR-encoded parameters to apply
///
/// # Returns
/// A new PlutusScript with parameters applied
pub fn apply_params_to_script(script: &PlutusScript, params: &[Vec<u8>]) -> PlutusResult<PlutusScript> {
    use minicbor::{Encoder, Decoder};

    // Scripts are double-encoded: outer CBOR byte string wrapping inner script
    // First, decode the outer layer to get the inner script bytes
    let mut decoder = Decoder::new(script.cbor());
    let inner_script = decoder.bytes()
        .map_err(|e| PlutusError::CborDecode(format!("Failed to decode script outer layer: {}", e)))?;

    // Build the applied script structure: [[param1, param2, ...], original_inner_script]
    let mut applied_inner = Vec::new();

    // CBOR array header for 2 elements
    applied_inner.push(0x82); // array(2)

    // First element: array of parameters
    let params_array_header = match params.len() {
        n @ 0..=23 => vec![0x80 + n as u8], // array(n) for n <= 23
        n @ 24..=255 => vec![0x98, n as u8], // array(n) for n <= 255
        n => return Err(PlutusError::InvalidInput(format!("Too many parameters: {}", n))),
    };
    applied_inner.extend_from_slice(&params_array_header);

    // Append each parameter (already CBOR-encoded)
    for param in params {
        applied_inner.extend_from_slice(param);
    }

    // Second element: original inner script bytes
    applied_inner.extend_from_slice(inner_script);

    // Wrap in CBOR byte string (re-encode as double-encoded script)
    let mut applied_bytes = Vec::new();
    {
        let mut encoder = Encoder::new(&mut applied_bytes);
        encoder.bytes(&applied_inner)
            .map_err(|e| PlutusError::CborEncode(e.to_string()))?;
    }

    // Create new script with same version but applied parameters
    PlutusScript::from_cbor(applied_bytes, script.version())
}
