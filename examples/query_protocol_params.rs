// Example: Query protocol parameters from hayate
//
// This demonstrates what midnight-cli would receive when querying
// current protocol parameters for fee calculation.

use hayate::wallet::utxorpc_client::WalletUtxorpcClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("=== Query Protocol Parameters Example ===\n");

    // Connect to hayate UTxORPC endpoint
    let endpoint = std::env::var("UTXORPC_ENDPOINT")
        .unwrap_or_else(|_| "http://127.0.0.1:50051".to_string());

    println!("Connecting to: {}", endpoint);

    let mut client = WalletUtxorpcClient::connect(endpoint).await?;

    // Query protocol parameters
    println!("\nQuerying protocol parameters...");

    match client.query_protocol_params().await? {
        Some(params) => {
            println!("\n✅ Protocol Parameters Retrieved:\n");

            println!("📊 Fee Parameters:");
            println!("  min_fee_a:              {} (linear coefficient)", params.min_fee_a);
            println!("  min_fee_b:              {} lovelace (base fee)", params.min_fee_b);
            println!("  max_tx_size:            {} bytes", params.max_tx_size);
            println!("  max_block_body_size:    {} bytes", params.max_block_body_size);

            println!("\n💰 UTxO Parameters:");
            println!("  utxo_cost_per_byte:     {} lovelace/byte", params.utxo_cost_per_byte);
            if let Some(min_utxo) = params.min_utxo_lovelace {
                println!("  min_utxo_lovelace:      {} lovelace (legacy)", min_utxo);
            }

            println!("\n🔧 Plutus Parameters:");
            if let Some(price_mem) = params.price_memory {
                println!("  price_memory:           {}/{} = {:.6}",
                    price_mem.numerator, price_mem.denominator, price_mem.to_f64());
            }
            if let Some(price_steps) = params.price_steps {
                println!("  price_steps:            {}/{} = {:.9}",
                    price_steps.numerator, price_steps.denominator, price_steps.to_f64());
            }
            if let Some(max_tx_ex) = params.max_tx_execution_units {
                println!("  max_tx_execution_units:");
                println!("    memory:               {} units", max_tx_ex.mem);
                println!("    steps:                {} units", max_tx_ex.steps);
            }
            if let Some(max_block_ex) = params.max_block_execution_units {
                println!("  max_block_execution_units:");
                println!("    memory:               {} units", max_block_ex.mem);
                println!("    steps:                {} units", max_block_ex.steps);
            }

            println!("\n🎯 Stake Parameters:");
            println!("  key_deposit:            {} lovelace ({} ADA)",
                params.key_deposit, params.key_deposit / 1_000_000);
            println!("  pool_deposit:           {} lovelace ({} ADA)",
                params.pool_deposit, params.pool_deposit / 1_000_000);
            println!("  min_pool_cost:          {} lovelace ({} ADA)",
                params.min_pool_cost, params.min_pool_cost / 1_000_000);

            println!("\n📅 Metadata:");
            println!("  epoch:                  {}", params.epoch);

            println!("\n📐 Example Fee Calculations:");
            let small_tx_size = 300u64; // ~300 bytes for simple tx
            let small_fee = params.calculate_min_fee(small_tx_size);
            println!("  300-byte tx:            {} lovelace ({:.3} ADA)",
                small_fee, small_fee as f64 / 1_000_000.0);

            let large_tx_size = 10_000u64; // ~10KB for complex tx
            let large_fee = params.calculate_min_fee(large_tx_size);
            println!("  10KB tx:                {} lovelace ({:.3} ADA)",
                large_fee, large_fee as f64 / 1_000_000.0);

            println!("\n💎 Example Min-UTxO Calculations:");
            let simple_output = 50u64; // ~50 bytes
            let min_ada = params.calculate_min_utxo(simple_output);
            println!("  50-byte output:         {} lovelace ({:.3} ADA)",
                min_ada, min_ada as f64 / 1_000_000.0);

            let datum_output = 200u64; // ~200 bytes with datum
            let min_ada_datum = params.calculate_min_utxo(datum_output);
            println!("  200-byte output:        {} lovelace ({:.3} ADA)",
                min_ada_datum, min_ada_datum as f64 / 1_000_000.0);
        }
        None => {
            println!("\n⚠️  No protocol parameters available");
            println!("Make sure hayate is configured with a node socket_path");
        }
    }

    Ok(())
}
