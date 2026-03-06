// Wallet module for Hayate
// Implements pure Rust wallet functionality with GPG encrypted mnemonics

pub mod mnemonic;
pub mod storage;
pub mod derivation;
pub mod transaction;
pub mod utxorpc_client;
pub mod cli;
pub mod plutus;
pub mod tx_builder;
pub mod wallet;

pub use storage::*;
pub use cli::handle_wallet_command;
pub use wallet::{Wallet, ed25519_secret_to_privatekey};
pub use derivation::Network;
