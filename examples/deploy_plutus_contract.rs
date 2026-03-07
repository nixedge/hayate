// Example: Deploying a Plutus contract to testnet
//
// This example demonstrates how to use PlutusTransactionBuilder to deploy
// a Plutus script contract with an inline datum to a Cardano testnet.
//
// This is typically used for deploying governance contracts to SanchoNet
// before launching a new Midnight network.

use hayate::wallet::plutus::{
    datum::datum_hash, DatumOption, Network, PlutusScript, PlutusVersion, VersionedMultisig,
};
use hayate::wallet::tx_builder::{PlutusInput, PlutusOutput, PlutusTransactionBuilder};
use hayate::wallet::utxorpc_client::UtxoData;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Plutus Contract Deployment Example ===\n");

    // 1. Load your Plutus script (this would typically come from a compiled contract)
    let script_cbor = vec![0x01, 0x02, 0x03, 0x04]; // Example CBOR bytes
    let script = PlutusScript::v2_from_cbor(script_cbor)?;

    println!("Script hash: {}", hex::encode(script.hash()));
    println!(
        "Script address: {}\n",
        hex::encode(script.address(Network::Testnet)?)
    );

    // 2. Create the governance datum (VersionedMultisig)
    let datum = VersionedMultisig {
        threshold: 2,
        members: vec![
            hayate::wallet::plutus::GovernanceMember {
                cardano_hash: [1u8; 28],  // First member's Cardano key hash
                sr25519_key: [2u8; 32],   // First member's SR25519 key
            },
            hayate::wallet::plutus::GovernanceMember {
                cardano_hash: [3u8; 28],  // Second member's Cardano key hash
                sr25519_key: [4u8; 32],   // Second member's SR25519 key
            },
            hayate::wallet::plutus::GovernanceMember {
                cardano_hash: [5u8; 28],  // Third member's Cardano key hash
                sr25519_key: [6u8; 32],   // Third member's SR25519 key
            },
        ],
        logic_round: 0,
    };

    let datum_cbor = datum.to_cbor()?;
    println!("Datum CBOR: {}", hex::encode(&datum_cbor));
    println!(
        "Datum hash: {}\n",
        hex::encode(datum_hash(&datum_cbor))
    );

    // 3. Create the transaction builder
    let change_address = vec![0x00; 57]; // Your change address (payment address)
    let mut builder = PlutusTransactionBuilder::new(Network::Testnet, change_address);

    // 4. Add funding input (this would come from querying UTxOs via UTxORPC)
    // In a real scenario, you'd query your wallet's UTxOs using hayate's utxorpc_client
    let funding_utxo = UtxoData {
        tx_hash: vec![0u8; 32],
        output_index: 0,
        address: vec![0x00; 57], // Your payment address
        coin: 100_000_000,       // 100 ADA
        assets: Vec::new(),
        datum_hash: None,
        datum: None,
    };

    let funding_input = PlutusInput::regular(funding_utxo);
    builder.add_input(&funding_input)?;

    // 5. Create output sending funds to the script address with inline datum
    let script_address = script.address(Network::Testnet)?;
    let contract_output = PlutusOutput::new(script_address, 50_000_000) // 50 ADA locked in contract
        .with_datum(DatumOption::inline(datum_cbor))
        .with_script_ref(script.clone()); // Include script reference for future use

    builder.add_output(&contract_output)?;

    // 6. Add change output (optional, but recommended)
    // In a real implementation, you'd calculate the exact change amount
    // based on inputs, outputs, and fees

    // 7. Add collateral (required for Plutus transactions)
    // Even for contract deployment without script execution, collateral is needed
    let collateral_utxo = UtxoData {
        tx_hash: vec![0u8; 32],
        output_index: 1,
        address: vec![0x00; 57],
        coin: 5_000_000, // 5 ADA collateral
        assets: Vec::new(),
        datum_hash: None,
        datum: None,
    };
    builder.add_collateral(&collateral_utxo)?;

    // 8. Set transaction parameters
    builder
        .set_fee(200_000) // 0.2 ADA fee
        .set_ttl(1000000) // TTL in slots (adjust based on current slot)
        .set_network_id()
        .set_default_language_view(PlutusVersion::V2);

    // 9. Build the transaction
    println!("Building transaction...");
    let (tx_bytes, tx_hash) = builder.build()?;

    println!("Transaction hash: {}", hex::encode(&tx_hash));
    println!("Transaction CBOR: {}", hex::encode(&tx_bytes));
    println!("Transaction size: {} bytes\n", tx_bytes.len());

    // 10. Sign and submit (not shown here - would use hayate wallet signing)
    println!("Next steps:");
    println!("1. Sign the transaction with your wallet keys");
    println!("2. Submit to the network via UTxORPC or cardano-submit-api");
    println!("3. Wait for confirmation");
    println!("4. Use the UTxO reference in your genesis configuration");

    Ok(())
}
