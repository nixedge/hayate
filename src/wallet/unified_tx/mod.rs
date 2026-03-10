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

#[allow(unused_imports)]
pub use builder::UnifiedTxBuilder;
#[allow(unused_imports)]
pub use error::UnifiedTxError;
#[allow(unused_imports)]
pub use types::*;
