// Integration tests for Plutus simulator with simple test contracts
//
// Tests three validators:
// 1. always_true - always succeeds
// 2. check_42 - requires redeemer to be 42
// 3. match_datum - requires redeemer to match datum value

use hayate::wallet::simulator::{LedgerState, TransactionSimulator};
use anyhow::Result;

// Contract CBORs from compiled Aiken validators
const ALWAYS_TRUE_CBOR: &str = "585c01010029800aba2aba1aab9eaab9dab9a4888896600264653001300600198031803800cc0180092225980099b8748008c01cdd500144c8cc892898050009805180580098041baa0028b200c180300098019baa0068a4d13656400401";
const CHECK_42_CBOR: &str = "585e01010029800aba2aba1aab9eaab9dab9a4888896600264646644b30013370e900118031baa002899199119b87375a601600c902a18048009804980500098039baa0028b200a30063007001300600230060013003375400d149a26cac8009";
const MATCH_DATUM_CBOR: &str = "587701010029800aba2aba1aab9eaab9dab9a4888896600264646644b30013370e900118031baa00289919912cc004cdc3a400060126ea8006266e1cdd6980598051baa001375a601600d14a08040c024004c024c028004c01cdd5001459005180318038009803001180300098019baa0068a4d13656400401";

// UTxO references from test ledger state
const ALWAYS_TRUE_UTXO: &str = "3b2aca1e92b2354d8a0baf786a693093e8323b743e5ac0e1fad3c10de8df8f87:0";
const CHECK_42_UTXO: &str = "5fa287aa60375f3bc4f2410c97a31d4c26461c749c9521a593df0da2ae0bb076:0";
const MATCH_DATUM_UTXO: &str = "07c7c78ef1545b9fe8e6ea8d2ef3209ef33a05e94e94f6ad61b55c34279ce671:0";
const WALLET_COLLATERAL_UTXO: &str = "85b369193c99224ac8f460a84b1d2ad438204f89ae008fdd61ab050597f564ac:0";


/// Build a transaction that spends a script UTxO using pallas-primitives directly
fn build_script_spend_tx(
    script_utxo_ref: &str,
    script_cbor: &str,
    redeemer_data: Vec<u8>,
    collateral_utxo_ref: &str,
) -> Result<Vec<u8>> {
    use pallas_primitives::conway::*;
    use pallas_primitives::{Fragment, NonEmptySet};
    use pallas_crypto::hash::Hash;

    // Parse UTxO references
    let parse_utxo_ref = |utxo_ref: &str| -> Result<Hash<32>> {
        let parts: Vec<&str> = utxo_ref.split(':').collect();
        let tx_hash_vec = hex::decode(parts[0])?;
        let tx_hash_array: [u8; 32] = tx_hash_vec.try_into()
            .map_err(|_| anyhow::anyhow!("Invalid tx hash length"))?;
        Ok(Hash::from(tx_hash_array))
    };

    let script_tx_hash = parse_utxo_ref(script_utxo_ref)?;
    let script_index = script_utxo_ref.split(':').nth(1).unwrap().parse::<u64>()?;
    let collateral_tx_hash = parse_utxo_ref(collateral_utxo_ref)?;
    let collateral_index = collateral_utxo_ref.split(':').nth(1).unwrap().parse::<u64>()?;

    // Decode the Aiken CBOR to get raw script bytes
    use pallas_codec::minicbor::{decode, bytes::ByteVec, Encoder};
    let aiken_cbor_bytes = hex::decode(script_cbor)?;
    let script_bytes_raw: ByteVec = decode(&aiken_cbor_bytes)?;

    // Verify the script can be loaded by uplc (using unwrapped bytes)
    use uplc::ast::Program;
    match Program::<uplc::ast::FakeNamedDeBruijn>::from_flat(&script_bytes_raw) {
        Ok(program) => {
            eprintln!("[DEBUG] ✓ Script successfully parsed by uplc");
            eprintln!("[DEBUG]   Version: {:?}", program.version);
        }
        Err(e) => {
            return Err(anyhow::anyhow!("Script failed to parse: {:?}", e));
        }
    }

    // Compute script hash from Aiken CBOR (NOT raw bytes!)
    // This is the correct approach - hash the CBOR-encoded script
    let computed_hash = Hasher::<224>::hash_tagged(&aiken_cbor_bytes, 3);

    // PlutusScript needs the Aiken CBOR (already has 585c wrapper)
    // Pallas will add another wrapper, creating the double-wrap structure
    use pallas_primitives::Bytes;
    let script_bytes_for_witness: Bytes = aiken_cbor_bytes.into();
    eprintln!("[DEBUG] Script bytes (raw): {} bytes", script_bytes_raw.len());
    eprintln!("[DEBUG] Script bytes hex: {}", hex::encode(&script_bytes_raw[..]));
    eprintln!("[DEBUG] Script hash (V3): {}", hex::encode(computed_hash));
    eprintln!("[DEBUG] Script bytes (for witness, with CBOR wrapper): {} bytes", script_bytes_for_witness.len());
    eprintln!("[DEBUG] Redeemer data bytes: {}", hex::encode(&redeemer_data));

    // Build transaction using primitives directly

    // 1. Transaction inputs
    let inputs = vec![TransactionInput {
        transaction_id: script_tx_hash.into(),
        index: script_index,
    }];

    // 2. Collateral inputs
    let collateral = vec![TransactionInput {
        transaction_id: collateral_tx_hash.into(),
        index: collateral_index,
    }];

    // 3. Decode PlutusData from redeemer bytes
    // Now that redeemer_data is plain CBOR (not constructor-wrapped), this will work correctly
    let redeemer_plutus_data = PlutusData::decode_fragment(&redeemer_data)
        .map_err(|e| anyhow::anyhow!("Failed to decode PlutusData: {:?}", e))?;

    // 4. Build redeemers as MAP format (Conway style)
    let redeemer_key = RedeemersKey {
        tag: RedeemerTag::Spend,
        index: 0,  // First input
    };
    let redeemer_value = RedeemersValue {
        data: redeemer_plutus_data,
        ex_units: ExUnits {
            mem: 1_000_000,      // 1M mem
            steps: 1_000_000_000, // 1B steps
        },
    };

    // In pallas 1.0, Redeemers::Map uses BTreeMap instead of NonEmptyKeyValuePairs
    use std::collections::BTreeMap;
    let mut redeemers_map = BTreeMap::new();
    redeemers_map.insert(redeemer_key, redeemer_value);

    // 4b. Calculate script_data_hash (required for Plutus transactions)
    // Script data hash is blake2b-256 of CBOR-encoded [redeemers, datums, language_views]
    // For now, we'll hash just the redeemers structure
    use pallas_crypto::hash::Hasher;
    let redeemers_cbor = Redeemers::Map(redeemers_map.clone())
        .encode_fragment()
        .map_err(|e| anyhow::anyhow!("Failed to encode redeemers: {:?}", e))?;
    let script_data_hash = Hasher::<256>::hash(&redeemers_cbor);

    // 5. Create wallet for signing and collateral address derivation
    use hayate::wallet::{Wallet, Network};
    use bip39::Mnemonic;

    const TEST_MNEMONIC: &str = "bottom drive obey lake curtain smoke basket hold race lonely fit walk";

    // Parse mnemonic
    let mnemonic = Mnemonic::parse(TEST_MNEMONIC)
        .map_err(|e| anyhow::anyhow!("Invalid mnemonic: {:?}", e))?;

    // Create wallet from test mnemonic
    let wallet = Wallet::from_mnemonic(mnemonic, Network::Testnet, 0)
        .map_err(|e| anyhow::anyhow!("Failed to create wallet: {:?}", e))?;

    // 6. Build collateral return output
    // Collateral return sends change back to the collateral address
    // Total collateral = 150% of fee (standard collateral percentage)
    let fee = 2_000_000u64;  // 2 ADA fee
    let collateral_amount = 10_000_000u64;  // 10 ADA collateral UTxO
    let total_collateral_amount = (fee as f64 * 1.5) as u64;  // 150% of fee
    let collateral_return_amount = collateral_amount - total_collateral_amount;

    // Build collateral return output (returns to same address as collateral came from)
    let collateral_addr_bytes = wallet.payment_address_bytes(0)
        .map_err(|e| anyhow::anyhow!("Failed to derive collateral address: {:?}", e))?;

    // Encode collateral return output as CBOR
    let mut collateral_return_cbor = Vec::new();
    {
        let mut enc = Encoder::new(&mut collateral_return_cbor);
        enc.array(2)?;  // [address, amount]
        enc.bytes(&collateral_addr_bytes)?;
        enc.u64(collateral_return_amount)?;
    }

    // Decode as TransactionOutput
    let collateral_return_output: TransactionOutput = decode(&collateral_return_cbor)
        .map_err(|e| anyhow::anyhow!("Failed to decode collateral return: {:?}", e))?;

    // 7. Build transaction body
    let tx_body = TransactionBody {
        inputs: inputs.into(),
        outputs: vec![],  // No output needed for this test
        fee,
        ttl: None,
        certificates: None.into(),
        withdrawals: None,
        auxiliary_data_hash: None,
        validity_interval_start: None,
        mint: None,
        script_data_hash: Some(script_data_hash.into()),  // Required for Plutus transactions!
        collateral: Some(NonEmptySet::from_vec(collateral.clone()).unwrap()),
        required_signers: None.into(),
        network_id: None,
        collateral_return: Some(collateral_return_output.into()),  // Required for Plutus!
        total_collateral: Some(total_collateral_amount),  // Required for Plutus!
        reference_inputs: None,  // No reference inputs needed - script is in witness set
        voting_procedures: None,
        proposal_procedures: None.into(),
        treasury_value: None,
        donation: None,
    };

    // 8. Sign the transaction with test mnemonic key
    // Get signing key for address index 0
    let signing_key = wallet.payment_signing_key(0)
        .map_err(|e| anyhow::anyhow!("Failed to derive signing key: {:?}", e))?;

    // Compute transaction body hash (Blake2b-256)
    let tx_body_cbor = tx_body.encode_fragment()
        .map_err(|e| anyhow::anyhow!("Failed to encode tx body: {:?}", e))?;
    let tx_body_hash = Hasher::<256>::hash(&tx_body_cbor);

    eprintln!("[DEBUG] Transaction body hash for signing: {}", hex::encode(&tx_body_hash));

    // Sign the hash
    let signature = signing_key.sign(&tx_body_hash);
    let vkey = signing_key.public_key();

    eprintln!("[DEBUG] Public key: {}", hex::encode(vkey.as_ref()));
    eprintln!("[DEBUG] Signature: {}", hex::encode(signature.as_ref()));

    // Create vkeywitness
    let vkeywitness = VKeyWitness {
        vkey: vkey.as_ref().to_vec().into(),
        signature: signature.as_ref().to_vec().into(),
    };

    // 9. Build witness set with MAP-format redeemers and signature
    let witness_set = WitnessSet {
        vkeywitness: Some(NonEmptySet::from_vec(vec![vkeywitness]).unwrap()),
        native_script: None,
        bootstrap_witness: None,
        plutus_v1_script: None,
        plutus_v2_script: None,
        plutus_v3_script: Some(NonEmptySet::from_vec(vec![
            PlutusScript::<3>(script_bytes_for_witness)
        ]).unwrap()),
        plutus_data: None,
        redeemer: Some(Redeemers::Map(redeemers_map).into()),
    };

    // 10. Build complete transaction
    let tx = Tx {
        transaction_body: tx_body.into(),
        transaction_witness_set: witness_set.into(),
        success: true,
        auxiliary_data: None.into(),
    };

    // 11. Encode transaction
    let tx_bytes = tx.encode_fragment().map_err(|e| anyhow::anyhow!("Failed to encode transaction: {:?}", e))?;

    eprintln!("\n[DEBUG build_script_spend_tx] Transaction CBOR ({} bytes):", tx_bytes.len());
    eprintln!("{}", hex::encode(&tx_bytes));
    eprintln!();

    Ok(tx_bytes)
}

#[test]
fn test_decode_our_transaction() -> Result<()> {
    use pallas_primitives::conway::Tx;
    use pallas_primitives::Fragment;

    println!("\n🧪 Testing if pallas can decode our transaction...");

    // Build a simple transaction
    let tx_bytes = build_script_spend_tx(
        ALWAYS_TRUE_UTXO,
        ALWAYS_TRUE_CBOR,
        {
            let mut redeemer_data = Vec::new();
            let mut enc = pallas_codec::minicbor::Encoder::new(&mut redeemer_data);
            enc.tag(pallas_codec::minicbor::data::Tag::new(121))?;
            enc.array(0)?;
            redeemer_data
        },
        WALLET_COLLATERAL_UTXO,
    )?;

    println!("  Transaction: {} bytes", tx_bytes.len());

    match Tx::decode_fragment(&tx_bytes) {
        Ok(tx) => {
            println!("  ✅ Pallas decoded successfully!");
            println!("    Inputs: {}", tx.transaction_body.inputs.len());
            println!("    V3 scripts: {}", tx.transaction_witness_set.plutus_v3_script.as_ref().map(|s| s.len()).unwrap_or(0));
        }
        Err(e) => {
            println!("  ❌ Pallas decode failed: {:?}", e);
            return Err(anyhow::anyhow!("Pallas decode failed: {:?}", e));
        }
    }

    // Now test decoding UTxO output
    println!("\n  Testing UTxO output decoding...");

    use pallas_primitives::conway::TransactionOutput;
    let ledger_state = LedgerState::from_file(std::path::Path::new("test-ledger-state.json"))?;
    let utxo_key = "3b2aca1e92b2354d8a0baf786a693093e8323b743e5ac0e1fad3c10de8df8f87:0";

    if let Some(output_cbor) = ledger_state.utxos.get(utxo_key) {
        println!("  UTxO output: {} bytes", output_cbor.len());
        println!("  Hex: {}", hex::encode(output_cbor));

        match TransactionOutput::decode_fragment(output_cbor) {
            Ok(output) => {
                println!("  ✅ Pallas decoded UTxO output successfully!");
            }
            Err(e) => {
                println!("  ❌ Pallas failed to decode UTxO output: {:?}", e);
                return Err(anyhow::anyhow!("UTxO output decode failed: {:?}", e));
            }
        }
    }

    Ok(())
}

#[test]
#[ignore] // Outdated transaction with old hash - use test_real_tx instead
fn test_cardano_cli_transaction() -> Result<()> {
    println!("\n🧪 Testing cardano-cli generated transaction with uplc directly...");

    // Cardano-CLI transaction
    let cardano_hex = "84a500d90102818258203b2aca1e92b2354d8a0baf786a693093e8323b743e5ac0e1fad3c10de8df8f87000dd901028182582085b369193c99224ac8f460a84b1d2ad438204f89ae008fdd61ab050597f564ac0012d90102818258203b2aca1e92b2354d8a0baf786a693093e8323b743e5ac0e1fad3c10de8df8f87000180021a001e8480a105a182000082d87980821a3b9aca001a000f4240f5f6";
    let tx_bytes = hex::decode(cardano_hex)?;

    // UTxO pair for always_true script (with correct hash - hash of Aiken CBOR)
    let utxo_input_hex = "8258203b2aca1e92b2354d8a0baf786a693093e8323b743e5ac0e1fad3c10de8df8f8700";
    let utxo_output_hex = "a300581d70d27ccc13fab5b782984a3d1f99353197ca1a81be069941ffc003ee75011a00989680028201d8184100";

    let utxo_pairs = vec![(
        hex::decode(utxo_input_hex)?,
        hex::decode(utxo_output_hex)?
    )];

    // Cost models (V3 only)
    let cost_models_hex = "a10298a61a000189b41901a401011903e818ad00011903e819ea350401192baf18201a000312591920a404193e801864193e801864193e801864193e801864193e801864193e80186418641864193e8018641a000170a718201a00020782182019f016041a0001194a18b2000119568718201a0001643519030104021a00014f581a00037c71187a0001011903e819a7a90402195fe419733a1826011a000db464196a8f0119ca3f19022e011999101903e819ecb2011a00022a4718201a000144ce1820193bc318201a0001291101193371041956540a197147184a01197147184a0119a9151902280119aecd19021d0119843c18201a00010a9618201a00011aaa1820191c4b1820191cdf1820192d1a18201a00014f581a00037c71187a0001011a0001614219020700011a000122c118201a00014f581a00037c71187a0001011a00014f581a00037c71187a0001011a0004213c19583c041a00163cad19fc3604194ff30104001a00022aa818201a000189b41901a401011a00013eff182019e86a1820194eae182019600c1820195108182019654d182019602f18201a032e93af1937fd0a";
    let cost_models = hex::decode(cost_models_hex)?;

    let max_ex_units = (14_000_000u64, 10_000_000_000u64);
    let slot_config = (1_666_656_000_000u64, 0u64, 1_000u32);

    println!("  Calling uplc::tx::eval_phase_two_raw...");
    match uplc::tx::eval_phase_two_raw(
        &tx_bytes,
        &utxo_pairs,
        Some(&cost_models),
        max_ex_units,
        slot_config,
        false,
        |_| {},
    ) {
        Ok(results) => {
            println!("  ✅ SUCCESS! {} redeemers evaluated", results.len());
            for (i, (_redeemer, result)) in results.iter().enumerate() {
                println!("    Redeemer {}: cost={{cpu={}, mem={}}}", i, result.cost().cpu, result.cost().mem);
            }
        }
        Err(e) => {
            println!("  ❌ FAILED: {}", e);
            return Err(anyhow::anyhow!("uplc evaluation failed: {}", e));
        }
    }

    Ok(())
}

#[test]
fn test_always_true_succeeds() -> Result<()> {
    println!("\n🧪 Testing always_true validator...");

    // Load ledger state
    let ledger_state = LedgerState::from_file(
        std::path::Path::new("test-ledger-state.json")
    )?;

    // Create redeemer (can be anything - using plain integer like cardano-cli does)
    // NOTE: cardano-cli encodes redeemers as plain CBOR values, not PlutusData constructors
    use pallas_codec::minicbor::Encoder;
    let mut redeemer_data = Vec::new();
    {
        let mut enc = Encoder::new(&mut redeemer_data);
        enc.u32(0)?;  // Plain integer 0 (matches cardano-cli format)
    }
    eprintln!("[DEBUG] Redeemer data bytes: {}", hex::encode(&redeemer_data));

    // Build transaction
    let tx_bytes = build_script_spend_tx(
        ALWAYS_TRUE_UTXO,
        ALWAYS_TRUE_CBOR,
        redeemer_data,
        WALLET_COLLATERAL_UTXO,
    )?;

    // Simulate
    let simulator = TransactionSimulator::new_offline();
    let result = simulator.simulate_with_ledger_state(&tx_bytes, &ledger_state)?;

    println!("  Result: {:?}", result);
    assert!(result.success, "always_true validator should succeed");
    println!("  ✅ PASS - always_true validator succeeded!\n");

    Ok(())
}

#[test]
fn test_check_42_with_correct_redeemer() -> Result<()> {
    println!("\n🧪 Testing check_42 validator with redeemer=42...");

    // Load ledger state
    let ledger_state = LedgerState::from_file(
        std::path::Path::new("test-ledger-state.json")
    )?;

    // Create redeemer with value 42
    use pallas_codec::minicbor::Encoder;
    let mut redeemer_data = Vec::new();
    Encoder::new(&mut redeemer_data).u32(42)?;

    // Build transaction
    let tx_bytes = build_script_spend_tx(
        CHECK_42_UTXO,
        CHECK_42_CBOR,
        redeemer_data,
        WALLET_COLLATERAL_UTXO,
    )?;

    // Simulate
    let simulator = TransactionSimulator::new_offline();
    let result = simulator.simulate_with_ledger_state(&tx_bytes, &ledger_state)?;

    println!("  Result: {:?}", result);
    assert!(result.success, "check_42 validator should succeed with redeemer=42");
    println!("  ✅ PASS - check_42 validator succeeded with redeemer=42!\n");

    Ok(())
}

#[test]
fn test_check_42_with_wrong_redeemer() -> Result<()> {
    println!("\n🧪 Testing check_42 validator with redeemer=41 (should fail)...");

    // Load ledger state
    let ledger_state = LedgerState::from_file(
        std::path::Path::new("test-ledger-state.json")
    )?;

    // Create redeemer with value 41 (wrong!)
    use pallas_codec::minicbor::Encoder;
    let mut redeemer_data = Vec::new();
    Encoder::new(&mut redeemer_data).u32(41)?;

    // Build transaction
    let tx_bytes = build_script_spend_tx(
        CHECK_42_UTXO,
        CHECK_42_CBOR,
        redeemer_data,
        WALLET_COLLATERAL_UTXO,
    )?;

    // Simulate
    let simulator = TransactionSimulator::new_offline();
    let result = simulator.simulate_with_ledger_state(&tx_bytes, &ledger_state)?;

    println!("  Result: {:?}", result);
    assert!(!result.success, "check_42 validator should fail with redeemer=41");
    println!("  ✅ PASS - check_42 validator correctly rejected redeemer=41!\n");

    Ok(())
}

#[test]
fn test_match_datum_with_matching_values() -> Result<()> {
    println!("\n🧪 Testing match_datum validator with matching values (100)...");

    // Load ledger state
    let ledger_state = LedgerState::from_file(
        std::path::Path::new("test-ledger-state.json")
    )?;

    // Create redeemer with value 100 (matches datum)
    use pallas_codec::minicbor::Encoder;
    let mut redeemer_data = Vec::new();
    Encoder::new(&mut redeemer_data).u32(100)?;

    // Build transaction
    let tx_bytes = build_script_spend_tx(
        MATCH_DATUM_UTXO,
        MATCH_DATUM_CBOR,
        redeemer_data,
        WALLET_COLLATERAL_UTXO,
    )?;

    // Simulate
    let simulator = TransactionSimulator::new_offline();
    let result = simulator.simulate_with_ledger_state(&tx_bytes, &ledger_state)?;

    println!("  Result: {:?}", result);
    assert!(result.success, "match_datum validator should succeed when datum==redeemer");
    println!("  ✅ PASS - match_datum validator succeeded with matching values!\n");

    Ok(())
}

#[test]
fn test_match_datum_with_mismatched_values() -> Result<()> {
    println!("\n🧪 Testing match_datum validator with mismatched values (99 != 100)...");

    // Load ledger state
    let ledger_state = LedgerState::from_file(
        std::path::Path::new("test-ledger-state.json")
    )?;

    // Create redeemer with value 99 (doesn't match datum of 100)
    use pallas_codec::minicbor::Encoder;
    let mut redeemer_data = Vec::new();
    Encoder::new(&mut redeemer_data).u32(99)?;

    // Build transaction
    let tx_bytes = build_script_spend_tx(
        MATCH_DATUM_UTXO,
        MATCH_DATUM_CBOR,
        redeemer_data,
        WALLET_COLLATERAL_UTXO,
    )?;

    // Simulate
    let simulator = TransactionSimulator::new_offline();
    let result = simulator.simulate_with_ledger_state(&tx_bytes, &ledger_state)?;

    println!("  Result: {:?}", result);
    assert!(!result.success, "match_datum validator should fail when datum!=redeemer");
    println!("  ✅ PASS - match_datum validator correctly rejected mismatched values!\n");

    Ok(())
}

// Uncomment and fill in with a real transaction from preview/preprod to test
// To get a transaction:
// 1. Find a tx with V3 script on preview/preprod using cardano-cli or blockfrost
// 2. Get the tx CBOR: cardano-cli transaction view --tx-file tx.signed --output-json | jq -r '.cborHex'
// 3. Paste the hex string below
