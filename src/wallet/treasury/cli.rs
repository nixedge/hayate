// CLI handlers for SundaeSwap treasury contract interactions

use crate::wallet::treasury::{
    config::{parse_milestone, TreasuryConfig, UtxoRef},
    types::{
        CardanoValue, PayoutStatus, Payout, SundaeMultisigScript,
        TreasuryRedeemer, VendorDatum, VendorRedeemer,
    },
};
use crate::wallet::{
    WalletStorage,
    plutus::{PlutusScript, PlutusVersion, Redeemer, RedeemerTag},
    plutus::redeemer::ExUnits,
    tx_builder::PlutusTransactionBuilder,
    tx_builder::types::PlutusOutput,
    multisig::vkey_hash,
    multisig::derive_multisig_payment_key,
    derivation::derive_account,
};
use ed25519_bip32::DerivationScheme;
use anyhow::{bail, Context, Result};
use pallas_addresses::Address as PallasAddress;

// PlutusTransactionBuilder takes crate::wallet::plutus::address::Network
use crate::wallet::plutus::Network;

// ── Shared helpers ────────────────────────────────────────────────────────────

fn parse_network(s: &str) -> Result<Network> {
    match s.to_lowercase().as_str() {
        "mainnet" => Ok(Network::Mainnet),
        "testnet" | "preprod" | "preview" | "sanchonet" => Ok(Network::Testnet),
        _ => bail!(
            "Invalid network '{}': expected mainnet, testnet, preprod, preview, or sanchonet",
            s
        ),
    }
}

/// Parse a bech32 address into raw bytes
fn parse_address_bytes(addr_str: &str) -> Result<Vec<u8>> {
    let addr = PallasAddress::from_bech32(addr_str)
        .with_context(|| format!("Invalid bech32 address: '{}'", addr_str))?;
    Ok(addr.to_vec())
}

/// CIP-1854 key hash — used for multisig committee keys (block-payment)
fn multisig_key_hash(
    storage: &WalletStorage,
    wallet: &str,
    account: u32,
    key_index: u32,
) -> Result<[u8; 28]> {
    let root = storage
        .derive_root(wallet)
        .with_context(|| format!("Failed to derive root key from wallet '{}'", wallet))?;
    let key = derive_multisig_payment_key(&root, account, key_index);
    Ok(vkey_hash(&key.public().public_key()))
}

/// CIP-1852 key hash — used for vendor/withdrawal keys (standard payment path)
fn payment_key_hash(
    storage: &WalletStorage,
    wallet: &str,
    account: u32,
    key_index: u32,
) -> Result<[u8; 28]> {
    let root = storage
        .derive_root(wallet)
        .with_context(|| format!("Failed to derive root key from wallet '{}'", wallet))?;
    let acct = derive_account(&root, account)
        .with_context(|| format!("Failed to derive account {} from wallet '{}'", account, wallet))?;
    let payment_chain = acct.account_key.derive(DerivationScheme::V2, 0);
    let payment_key = payment_chain.derive(DerivationScheme::V2, key_index);
    Ok(vkey_hash(&payment_key.public().public_key()))
}

/// Load the vendor script from config
fn load_vendor_script(config: &TreasuryConfig) -> Result<PlutusScript> {
    let cbor = config
        .vendor_script_bytes()
        .context("vendor_script_cbor is required in the treasury config")?;
    PlutusScript::from_cbor(cbor, PlutusVersion::V3)
        .context("Failed to parse vendor script CBOR")
}

/// Load the treasury script from config
fn load_treasury_script(config: &TreasuryConfig) -> Result<PlutusScript> {
    let cbor = config
        .treasury_script_bytes()
        .context("treasury_script_cbor is required in the treasury config")?;
    PlutusScript::from_cbor(cbor, PlutusVersion::V3)
        .context("Failed to parse treasury script CBOR")
}

/// Save the built transaction to a file in cardano-cli JSON format
fn save_tx(tx_bytes: &[u8], tx_hash: &[u8], out_file: &str) -> Result<()> {
    let json = serde_json::json!({
        "type": "Tx ConwayEra",
        "description": "SundaeSwap Treasury Transaction",
        "cborHex": hex::encode(tx_bytes),
        "txHash": hex::encode(tx_hash)
    });
    std::fs::write(out_file, serde_json::to_string_pretty(&json)?)
        .with_context(|| format!("Failed to write transaction to '{}'", out_file))?;
    println!("Transaction saved to: {}", out_file);
    println!("TX hash: {}", hex::encode(tx_hash));
    Ok(())
}

// ── fund ──────────────────────────────────────────────────────────────────────

/// Build a Fund transaction: move ADA from the treasury script to a new vendor UTxO.
#[allow(clippy::too_many_arguments)]
pub fn handle_fund(
    storage: &WalletStorage,
    wallet: &str,
    account: u32,
    key_index: u32,
    config_file: &str,
    treasury_utxo: &str,
    vendor_key_hash_hex: &str,
    milestones: &[String],
    fee: u64,
    collateral: &str,
    out_file: &str,
    network_str: &str,
    ttl: Option<u64>,
) -> Result<()> {
    let network = parse_network(network_str)?;
    let config = TreasuryConfig::load(config_file)?;

    let treasury_addr_bytes = parse_address_bytes(&config.treasury_address)?;
    let vendor_addr_bytes = parse_address_bytes(&config.vendor_address)?;
    let treasury_utxo_ref = UtxoRef::parse(treasury_utxo)?;
    let collateral_ref = UtxoRef::parse(collateral)?;

    if milestones.is_empty() {
        bail!("At least one --milestone is required");
    }
    let parsed: Vec<(i64, i64)> = milestones
        .iter()
        .map(|m| parse_milestone(m))
        .collect::<Result<_>>()?;

    for (ts_ms, _) in &parsed {
        if *ts_ms > config.payout_upperbound {
            bail!(
                "Milestone date {} ms exceeds payout_upperbound {} ms",
                ts_ms,
                config.payout_upperbound
            );
        }
    }

    let vendor_key_bytes = hex::decode(vendor_key_hash_hex.trim())
        .context("Invalid vendor key hash hex")?;
    if vendor_key_bytes.len() != 28 {
        bail!("Vendor key hash must be 28 bytes (56 hex chars)");
    }
    let vendor_script = SundaeMultisigScript::Signature {
        key_hash: vendor_key_bytes.clone(),
    };
    let payouts: Vec<Payout> = parsed
        .iter()
        .map(|(ts_ms, lovelace)| Payout::active(*ts_ms, *lovelace))
        .collect();
    let total_lovelace: i64 = payouts.iter().map(|p| p.value.lovelace).sum();

    let vendor_datum = VendorDatum::new(vendor_script, payouts);
    let vendor_datum_cbor = vendor_datum.to_cbor();

    let fund_redeemer = TreasuryRedeemer::Fund {
        amount: CardanoValue::lovelace(total_lovelace),
    };
    let redeemer_cbor = fund_redeemer.to_cbor();

    let treasury_script = load_treasury_script(&config)?;

    let treasury_input_lovelace = treasury_utxo_ref.lovelace;
    if treasury_input_lovelace < (total_lovelace as u64 + fee) {
        bail!(
            "Treasury UTxO has {} lovelace but need {} (amount) + {} (fee)",
            treasury_input_lovelace,
            total_lovelace,
            fee
        );
    }
    let treasury_change = treasury_input_lovelace - total_lovelace as u64 - fee;

    let committee_key_hash = multisig_key_hash(storage, wallet, account, key_index)?;

    let treasury_input_utxo = treasury_utxo_ref.to_utxo_data(&treasury_addr_bytes);
    let collateral_utxo = collateral_ref.to_utxo_data(&treasury_addr_bytes);

    let redeemer = Redeemer::new(
        RedeemerTag::Spend,
        0,
        redeemer_cbor,
        ExUnits::governance_default(),
    );

    let mut builder =
        PlutusTransactionBuilder::new(network, treasury_addr_bytes.clone());

    builder.add_script_input(&treasury_input_utxo, treasury_script, redeemer, None)?;

    let vendor_output = PlutusOutput::with_inline_datum(
        vendor_addr_bytes.clone(),
        total_lovelace as u64,
        vendor_datum_cbor,
    );
    builder.add_output(&vendor_output)?;

    if treasury_change > 0 {
        let change_output = PlutusOutput::new(treasury_addr_bytes.clone(), treasury_change);
        builder.add_output(&change_output)?;
    }

    builder.add_collateral(&collateral_utxo)?;

    builder.add_required_signer(committee_key_hash)?;
    let mut vendor_hash_arr = [0u8; 28];
    vendor_hash_arr.copy_from_slice(&vendor_key_bytes);
    builder.add_required_signer(vendor_hash_arr)?;

    builder
        .set_fee(fee)
        .set_default_language_view(PlutusVersion::V3)
        .set_network_id();

    if let Some(ttl_slot) = ttl {
        builder.set_ttl(ttl_slot);
    }

    let (tx_bytes, tx_hash) = builder.build()?;

    println!("Fund transaction built successfully.");
    println!("  Treasury input:  {} lovelace", treasury_input_lovelace);
    println!("  Vendor output:   {} lovelace", total_lovelace);
    println!("  Treasury change: {} lovelace", treasury_change);
    println!("  Fee:             {} lovelace", fee);
    println!("  Milestones:      {}", milestones.len());

    save_tx(&tx_bytes, &tx_hash, out_file)
}

// ── block-payment (Adjudicate) ────────────────────────────────────────────────

/// Build an Adjudicate transaction: pause or resume milestone payments.
#[allow(clippy::too_many_arguments)]
pub fn handle_block_payment(
    storage: &WalletStorage,
    wallet: &str,
    account: u32,
    key_index: u32,
    config_file: &str,
    vendor_utxo: &str,
    current_datum_hex: &str,
    pause_indices: &[u32],
    resume_indices: &[u32],
    fee: u64,
    collateral: &str,
    out_file: &str,
    network_str: &str,
    ttl: Option<u64>,
    validity_start: Option<u64>,
) -> Result<()> {
    let network = parse_network(network_str)?;
    let config = TreasuryConfig::load(config_file)?;

    let vendor_addr_bytes = parse_address_bytes(&config.vendor_address)?;
    let vendor_utxo_ref = UtxoRef::parse(vendor_utxo)?;
    let collateral_ref = UtxoRef::parse(collateral)?;

    let datum_bytes = hex::decode(current_datum_hex.trim())
        .context("Invalid current datum hex")?;
    let current_datum =
        decode_vendor_datum(&datum_bytes).context("Failed to decode current VendorDatum")?;

    let n = current_datum.payouts.len();
    let mut new_statuses: Vec<PayoutStatus> = current_datum
        .payouts
        .iter()
        .map(|p| p.status.clone())
        .collect();

    for &idx in pause_indices {
        if idx as usize >= n {
            bail!("Pause index {} out of range (have {} payouts)", idx, n);
        }
        new_statuses[idx as usize] = PayoutStatus::Paused;
    }
    for &idx in resume_indices {
        if idx as usize >= n {
            bail!("Resume index {} out of range (have {} payouts)", idx, n);
        }
        new_statuses[idx as usize] = PayoutStatus::Active;
    }

    let updated_datum = current_datum.with_updated_statuses(&new_statuses)?;
    let updated_datum_cbor = updated_datum.to_cbor();

    let adjudicate_redeemer = VendorRedeemer::Adjudicate {
        statuses: new_statuses,
    };
    let redeemer_cbor = adjudicate_redeemer.to_cbor();

    let vendor_script = load_vendor_script(&config)?;

    let vendor_input_lovelace = vendor_utxo_ref.lovelace;
    let vendor_output_lovelace = vendor_input_lovelace.saturating_sub(fee);

    let committee_key_hash = multisig_key_hash(storage, wallet, account, key_index)?;

    let mut vendor_input_utxo = vendor_utxo_ref.to_utxo_data(&vendor_addr_bytes);
    vendor_input_utxo.datum = Some(datum_bytes);
    let collateral_utxo = collateral_ref.to_utxo_data(&vendor_addr_bytes);

    let redeemer = Redeemer::new(
        RedeemerTag::Spend,
        0,
        redeemer_cbor,
        ExUnits::governance_default(),
    );

    let mut builder = PlutusTransactionBuilder::new(network, vendor_addr_bytes.clone());

    builder.add_script_input(&vendor_input_utxo, vendor_script, redeemer, None)?;

    let output = PlutusOutput::with_inline_datum(
        vendor_addr_bytes.clone(),
        vendor_output_lovelace,
        updated_datum_cbor,
    );
    builder.add_output(&output)?;

    builder.add_collateral(&collateral_utxo)?;
    builder.add_required_signer(committee_key_hash)?;
    builder
        .set_fee(fee)
        .set_default_language_view(PlutusVersion::V3)
        .set_network_id();

    if let Some(ttl_slot) = ttl {
        builder.set_ttl(ttl_slot);
    }
    if let Some(start) = validity_start {
        builder.set_validity_start(start);
        if ttl.is_none() {
            builder.set_ttl(start + 129_600); // 36-hour window (1s slots)
        }
    }

    let (tx_bytes, tx_hash) = builder.build()?;

    println!("Adjudicate transaction built successfully.");
    println!("  Vendor UTxO:  {} lovelace", vendor_input_lovelace);
    println!("  Paused:       {} payouts", pause_indices.len());
    println!("  Resumed:      {} payouts", resume_indices.len());
    println!("  Fee:          {} lovelace", fee);

    save_tx(&tx_bytes, &tx_hash, out_file)
}

// ── withdraw ──────────────────────────────────────────────────────────────────

/// Build a Withdraw transaction: claim matured payouts from a vendor UTxO.
#[allow(clippy::too_many_arguments)]
pub fn handle_withdraw(
    storage: &WalletStorage,
    wallet: &str,
    account: u32,
    key_index: u32,
    config_file: &str,
    vendor_utxo: &str,
    current_datum_hex: &str,
    recipient_address: &str,
    current_time_ms: i64,
    fee: u64,
    collateral: &str,
    out_file: &str,
    network_str: &str,
    ttl: Option<u64>,
    validity_start: Option<u64>,
) -> Result<()> {
    let network = parse_network(network_str)?;
    let config = TreasuryConfig::load(config_file)?;

    let vendor_addr_bytes = parse_address_bytes(&config.vendor_address)?;
    let recipient_addr_bytes = parse_address_bytes(recipient_address)?;
    let vendor_utxo_ref = UtxoRef::parse(vendor_utxo)?;
    let collateral_ref = UtxoRef::parse(collateral)?;

    let datum_bytes = hex::decode(current_datum_hex.trim())
        .context("Invalid current datum hex")?;
    let current_datum =
        decode_vendor_datum(&datum_bytes).context("Failed to decode current VendorDatum")?;

    let matured_lovelace = current_datum.matured_lovelace(current_time_ms);
    if matured_lovelace == 0 {
        bail!(
            "No matured payouts found at time {} ms. \
             Ensure milestones have matured and are Active.",
            current_time_ms
        );
    }

    let remaining_datum = current_datum.remaining_after_withdrawal(current_time_ms);
    let remaining_lovelace: i64 = remaining_datum.payouts.iter().map(|p| p.value.lovelace).sum();

    let redeemer_cbor = VendorRedeemer::Withdraw.to_cbor();
    let vendor_script = load_vendor_script(&config)?;
    let vendor_key_hash = payment_key_hash(storage, wallet, account, key_index)?;

    let mut vendor_input_utxo = vendor_utxo_ref.to_utxo_data(&vendor_addr_bytes);
    vendor_input_utxo.datum = Some(datum_bytes);
    let collateral_utxo = collateral_ref.to_utxo_data(&vendor_addr_bytes);

    let redeemer = Redeemer::new(
        RedeemerTag::Spend,
        0,
        redeemer_cbor,
        ExUnits::governance_default(),
    );

    let mut builder = PlutusTransactionBuilder::new(network, recipient_addr_bytes.clone());

    builder.add_script_input(&vendor_input_utxo, vendor_script, redeemer, None)?;

    let payout_net = (matured_lovelace as u64).saturating_sub(fee);
    let recipient_output = PlutusOutput::new(recipient_addr_bytes.clone(), payout_net);
    builder.add_output(&recipient_output)?;

    if !remaining_datum.payouts.is_empty() && remaining_lovelace > 0 {
        let remaining_output = PlutusOutput::with_inline_datum(
            vendor_addr_bytes.clone(),
            remaining_lovelace as u64,
            remaining_datum.to_cbor(),
        );
        builder.add_output(&remaining_output)?;
    }

    builder.add_collateral(&collateral_utxo)?;
    builder.add_required_signer(vendor_key_hash)?;
    builder
        .set_fee(fee)
        .set_default_language_view(PlutusVersion::V3)
        .set_network_id();

    if let Some(ttl_slot) = ttl {
        builder.set_ttl(ttl_slot);
    }
    if let Some(start) = validity_start {
        builder.set_validity_start(start);
    }

    let (tx_bytes, tx_hash) = builder.build()?;

    println!("Withdraw transaction built successfully.");
    println!("  Vendor input:   {} lovelace", vendor_utxo_ref.lovelace);
    println!("  Matured payout: {} lovelace", matured_lovelace);
    println!(
        "  Remaining:      {} lovelace ({} payouts)",
        remaining_lovelace,
        remaining_datum.payouts.len()
    );
    println!("  Fee:            {} lovelace", fee);

    save_tx(&tx_bytes, &tx_hash, out_file)
}

// ── CBOR decoding of VendorDatum ──────────────────────────────────────────────

fn decode_vendor_datum(cbor: &[u8]) -> Result<VendorDatum> {
    use pallas_codec::minicbor::Decoder;

    let mut dec = Decoder::new(cbor);

    let tag = dec
        .tag()
        .map_err(|e| anyhow::anyhow!("Expected Constr tag: {}", e))?;
    if tag.as_u64() != 121 {
        bail!(
            "Expected Constr 0 (tag 121) for VendorDatum, got tag {}",
            tag.as_u64()
        );
    }
    let field_count = dec
        .array()
        .map_err(|e| anyhow::anyhow!("Expected field array: {}", e))?
        .ok_or_else(|| anyhow::anyhow!("Indefinite array not supported"))?;
    if field_count != 2 {
        bail!("VendorDatum expects 2 fields, got {}", field_count);
    }

    let vendor = decode_multisig_script(&mut dec)?;

    let payout_count = dec
        .array()
        .map_err(|e| anyhow::anyhow!("Expected payouts array: {}", e))?
        .ok_or_else(|| anyhow::anyhow!("Indefinite payout array not supported"))?;
    let mut payouts = Vec::new();
    for _ in 0..payout_count {
        payouts.push(decode_payout(&mut dec)?);
    }

    Ok(VendorDatum::new(vendor, payouts))
}

fn decode_multisig_script(
    dec: &mut pallas_codec::minicbor::Decoder,
) -> Result<SundaeMultisigScript> {
    let tag = dec
        .tag()
        .map_err(|e| anyhow::anyhow!("Expected Constr tag for MultisigScript: {}", e))?;
    let constr_idx = tag.as_u64().saturating_sub(121);
    let field_count = dec
        .array()
        .map_err(|e| anyhow::anyhow!("Expected field array: {}", e))?
        .ok_or_else(|| anyhow::anyhow!("Indefinite array not supported"))?;

    match constr_idx {
        0 => {
            if field_count != 1 {
                bail!("Signature expects 1 field, got {}", field_count);
            }
            let key_hash = dec
                .bytes()
                .map_err(|e| anyhow::anyhow!("Expected key_hash bytes: {}", e))?
                .to_vec();
            Ok(SundaeMultisigScript::Signature { key_hash })
        }
        1 | 2 => {
            if field_count != 1 {
                bail!("AllOf/AnyOf expects 1 field, got {}", field_count);
            }
            let script_count = dec
                .array()
                .map_err(|e| anyhow::anyhow!("Expected scripts array: {}", e))?
                .ok_or_else(|| anyhow::anyhow!("Indefinite scripts array not supported"))?;
            let mut scripts = Vec::new();
            for _ in 0..script_count {
                scripts.push(decode_multisig_script(dec)?);
            }
            if constr_idx == 1 {
                Ok(SundaeMultisigScript::AllOf { scripts })
            } else {
                Ok(SundaeMultisigScript::AnyOf { scripts })
            }
        }
        3 => {
            if field_count != 2 {
                bail!("AtLeast expects 2 fields, got {}", field_count);
            }
            let required = dec
                .i64()
                .map_err(|e| anyhow::anyhow!("Expected required int: {}", e))?;
            let script_count = dec
                .array()
                .map_err(|e| anyhow::anyhow!("Expected scripts array: {}", e))?
                .ok_or_else(|| anyhow::anyhow!("Indefinite array not supported"))?;
            let mut scripts = Vec::new();
            for _ in 0..script_count {
                scripts.push(decode_multisig_script(dec)?);
            }
            Ok(SundaeMultisigScript::AtLeast { required, scripts })
        }
        4 => {
            if field_count != 1 {
                bail!("Before expects 1 field");
            }
            let time = dec
                .i64()
                .map_err(|e| anyhow::anyhow!("Expected time: {}", e))?;
            Ok(SundaeMultisigScript::Before { time })
        }
        5 => {
            if field_count != 1 {
                bail!("After expects 1 field");
            }
            let time = dec
                .i64()
                .map_err(|e| anyhow::anyhow!("Expected time: {}", e))?;
            Ok(SundaeMultisigScript::After { time })
        }
        6 => {
            if field_count != 1 {
                bail!("Script expects 1 field");
            }
            let script_hash = dec
                .bytes()
                .map_err(|e| anyhow::anyhow!("Expected script_hash bytes: {}", e))?
                .to_vec();
            Ok(SundaeMultisigScript::Script { script_hash })
        }
        _ => bail!("Unknown MultisigScript constructor index {}", constr_idx),
    }
}

fn decode_payout(dec: &mut pallas_codec::minicbor::Decoder) -> Result<Payout> {
    let tag = dec
        .tag()
        .map_err(|e| anyhow::anyhow!("Expected Constr tag for Payout: {}", e))?;
    if tag.as_u64() != 121 {
        bail!(
            "Expected Constr 0 (tag 121) for Payout, got {}",
            tag.as_u64()
        );
    }
    let field_count = dec
        .array()
        .map_err(|e| anyhow::anyhow!("Expected Payout fields array: {}", e))?
        .ok_or_else(|| anyhow::anyhow!("Indefinite array not supported"))?;
    if field_count != 3 {
        bail!("Payout expects 3 fields, got {}", field_count);
    }

    let maturation = dec
        .i64()
        .map_err(|e| anyhow::anyhow!("Expected maturation: {}", e))?;
    let value = decode_value(dec)?;
    let status = decode_payout_status(dec)?;

    Ok(Payout { maturation, value, status })
}

fn decode_value(dec: &mut pallas_codec::minicbor::Decoder) -> Result<CardanoValue> {
    let outer_count = dec
        .map()
        .map_err(|e| anyhow::anyhow!("Expected Value map: {}", e))?
        .ok_or_else(|| anyhow::anyhow!("Indefinite value map not supported"))?;

    let mut total_lovelace = 0i64;
    for _ in 0..outer_count {
        let _policy_id = dec
            .bytes()
            .map_err(|e| anyhow::anyhow!("Expected policy_id: {}", e))?
            .to_vec();
        let inner_count = dec
            .map()
            .map_err(|e| anyhow::anyhow!("Expected inner asset map: {}", e))?
            .ok_or_else(|| anyhow::anyhow!("Indefinite inner map not supported"))?;
        for _ in 0..inner_count {
            let _asset_name = dec
                .bytes()
                .map_err(|e| anyhow::anyhow!("Expected asset_name: {}", e))?
                .to_vec();
            let amount = dec
                .i64()
                .map_err(|e| anyhow::anyhow!("Expected asset amount: {}", e))?;
            total_lovelace += amount;
        }
    }
    Ok(CardanoValue::lovelace(total_lovelace))
}

fn decode_payout_status(dec: &mut pallas_codec::minicbor::Decoder) -> Result<PayoutStatus> {
    let tag = dec
        .tag()
        .map_err(|e| anyhow::anyhow!("Expected PayoutStatus tag: {}", e))?;
    let constr_idx = tag.as_u64().saturating_sub(121);
    let _field_count = dec
        .array()
        .map_err(|e| anyhow::anyhow!("Expected fields array: {}", e))?;
    match constr_idx {
        0 => Ok(PayoutStatus::Active),
        1 => Ok(PayoutStatus::Paused),
        _ => bail!("Unknown PayoutStatus constructor {}", constr_idx),
    }
}
