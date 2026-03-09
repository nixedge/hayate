// Example: Deploy a Plutus contract using UnifiedTxBuilder
//
// This demonstrates deploying a Plutus contract with datum and optional
// script reference. This is what midnight-cli would use for contract deployment.

use hayate::wallet::{Wallet, Network};
use hayate::wallet::unified_tx::UnifiedTxBuilder;
use hayate::wallet::plutus::{PlutusScript, DatumOption, Network as PlutusNetwork};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("=== Unified Transaction Builder - Deploy Contract ===\n");

    // Get configuration from environment
    let mnemonic_str = std::env::var("MNEMONIC")
        .expect("MNEMONIC environment variable required");

    let contract_path = std::env::var("CONTRACT_SCRIPT")
        .unwrap_or_else(|_| "contract.plutus".to_string());

    let endpoint = std::env::var("UTXORPC_ENDPOINT")
        .unwrap_or_else(|_| "http://127.0.0.1:50051".to_string());

    println!("Configuration:");
    println!("  Endpoint: {}", endpoint);
    println!("  Contract: {}\n", contract_path);

    // Create wallet
    let wallet = Arc::new(Wallet::from_mnemonic_str(&mnemonic_str, Network::Testnet, 0)?);

    // Load contract script
    println!("Loading contract script...");
    let script_cbor = std::fs::read(&contract_path)
        .unwrap_or_else(|_| {
            println!("⚠️  Could not load contract script, using example...");
            // Example empty script for demonstration
            vec![0x58, 0x01, 0x00] // Minimal CBOR script
        });

    let contract_script = PlutusScript::v2_from_cbor(script_cbor)?;

    // Get contract address
    let contract_address_bytes = contract_script.address(PlutusNetwork::Testnet)?;
    let contract_address = pallas_addresses::Address::from_bytes(&contract_address_bytes)
        .map_err(|e| format!("Failed to parse address: {}", e))?
        .to_bech32()
        .map_err(|e| format!("Failed to encode bech32: {}", e))?;
    println!("  Contract Address: {}\n", contract_address);

    // Create datum (example: integer constructor with value 42)
    // In a real scenario, this would be your contract's specific datum structure
    let datum_cbor = vec![0xd8, 0x7a, 0x18, 0x2a]; // Constructor 0 with integer 42
    let datum = DatumOption::inline(datum_cbor);

    // Deployment parameters
    let deployment_ada = 50.0; // Lock 50 ADA with the contract
    let deployment_lovelace = (deployment_ada * 1_000_000.0) as u64;

    println!("Deployment:");
    println!("  Amount:    {} ADA ({} lovelace)", deployment_ada, deployment_lovelace);
    println!("  Datum:     inline");
    println!("  Reference: embedded\n");

    // Build and submit deployment transaction
    println!("Building deployment transaction...");

    let tx_hash = UnifiedTxBuilder::new(wallet, endpoint)
        .await?
        .query_utxos().await?
        .pay_to_script_with_assets(
            contract_address_bytes,
            deployment_lovelace,
            Vec::new(),  // No native assets
            datum,
            Some(contract_script),  // Include script reference
        )?
        .auto_collateral().await?  // Required for script reference
        .build_sign_submit().await?;

    println!("\n✅ Contract deployed successfully!");
    println!("   TX Hash: {}", hex::encode(&tx_hash));
    println!("\n📍 Contract is now live at: {}", contract_address);
    println!("   - Locked: {} ADA", deployment_ada);
    println!("   - Datum: inline");
    println!("   - Script: available as reference\n");

    println!("Compare with manual approach:");
    println!("  Manual approach:  ~100+ lines of code");
    println!("  Unified approach: ~10 lines of code (90% reduction)\n");

    Ok(())
}
