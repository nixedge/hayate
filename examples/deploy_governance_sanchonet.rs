// Deploy governance contracts to SanchoNet with proper datums
//
// This reads governance member keys and constructs proper VersionedMultisig datums

use hayate::wallet::{Wallet, Network};
use hayate::wallet::unified_tx::UnifiedTxBuilder;
use hayate::wallet::plutus::{PlutusScript, DatumOption, Network as PlutusNetwork, VersionedMultisig, GovernanceMember};
use std::sync::Arc;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct GovernanceKeyFile {
    cardano_hash: String,
    sr25519_key: String,
}

fn load_governance_member(path: &str) -> Result<GovernanceMember, Box<dyn std::error::Error>> {
    let json_data = std::fs::read_to_string(path)?;
    let key_file: GovernanceKeyFile = serde_json::from_str(&json_data)?;

    // Parse hex strings to byte arrays
    let cardano_hash_vec = hex::decode(&key_file.cardano_hash)?;
    let sr25519_vec = hex::decode(&key_file.sr25519_key)?;

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

async fn deploy_contract(
    wallet: Arc<Wallet>,
    endpoint: String,
    contract_path: &str,
    contract_name: &str,
    members: Vec<GovernanceMember>,
    threshold: u32,
    deployment_ada: f64,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    println!("\n=== Deploying {} ===", contract_name);

    // Load contract script
    let script_cbor = std::fs::read(contract_path)?;
    let contract_script = PlutusScript::v2_from_cbor(script_cbor)?;

    // Get contract address
    let contract_address_bytes = contract_script.address(PlutusNetwork::Testnet)?;
    let contract_address = pallas_addresses::Address::from_bytes(&contract_address_bytes)
        .map_err(|e| format!("Failed to parse address: {}", e))?
        .to_bech32()
        .map_err(|e| format!("Failed to encode bech32: {}", e))?;

    println!("  Contract: {}", contract_path);
    println!("  Address:  {}", contract_address);
    println!("  Members:  {}", members.len());
    println!("  Threshold: {}", threshold);
    println!("  Amount:   {} ADA", deployment_ada);

    // Create VersionedMultisig datum
    let datum = VersionedMultisig {
        threshold,
        members,
        logic_round: 0,
    };

    let datum_cbor = datum.to_cbor()?;
    let datum_option = DatumOption::inline(datum_cbor);

    let deployment_lovelace = (deployment_ada * 1_000_000.0) as u64;

    // Build and submit deployment transaction
    println!("  Building transaction...");

    let tx_hash = UnifiedTxBuilder::new(wallet, endpoint)
        .await?
        .query_utxos().await?
        .pay_to_script_with_assets(
            contract_address_bytes,
            deployment_lovelace,
            Vec::new(),
            datum_option,
            Some(contract_script),
        )?
        .auto_collateral().await?
        .build_sign_submit().await?;

    println!("  ✅ Deployed! TX Hash: {}", hex::encode(&tx_hash));

    Ok(tx_hash)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Governance Contract Deployment to SanchoNet ===\n");

    // Get configuration from environment
    let mnemonic_str = std::env::var("MNEMONIC")
        .expect("MNEMONIC environment variable required");

    let endpoint = std::env::var("UTXORPC_ENDPOINT")
        .unwrap_or_else(|_| "http://127.0.0.1:50051".to_string());

    let gov_keys_dir = std::env::var("GOV_KEYS_DIR")
        .unwrap_or_else(|_| "/home/sam/work/iohk/midnight-playground/gov-keys".to_string());

    let contracts_dir = std::env::var("CONTRACTS_DIR")
        .unwrap_or_else(|_| "/home/sam/work/iohk/midnight-playground/contracts".to_string());

    println!("Configuration:");
    println!("  UTxORPC Endpoint: {}", endpoint);
    println!("  Gov Keys Dir:     {}", gov_keys_dir);
    println!("  Contracts Dir:    {}\n", contracts_dir);

    // Create wallet
    println!("Initializing wallet...");
    let wallet = Arc::new(Wallet::from_mnemonic_str(&mnemonic_str, Network::Testnet, 0)?);

    // Show wallet address
    let enterprise_addr = wallet.enterprise_address(0)?;
    println!("  Enterprise Address: {}\n", enterprise_addr);

    // Load Council governance members
    println!("Loading governance members...");
    let council_members = vec![
        load_governance_member(&format!("{}/council-1.json", gov_keys_dir))?,
        load_governance_member(&format!("{}/council-2.json", gov_keys_dir))?,
        load_governance_member(&format!("{}/council-3.json", gov_keys_dir))?,
    ];
    println!("  Council members: {}", council_members.len());

    // Load Technical Authority members
    let ta_members = vec![
        load_governance_member(&format!("{}/ta-1.json", gov_keys_dir))?,
        load_governance_member(&format!("{}/ta-2.json", gov_keys_dir))?,
        load_governance_member(&format!("{}/ta-3.json", gov_keys_dir))?,
    ];
    println!("  TA members: {}", ta_members.len());

    // Deploy Council governance contract
    let council_tx = deploy_contract(
        wallet.clone(),
        endpoint.clone(),
        &format!("{}/council_governance_council_governance.plutus", contracts_dir),
        "Council Governance",
        council_members,
        2,  // threshold
        50.0,  // 50 ADA
    ).await?;

    // Deploy Technical Authority contract
    let ta_tx = deploy_contract(
        wallet.clone(),
        endpoint.clone(),
        &format!("{}/tech_auth_governance_tech_auth_governance.plutus", contracts_dir),
        "Technical Authority",
        ta_members,
        2,  // threshold
        50.0,  // 50 ADA
    ).await?;

    println!("\n========================================");
    println!("✅ All governance contracts deployed!");
    println!("========================================");
    println!("\nTransaction Hashes:");
    println!("  Council:    {}", hex::encode(&council_tx));
    println!("  Tech Auth:  {}", hex::encode(&ta_tx));
    println!("\nNext steps:");
    println!("1. Wait for transactions to confirm");
    println!("2. Query UTxOs to get contract references");
    println!("3. Update genesis configuration with contract addresses");

    Ok(())
}
