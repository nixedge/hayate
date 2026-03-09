// Unified Transaction Builder
//
// High-level API for building Cardano transactions with automatic:
// - UTxO querying and selection
// - Fee calculation from protocol parameters
// - Change management with min-utxo constraints
// - Support for simple sends, scripts, minting, and burning

pub mod types;
pub mod error;
pub mod builder;

pub use types::*;
pub use error::*;
pub use builder::UnifiedTxBuilder;
