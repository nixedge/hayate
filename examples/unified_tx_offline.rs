// Example: Offline transaction building with JSON files
//
// This demonstrates how to build transactions without a live node connection,
// using JSON files for protocol parameters and UTxOs. Useful for:
// - Air-gapped transaction signing
// - Testing without a node
// - Scenarios with limited network access

use hayate::wallet::{Wallet, Network};
use hayate::wallet::unified_tx::UnifiedTxBuilder;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("=== Offline Transaction Building ===\n");

    // Get configuration
    let mnemonic_str = std::env::var("MNEMONIC")
        .expect("MNEMONIC environment variable required");

    let recipient = std::env::var("RECIPIENT")
        .unwrap_or_else(|_| "addr_test1qz2fxv2umyhttkxyxp8x0dlpdt3k6cwng5pxj3jhsydzer3jcu5d8ps7zex2k2xt3uqxgjqnnj83ws8lhrn648jjxtwq2ytjqp".to_string());

    println!("Configuration:");
    println!("  Recipient: {}\n", recipient);

    // Create wallet
    let wallet = Arc::new(Wallet::from_mnemonic_str(&mnemonic_str, Network::Testnet, 0)?);

    println!("Step 1: Export protocol parameters to JSON");
    println!("--------------------------------------------------");
    println!("On a machine with node access, run:");
    println!("  cargo run --example query_protocol_params > protocol_params.json\n");

    println!("Step 2: Export wallet UTxOs to JSON");
    println!("--------------------------------------------------");
    println!("On a machine with node access, query your UTxOs and save to utxos.json");
    println!("Example utxos.json format:");
    println!(r#"[
  {{
    "tx_hash": "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890",
    "output_index": 0,
    "address": "00142857...",
    "coin": 10000000,
    "assets": []
  }}
]"#);
    println!();

    // Check if files exist
    let params_file = "protocol_params.json";
    let utxos_file = "utxos.json";

    if !std::path::Path::new(params_file).exists() {
        println!("⚠️  {} not found", params_file);
        println!("Creating example protocol params file...\n");

        // Create example protocol params
        use hayate::protocol_params::ProtocolParameters;
        let params = ProtocolParameters::preprod_defaults();
        let json = serde_json::to_string_pretty(&params)?;
        std::fs::write(params_file, json)?;
        println!("✅ Created {}", params_file);
    }

    if !std::path::Path::new(utxos_file).exists() {
        println!("⚠️  {} not found", utxos_file);
        println!("Creating example UTxOs file...\n");

        // Create example UTxOs
        use hayate::wallet::utxorpc_client::UtxoData;
        let utxos = vec![
            UtxoData {
                tx_hash: hex::decode("abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890")?,
                output_index: 0,
                address: wallet.payment_address_bytes(0)?,
                coin: 10_000_000, // 10 ADA
                assets: vec![],
                datum_hash: None,
                datum: None,
            }
        ];
        let json = serde_json::to_string_pretty(&utxos)?;
        std::fs::write(utxos_file, json)?;
        println!("✅ Created {}", utxos_file);
    }

    println!("\nStep 3: Build transaction offline");
    println!("--------------------------------------------------");

    // Build transaction offline using JSON files
    let mut builder = UnifiedTxBuilder::offline(wallet);

    let built_tx = builder
        .with_protocol_params_from_file(params_file)?
        .with_utxos_from_file(utxos_file)?
        .send_ada(&recipient, 5_000_000)?
        .build()
        .await?;

    println!("✅ Transaction built offline!");
    println!("   TX Hash: {}", hex::encode(&built_tx.tx_hash));
    println!("   TX Size: {} bytes", built_tx.tx_bytes.len());
    println!("   Fee:     {} lovelace ({:.3} ADA)", built_tx.fee_paid, built_tx.fee_paid as f64 / 1_000_000.0);
    println!("   Inputs:  {}", built_tx.inputs_used.len());
    println!("   Outputs: {}", built_tx.output_count);
    println!("   Change:  {} lovelace ({:.3} ADA)", built_tx.change_amount, built_tx.change_amount as f64 / 1_000_000.0);

    println!("\nStep 4: Save unsigned transaction");
    println!("--------------------------------------------------");
    let unsigned_tx_file = "unsigned_tx.cbor";
    std::fs::write(unsigned_tx_file, &built_tx.tx_bytes)?;
    println!("✅ Saved to {}", unsigned_tx_file);

    println!("\nStep 5: Sign transaction (air-gapped)");
    println!("--------------------------------------------------");
    println!("Transfer {} to your air-gapped machine", unsigned_tx_file);
    println!("Use build_and_sign() to sign the transaction");
    println!("Then transfer the signed transaction back for submission");

    println!("\n🔐 Offline Transaction Workflow Complete!");
    println!("\nBenefits of offline building:");
    println!("  ✓ No network connection required");
    println!("  ✓ Safe for air-gapped signing");
    println!("  ✓ Transaction parameters exported as JSON");
    println!("  ✓ Full control over the building process");

    Ok(())
}
