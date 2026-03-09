// Example: Mint native assets using UnifiedTxBuilder
//
// This demonstrates minting tokens with the unified transaction API.
// Supports both Plutus and native script policies.

use hayate::wallet::{Wallet, Network};
use hayate::wallet::unified_tx::UnifiedTxBuilder;
use hayate::wallet::plutus::{PlutusScript, Redeemer, RedeemerTag};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("=== Unified Transaction Builder - Minting ===\n");

    // Get configuration from environment
    let mnemonic_str = std::env::var("MNEMONIC")
        .expect("MNEMONIC environment variable required");

    let recipient = std::env::var("RECIPIENT")
        .expect("RECIPIENT environment variable required");

    let endpoint = std::env::var("UTXORPC_ENDPOINT")
        .unwrap_or_else(|_| "http://127.0.0.1:50051".to_string());

    let policy_script_path = std::env::var("POLICY_SCRIPT")
        .unwrap_or_else(|_| "policy.plutus".to_string());

    println!("Configuration:");
    println!("  Endpoint:      {}", endpoint);
    println!("  Recipient:     {}", recipient);
    println!("  Policy Script: {}\n", policy_script_path);

    // Create wallet
    let wallet = Arc::new(Wallet::from_mnemonic_str(&mnemonic_str, Network::Testnet, 0)?);

    // Load policy script
    println!("Loading policy script...");
    let script_cbor = std::fs::read(&policy_script_path)
        .unwrap_or_else(|_| {
            println!("⚠️  Could not load policy script, using example...");
            // Example empty script for demonstration
            vec![0x58, 0x01, 0x00] // Minimal CBOR script
        });

    let policy_script = PlutusScript::v2_from_cbor(script_cbor)?;
    let policy_id = policy_script.policy_id();

    println!("  Policy ID: {}\n", hex::encode(&policy_id));

    // Create mint operation
    let asset_name = b"MyToken".to_vec();
    let mint_amount = 1000i64; // Mint 1000 tokens
    let redeemer = Redeemer::empty(RedeemerTag::Mint, 0);

    println!("Minting:");
    println!("  Asset Name: {}", String::from_utf8_lossy(&asset_name));
    println!("  Amount:     {}\n", mint_amount);

    // Build transaction with minting
    println!("Building transaction...");

    let tx_hash = UnifiedTxBuilder::new(wallet, endpoint)
        .await?
        .query_utxos().await?
        .send_ada(&recipient, 2_000_000)?  // Send 2 ADA with minted tokens
        .mint_with_policy(policy_script, asset_name, mint_amount, redeemer)?
        .auto_collateral().await?  // Required for Plutus scripts
        .build_sign_submit().await?;

    println!("\n✅ Minting transaction submitted successfully!");
    println!("   TX Hash: {}\n", hex::encode(&tx_hash));

    println!("The recipient address now has:");
    println!("  - 2 ADA");
    println!("  - 1000 MyToken\n");

    Ok(())
}
