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
pub use datum::DatumOption;
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
