// Error types for unified transaction building

use thiserror::Error;

#[derive(Error, Debug)]
pub enum UnifiedTxError {
    #[error("Insufficient funds: need {need} lovelace, have {available} lovelace")]
    InsufficientFunds { need: u64, available: u64 },

    #[error("Insufficient assets: need {need} of {asset}, have {available}")]
    InsufficientAssets {
        asset: String,
        need: u64,
        available: u64,
    },

    #[error("No UTxOs available. Call query_utxos() first")]
    NoUtxos,

    #[error("No outputs specified")]
    NoOutputs,

    #[error("Collateral required for script transactions")]
    CollateralRequired,

    #[error("Invalid address: {0}")]
    InvalidAddress(String),

    #[error("Invalid bech32 address: {0}")]
    InvalidBech32(String),

    #[error("UTxORPC error: {0}")]
    UtxorpcError(#[from] anyhow::Error),

    #[error("Wallet error: {0}")]
    WalletError(#[from] crate::wallet::derivation::DerivationError),

    #[error("Builder error: {0}")]
    BuilderError(#[from] crate::wallet::tx_builder::TxBuilderError),

    #[error("Fee estimation failed: {0}")]
    FeeEstimationError(String),

    #[error("Protocol parameter error: {0}")]
    ProtocolParamError(#[from] crate::protocol_params::ProtocolParamError),

    #[error("Change amount below minimum UTxO requirement")]
    ChangeBelowMinimum,

    #[error("Transaction too large: {size} bytes exceeds maximum {max}")]
    TransactionTooLarge { size: usize, max: u64 },
}

pub type Result<T> = std::result::Result<T, UnifiedTxError>;
