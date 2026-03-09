// Example: Simple ADA send using UnifiedTxBuilder
//
// This demonstrates the high-level unified transaction API for midnight-cli
// and other wallet applications.

use hayate::wallet::{Wallet, Network};
use hayate::wallet::unified_tx::UnifiedTxBuilder;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("=== Unified Transaction Builder - Simple Send ===\n");

    // Get configuration from environment
    let mnemonic_str = std::env::var("MNEMONIC")
        .expect("MNEMONIC environment variable required");

    let recipient = std::env::var("RECIPIENT")
        .unwrap_or_else(|_| "addr_test1qz2fxv2umyhttkxyxp8x0dlpdt3k6cwng5pxj3jhsydzer3jcu5d8ps7zex2k2xt3uqxgjqnnj83ws8lhrn648jjxtwq2ytjqp".to_string());

    let amount_ada: f64 = std::env::var("AMOUNT")
        .unwrap_or_else(|_| "5.0".to_string())
        .parse()?;

    let amount_lovelace = (amount_ada * 1_000_000.0) as u64;

    let endpoint = std::env::var("UTXORPC_ENDPOINT")
        .unwrap_or_else(|_| "http://127.0.0.1:50051".to_string());

    println!("Configuration:");
    println!("  Endpoint:   {}", endpoint);
    println!("  Recipient:  {}", recipient);
    println!("  Amount:     {} ADA ({} lovelace)\n", amount_ada, amount_lovelace);

    // Create wallet
    let wallet = Arc::new(Wallet::from_mnemonic_str(&mnemonic_str, Network::Testnet, 0)?);

    // Show wallet addresses
    let payment_addr = wallet.payment_address(0)?;
    let enterprise_addr = wallet.enterprise_address(0)?;
    println!("Wallet addresses:");
    println!("  Payment:    {}", payment_addr);
    println!("  Enterprise: {}\n", enterprise_addr);

    // Build, sign, and submit transaction using unified API
    println!("Building transaction...");

    let tx_hash = UnifiedTxBuilder::new(wallet, endpoint)
        .await?
        .query_utxos().await?        // Query UTxOs from network
        .send_ada(&recipient, amount_lovelace)?  // Add send output
        .build_sign_submit().await?; // Build, sign, and submit

    println!("\n✅ Transaction submitted successfully!");
    println!("   TX Hash: {}\n", hex::encode(&tx_hash));

    println!("Compare with manual approach:");
    println!("  Manual: ~50 lines of code");
    println!("  Unified: 4 lines of code (80% reduction)\n");

    Ok(())
}
