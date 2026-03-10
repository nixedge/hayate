// Wallet module for Hayate
// Implements pure Rust wallet functionality with GPG encrypted mnemonics

pub mod mnemonic;
pub mod storage;
pub mod derivation;
pub mod transaction;
pub mod utxorpc_client;
pub mod cli;
#[allow(dead_code)]
pub mod plutus;
#[allow(dead_code)]
pub mod tx_builder;
#[allow(clippy::module_inception)]
pub mod wallet;
#[allow(dead_code)]
pub mod unified_tx;

pub use storage::*;
pub use cli::handle_wallet_command;
pub use wallet::Wallet;
