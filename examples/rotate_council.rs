// Rotate council governance by spending NFT from script and sending back with incremented logic_round
//
// This demonstrates the governance rotation mechanism

use hayate::wallet::{Wallet, Network};
use hayate::wallet::plutus::{PlutusScript, DatumOption, Network as PlutusNetwork, VersionedMultisig, GovernanceMember, Redeemer, RedeemerTag};
use hayate::wallet::utxorpc_client::{AssetData, WalletUtxorpcClient, UtxoData};
use hayate::wallet::tx_builder::{PlutusTransactionBuilder, PlutusOutput};
use std::sync::Arc;
use serde::Deserialize;
use pallas_addresses::Address;

#[derive(Debug, Deserialize)]
struct GovernanceKeyFile {
    cardano_key_hash: String,
    sr25519_public_key: String,
}

fn load_governance_member(path: &str) -> Result<GovernanceMember, Box<dyn std::error::Error>> {
    let json_data = std::fs::read_to_string(path)?;
    let key_file: GovernanceKeyFile = serde_json::from_str(&json_data)?;

    let cardano_hash_vec = hex::decode(&key_file.cardano_key_hash)?;
    let sr25519_vec = hex::decode(&key_file.sr25519_public_key)?;

    if cardano_hash_vec.len() != 28 {
        return Err(format!("Invalid cardano_hash length: {}", cardano_hash_vec.len()).into());
    }
    if sr25519_vec.len() != 32 {
        return Err(format!("Invalid sr25519_key length: {}", sr25519_vec.len()).into());
    }

    let mut cardano_hash = [0u8; 28];
    let mut sr25519_key = [0u8; 32];
    cardano_hash.copy_from_slice(&cardano_hash_vec);
    sr25519_key.copy_from_slice(&sr25519_vec);

    Ok(GovernanceMember {
        cardano_hash,
        sr25519_key,
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Council Governance Rotation ===\n");

    let gov_keys_dir = std::env::var("GOV_KEYS_DIR")
        .unwrap_or_else(|_| "/home/sam/work/iohk/midnight-playground/gov-keys".to_string());

    let contracts_dir = std::env::var("CONTRACTS_DIR")
        .unwrap_or_else(|_| "/home/sam/work/iohk/midnight-playground/contracts".to_string());

    let endpoint = std::env::var("UTXORPC_ENDPOINT")
        .unwrap_or_else(|_| "http://127.0.0.1:50051".to_string());

    // Load council governance members
    println!("Loading Council members...");
    let council_members = vec![
        load_governance_member(&format!("{}/council-1.json", gov_keys_dir))?,
        load_governance_member(&format!("{}/council-2.json", gov_keys_dir))?,
        load_governance_member(&format!("{}/council-3.json", gov_keys_dir))?,
    ];
    println!("  Loaded {} members\n", council_members.len());

    // Load council member mnemonics (we need at least 2 for threshold)
    println!("Loading council member signing keys...");
    let member1_mnemonic = std::fs::read_to_string(format!("{}/council-1.mnemonic", gov_keys_dir))?;
    let member2_mnemonic = std::fs::read_to_string(format!("{}/council-2.mnemonic", gov_keys_dir))?;

    let member1_wallet = Arc::new(Wallet::from_mnemonic_str(member1_mnemonic.trim(), Network::Testnet, 0)?);
    let member2_wallet = Arc::new(Wallet::from_mnemonic_str(member2_mnemonic.trim(), Network::Testnet, 0)?);

    // Load deployment wallet for collateral
    let deployment_mnemonic = std::env::var("MNEMONIC")
        .expect("MNEMONIC environment variable required for collateral");
    let deployment_wallet = Arc::new(Wallet::from_mnemonic_str(deployment_mnemonic.trim(), Network::Testnet, 0)?);

    println!("  Member 1: {}", member1_wallet.payment_address(0)?);
    println!("  Member 2: {}", member2_wallet.payment_address(0)?);
    println!("  Deployment wallet: {}\n", deployment_wallet.payment_address(0)?);

    // Load governance contract
    let contract_path = format!("{}/council_governance_council_governance.plutus", contracts_dir);
    let script_cbor = std::fs::read(&contract_path)?;
    let contract_script = PlutusScript::v2_from_cbor(script_cbor)?;
    let contract_address_bytes = contract_script.address(PlutusNetwork::Testnet)?;
    let contract_address = Address::from_bytes(&contract_address_bytes)
        .map_err(|e| format!("Failed to parse address: {}", e))?
        .to_bech32()
        .map_err(|e| format!("Failed to encode bech32: {}", e))?;

    println!("Contract address: {}", contract_address);
    println!("Script hash: {}\n", hex::encode(contract_script.hash()));

    // Query script UTxO with NFT
    println!("Querying script UTxO with NFT...");
    let mut client = WalletUtxorpcClient::connect(endpoint.clone()).await?;
    let script_utxos = client.query_utxos(vec![contract_address_bytes.clone()]).await?;

    println!("Found {} UTxOs at script address", script_utxos.len());

    // Find the one with the NFT (should have exactly 1 asset)
    let nft_utxo = script_utxos.iter()
        .find(|utxo| !utxo.assets.is_empty())
        .ok_or("No UTxO with NFT found at script address")?;

    println!("  UTxO: {}#{}", hex::encode(&nft_utxo.tx_hash), nft_utxo.output_index);
    println!("  ADA: {} lovelace", nft_utxo.coin);
    println!("  Assets: {}", nft_utxo.assets.len());

    if let Some(asset) = nft_utxo.assets.first() {
        println!("  NFT: {}:{}", hex::encode(&asset.policy_id), hex::encode(&asset.asset_name));
    }

    // Decode current datum
    let current_datum_bytes = nft_utxo.datum.as_ref()
        .ok_or("No datum found on script UTxO")?;

    // Parse current datum to get logic_round
    // For now, we'll manually construct the new datum with logic_round + 1
    let current_datum = VersionedMultisig::from_cbor(&current_datum_bytes)?;
    println!("\nCurrent datum:");
    println!("  total_signers: {}", current_datum.total_signers);
    println!("  members: {}", current_datum.members.len());
    println!("  logic_round: {}\n", current_datum.logic_round);

    // Create new datum with incremented logic_round
    let new_datum = VersionedMultisig {
        total_signers: current_datum.total_signers,
        members: council_members,  // Same members
        logic_round: current_datum.logic_round + 1,  // Increment!
    };

    println!("New datum:");
    println!("  logic_round: {} -> {}", current_datum.logic_round, new_datum.logic_round);

    // Create redeemer (UpdateRedeemer - just empty for now)
    let redeemer = Redeemer::empty(RedeemerTag::Spend, 0);

    // Build rotation transaction
    println!("\nBuilding rotation transaction...");

    let network = PlutusNetwork::Testnet;
    let change_address = member1_wallet.payment_address_bytes(0)?;
    let mut builder = PlutusTransactionBuilder::new(network, change_address);

    // Add script input (spending the NFT from the contract)
    builder.add_script_input(
        &nft_utxo,
        contract_script.clone(),
        redeemer,
        Some(current_datum_bytes.clone()),  // Provide current datum as witness
    )?;

    // Add output back to script with new datum
    let new_datum_cbor = new_datum.to_cbor()?;
    let datum_option = DatumOption::inline(new_datum_cbor);

    let script_output = PlutusOutput::with_assets(
        contract_address_bytes.clone(),
        nft_utxo.coin,
        nft_utxo.assets.clone()
    ).with_datum(datum_option);

    builder.add_output(&script_output)?;

    // Add collateral (required for script transactions)
    // Query deployment wallet for collateral
    let deployment_addr_bytes = deployment_wallet.payment_address_bytes(0)?;
    let deployment_utxos = client.query_utxos(vec![deployment_addr_bytes]).await?;

    // Find a pure ADA UTxO for collateral (5 ADA)
    let collateral_utxo = deployment_utxos.iter()
        .find(|utxo| utxo.assets.is_empty() && utxo.coin >= 5_000_000)
        .ok_or("No suitable collateral UTxO found")?;

    builder.add_collateral(collateral_utxo)?;

    // Set transaction parameters
    builder
        .set_fee(500_000)  // Estimate
        .set_network_id()
        .set_default_language_view(hayate::wallet::plutus::PlutusVersion::V2);

    println!("  Script input: spending NFT from contract");
    println!("  Script output: sending NFT back with logic_round = {}", new_datum.logic_round);
    println!("  Collateral: {} lovelace", collateral_utxo.coin);

    // Sign with council member keys (need 2/3 threshold)
    println!("\nSigning with council member keys (2/3 threshold)...");
    let signing_keys = vec![
        member1_wallet.payment_signing_key(0)?,
        member2_wallet.payment_signing_key(0)?,
    ];

    let signed_tx = builder.build_and_sign(signing_keys)?;

    // Calculate tx hash
    use pallas_crypto::hash::Hasher;
    let tx_hash = Hasher::<256>::hash(&signed_tx);

    println!("\n✅ Rotation transaction built and signed!");
    println!("  TX Hash: {}", hex::encode(&tx_hash));
    println!("  TX Size: {} bytes", signed_tx.len());

    // Save transaction
    let tx_file = "/tmp/rotate-council.signed";
    let text_envelope = serde_json::json!({
        "type": "Tx ConwayEra",
        "description": "Ledger Cddl Format",
        "cborHex": hex::encode(&signed_tx)
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
