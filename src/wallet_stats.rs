// Wallet statistics queries

use crate::indexer::{Network, NetworkStorage};
use anyhow::Result;
use std::path::PathBuf;

pub struct WalletStats {
    pub utxo_count: usize,
    pub total_balance: u64,
    pub tx_count: usize,
    pub addresses_used: usize,
}

pub fn get_wallet_stats(
    db_path: PathBuf,
    network: Network,
    wallet_id: Option<String>,
) -> Result<WalletStats> {
    let storage = NetworkStorage::open(db_path, network)?;

    // Count UTxOs
    let mut utxo_count = 0;
    for _ in storage.utxo_tree.iter() {
        utxo_count += 1;
    }

    // Count address->UTxO mappings
    let mut address_utxo_count = 0;
    let mut addresses_used = std::collections::HashSet::new();
    for (key, _) in storage.address_utxo_index.iter() {
        address_utxo_count += 1;

        // Extract address from key (format: "address:utxo_key")
        if let Ok(key_str) = std::str::from_utf8(key.as_ref()) {
            if let Some(address) = key_str.split(':').next() {
                addresses_used.insert(address.to_string());
            }
        }
    }

    // Count transactions
    let mut tx_count = 0;
    let mut tx_hashes = std::collections::HashSet::new();
    for (key, _) in storage.address_tx_index.iter() {
        // Extract tx hash from key (format: "address:tx_hash")
        if let Ok(key_str) = std::str::from_utf8(key.as_ref()) {
            if let Some(tx_hash) = key_str.split(':').nth(1) {
                tx_hashes.insert(tx_hash.to_string());
            }
        }
    }
    tx_count = tx_hashes.len();

    // Get total balance by querying each unique address
    let mut total_balance = 0u64;
    for address in &addresses_used {
        let address_key = cardano_lsm::Key::from(address.as_bytes());
        if let Ok(balance) = storage.balance_tree.get(&address_key) {
            total_balance += balance;
        }
    }

    Ok(WalletStats {
        utxo_count,
        total_balance,
        tx_count,
        addresses_used: addresses_used.len(),
    })
}

pub fn print_wallet_stats(stats: WalletStats, network: &Network) {
    println!("\n疾風 Hayate Wallet Statistics");
    println!("================================");
    println!("Network:         {}", network.as_str());
    println!("UTxOs:           {}", stats.utxo_count);
    println!("Addresses used:  {}", stats.addresses_used);
    println!("Transactions:    {}", stats.tx_count);
    println!("Total balance:   {} lovelace", stats.total_balance);
    println!("                 {} ADA", stats.total_balance as f64 / 1_000_000.0);
    println!();
}
