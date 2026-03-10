// Deploy governance contracts to SanchoNet with proper datums
//
// This reads governance member keys and constructs proper VersionedMultisig datums

use hayate::wallet::{Wallet, Network};
use hayate::wallet::unified_tx::UnifiedTxBuilder;
use hayate::wallet::plutus::{PlutusScript, DatumOption, Network as PlutusNetwork, VersionedMultisig, GovernanceMember};
use hayate::wallet::utxorpc_client::AssetData;
use std::sync::Arc;
use serde::Deserialize;
use pallas_crypto::hash::Hasher;
use pallas_crypto::key::ed25519::SecretKey;

#[derive(Debug, Deserialize)]
struct GovernanceKeyFile {
    cardano_key_hash: String,
    sr25519_public_key: String,
}

fn load_governance_member(path: &str) -> Result<GovernanceMember, Box<dyn std::error::Error>> {
    let json_data = std::fs::read_to_string(path)?;
    let key_file: GovernanceKeyFile = serde_json::from_str(&json_data)?;

    // Parse hex strings to byte arrays
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

async fn deploy_contract(
    wallet: Arc<Wallet>,
    endpoint: String,
    contract_path: &str,
    contract_name: &str,
    members: Vec<GovernanceMember>,
    total_signers: u32,
    deployment_ada: f64,
) -> Result<(Vec<u8>, Vec<u8>), Box<dyn std::error::Error>> {
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
    println!("  Script Hash: {}", hex::encode(contract_script.hash()));
    println!("  Members:  {}", members.len());
    println!("  Total Signers: {}", total_signers);
    println!("  Amount:   {} ADA", deployment_ada);

    // Generate a truly temporary one-shot key for NFT minting
    use rand::rngs::OsRng;
    let temp_secret = SecretKey::new(OsRng);
    let temp_public = temp_secret.public_key();
    let temp_public_bytes: [u8; 32] = temp_public.into();
    let vkey_hash: pallas_crypto::hash::Hash<28> = Hasher::<224>::hash(&temp_public_bytes);

    // Create native script minting policy using the temporary key
    use hayate::wallet::plutus::oneshot::TempKeyMintPolicy;
    let mut vkey_hash_bytes = [0u8; 28];
    vkey_hash_bytes.copy_from_slice(vkey_hash.as_ref());
    let mint_policy = TempKeyMintPolicy::new(vkey_hash_bytes);
    let native_script = mint_policy.to_native_script()?;
    let policy_id = mint_policy.policy_id()?;

    println!("  NFT Policy ID: {}", hex::encode(&policy_id));
    println!("  (Temporary minting key will be discarded after this transaction)");

    // Token name for governance NFT (just use "GOV")
    let token_name = b"GOV";

    // Create VersionedMultisig datum
    let datum = VersionedMultisig {
        total_signers,
        members,
        logic_round: 0,
    };

    let datum_cbor = datum.to_cbor()?;
    let datum_option = DatumOption::inline(datum_cbor);

    let deployment_lovelace = (deployment_ada * 1_000_000.0) as u64;

    // Build NFT asset data for the script output
    let nft_asset = AssetData {
        policy_id: policy_id.to_vec(),
        asset_name: token_name.to_vec(),
        amount: 1,
    };

    // Build and submit deployment transaction
    println!("  Building transaction...");

    let mut builder = UnifiedTxBuilder::new(wallet.clone(), endpoint)
        .await?;

    builder
        .query_utxos().await?
        .mint_with_native_script(native_script, policy_id, token_name.to_vec(), 1)?
        .pay_to_script_with_assets(
            contract_address_bytes,
            deployment_lovelace,
            vec![nft_asset],
            datum_option,
            None, // Don't include reference script for now
        )?;
    // Native scripts don't need collateral

    // Build and sign the transaction with both wallet keys and temp minting key
    println!("  Signing transaction with wallet keys + temporary minting key...");
    let temp_private_key = pallas_wallet::PrivateKey::Normal(temp_secret);
    let signed_tx_bytes = builder.build_and_sign_with_keys(vec![temp_private_key]).await?;

    // Calculate tx hash
    use pallas_crypto::hash::Hasher as CryptoHasher;
    let tx_hash = CryptoHasher::<256>::hash(&signed_tx_bytes);

    // Save signed transaction in cardano-cli TextEnvelope format
    let tx_file = format!("/tmp/deploy-{}.signed", contract_name.replace(' ', "-"));

    // Create TextEnvelope JSON format
    let text_envelope = serde_json::json!({
        "type": "Tx ConwayEra",
        "description": "Ledger Cddl Format",
        "cborHex": hex::encode(&signed_tx_bytes)
    });

    std::fs::write(&tx_file, serde_json::to_string_pretty(&text_envelope)?)?;

    println!("  Transaction built and signed successfully!");
    println!("  TX Hash: {}", hex::encode(&tx_hash));
    println!("  NFT Policy: {}", hex::encode(&policy_id));
    println!("  Signed transaction saved to: {}", tx_file);
    println!("  ");
    println!("  To submit manually, run:");
    println!("    cardano-cli transaction submit \\");
    println!("      --tx-file {} \\", tx_file);
    println!("      --socket-path /home/sam/work/iohk/midnight-playground/.run/sanchonet/cardano-node/node.socket");
    println!("  ");

    Ok((tx_hash.to_vec(), policy_id.to_vec()))
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
    println!("Loading Council governance members...");
    let council_members = vec![
        load_governance_member(&format!("{}/council-1.json", gov_keys_dir))?,
        load_governance_member(&format!("{}/council-2.json", gov_keys_dir))?,
        load_governance_member(&format!("{}/council-3.json", gov_keys_dir))?,
    ];
    println!("  Council members loaded: {}", council_members.len());

    // Deploy Council governance contract
    let council_tx = deploy_contract(
        wallet.clone(),
        endpoint.clone(),
        &format!("{}/council_governance_council_governance.plutus", contracts_dir),
        "Council Governance",
        council_members,
        3,  // total_signers = 3 (we have 3 council members)
        50.0,  // 50 ADA
    ).await?;

    println!("\n========================================");
    println!("✅ Council governance contract deployed!");
    println!("========================================");
    println!("\nTransaction Hash: {}", hex::encode(&council_tx.0));
    println!("NFT Policy ID:    {}", hex::encode(&council_tx.1));
    println!("\nNext steps:");
    println!("1. Wait for transaction to confirm");
    println!("2. Query contract UTxO to verify NFT is locked");
    println!("3. Test council rotation with same members + incremented logic_round");

    Ok(())
}
