// Check if wallet has UTxOs in hayate

use hayate::wallet::{Wallet, Network};
use hayate::wallet::utxorpc_client::WalletUtxorpcClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mnemonic_str = std::env::var("MNEMONIC")
        .expect("MNEMONIC environment variable required");

    let endpoint = std::env::var("UTXORPC_ENDPOINT")
        .unwrap_or_else(|_| "http://127.0.0.1:50051".to_string());

    println!("=== Checking Wallet UTxOs ===\n");
    println!("UTxORPC Endpoint: {}\n", endpoint);

    // Create wallet
    let wallet = Wallet::from_mnemonic_str(&mnemonic_str, Network::Testnet, 0)?;

    // Show addresses
    println!("Wallet Addresses:");
    for i in 0..5 {
        let payment_addr = wallet.payment_address(i)?;
        let enterprise_addr = wallet.enterprise_address(i)?;
        println!("  [{}] Payment:    {}", i, payment_addr);
        println!("      Enterprise: {}", enterprise_addr);
    }

    // Query UTxOs
    println!("\nQuerying UTxOs from hayate...");
    let mut client = WalletUtxorpcClient::connect(endpoint).await?;

    // Collect all addresses (both payment and enterprise)
    let mut all_addresses = Vec::new();
    for i in 0..20 {
        all_addresses.push(wallet.payment_address_bytes(i)?);
        all_addresses.push(wallet.enterprise_address_bytes(i)?);
    }

    let utxos = client.query_utxos(all_addresses).await?;

    println!("\nResults:");
    println!("  Total UTxOs: {}", utxos.len());

    let mut total_lovelace: u64 = 0;
    for (addr_idx, utxo) in utxos.iter().enumerate() {
        total_lovelace += utxo.coin;
        println!("\n  UTxO #{}: {}#{}",
            addr_idx + 1,
            hex::encode(&utxo.tx_hash),
            utxo.output_index
        );
        println!("    Amount: {} lovelace ({:.6} ADA)", utxo.coin, utxo.coin as f64 / 1_000_000.0);
        println!("    Address: {}", String::from_utf8_lossy(&utxo.address));
    }

    println!("\n=== Summary ===");
    println!("Total Balance: {} lovelace ({:.6} ADA)", total_lovelace, total_lovelace as f64 / 1_000_000.0);

    Ok(())
}
