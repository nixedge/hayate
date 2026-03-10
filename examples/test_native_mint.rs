// Minimal test case for native script minting
//
// Tests if native script witnesses work correctly with UnifiedTxBuilder

use hayate::wallet::{Wallet, Network};
use hayate::wallet::unified_tx::UnifiedTxBuilder;
use hayate::wallet::utxorpc_client::AssetData;
use std::sync::Arc;
use pallas_crypto::hash::Hasher;
use pallas_crypto::key::ed25519::SecretKey;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Minimal Native Script Minting Test ===\n");

    // Get mnemonic from environment
    let mnemonic_str = std::env::var("MNEMONIC")
        .expect("MNEMONIC environment variable required");

    let endpoint = std::env::var("UTXORPC_ENDPOINT")
        .unwrap_or_else(|_| "http://127.0.0.1:50051".to_string());

    println!("UTxORPC Endpoint: {}\n", endpoint);

    // Create wallet
    let wallet = Arc::new(Wallet::from_mnemonic_str(&mnemonic_str, Network::Testnet, 0)?);
    let recipient_addr = wallet.enterprise_address(0)?;

    println!("Wallet Address: {}\n", recipient_addr);

    // Generate temporary keypair for native script
    use rand::rngs::OsRng;
    let temp_secret = SecretKey::new(OsRng);
    let temp_public = temp_secret.public_key();
    let temp_public_bytes: [u8; 32] = temp_public.into();
    let vkey_hash: pallas_crypto::hash::Hash<28> = Hasher::<224>::hash(&temp_public_bytes);

    println!("Generated temporary keypair");
    println!("  VKey Hash: {}\n", hex::encode(&vkey_hash));

    // Create native script minting policy
    use hayate::wallet::plutus::oneshot::TempKeyMintPolicy;
    let mut vkey_hash_bytes = [0u8; 28];
    vkey_hash_bytes.copy_from_slice(vkey_hash.as_ref());
    let mint_policy = TempKeyMintPolicy::new(vkey_hash_bytes);
    let native_script = mint_policy.to_native_script()?;
    let policy_id = mint_policy.policy_id()?;

    println!("Native Script Policy:");
    println!("  Policy ID: {}", hex::encode(&policy_id));
    println!("  Script CBOR: {}\n", hex::encode(&native_script));

    // Token name
    let token_name = b"TEST";

    // Build NFT asset data
    let nft_asset = AssetData {
        policy_id: policy_id.to_vec(),
        asset_name: token_name.to_vec(),
        amount: 1,
    };

    // Build transaction
    println!("Building transaction...");

    let mut builder = UnifiedTxBuilder::new(wallet.clone(), endpoint)
        .await?;

    builder
        .query_utxos().await?
        .mint_with_native_script(native_script.clone(), policy_id, token_name.to_vec(), 1)?
        .send_assets(&recipient_addr, 2_000_000, vec![nft_asset])?;

    println!("  Minting 1 token with policy {}", hex::encode(&policy_id));
    println!("  Sending to {}", recipient_addr);

    // Sign with wallet keys + temp key
    println!("\nSigning with wallet keys + temporary minting key...");
    let temp_private_key = pallas_wallet::PrivateKey::Normal(temp_secret);
    let signed_tx_bytes = builder.build_and_sign_with_keys(vec![temp_private_key]).await?;

    // Calculate tx hash
    use pallas_crypto::hash::Hasher as CryptoHasher;
    let tx_hash = CryptoHasher::<256>::hash(&signed_tx_bytes);

    println!("\n✅ Transaction built and signed successfully!");
    println!("  TX Hash: {}", hex::encode(&tx_hash));
    println!("  TX Size: {} bytes", signed_tx_bytes.len());

    // Save transaction
    let tx_file = "/tmp/test_native_mint.signed";
    let text_envelope = serde_json::json!({
        "type": "Tx ConwayEra",
        "description": "Ledger Cddl Format",
        "cborHex": hex::encode(&signed_tx_bytes)
    });
    std::fs::write(tx_file, serde_json::to_string_pretty(&text_envelope)?)?;

    println!("\n  Signed transaction saved to: {}", tx_file);
    println!("\n  To submit:");
    println!("    cardano-cli conway transaction submit \\");
    println!("      --testnet-magic 4 \\");
    println!("      --tx-file {} \\", tx_file);
    println!("      --socket-path /path/to/node.socket");

    Ok(())
}
