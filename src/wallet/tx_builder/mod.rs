// Plutus transaction builder module
//
// This module provides transaction building for Plutus scripts using pallas-txbuilder.
// It wraps pallas types with wallet-specific functionality for easier use.

pub mod builder;
pub mod types;

pub use builder::PlutusTransactionBuilder;
pub use types::{PlutusInput, PlutusOutput};

use thiserror::Error;

#[derive(Error, Debug)]
pub enum TxBuilderError {
    #[error("Transaction builder error: {0}")]
    BuildError(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Invalid output: {0}")]
    InvalidOutput(String),

    #[error("Insufficient collateral")]
    InsufficientCollateral,

    #[error("Script error: {0}")]
    ScriptError(String),

    #[error("Plutus error: {0}")]
    Plutus(#[from] crate::wallet::plutus::PlutusError),

    #[error("Pallas error: {0}")]
    Pallas(String),

    #[error("CBOR encoding error: {0}")]
    CborEncode(String),
}

pub type TxBuilderResult<T> = Result<T, TxBuilderError>;
