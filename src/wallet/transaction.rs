// Transaction building and signing for Cardano

use crate::wallet::derivation::Account;
use crate::wallet::utxorpc_client::{UtxoData, WalletUtxorpcClient, AssetData};
use pallas_codec::minicbor;
use pallas_crypto::hash::Hasher;
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum TransactionError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("CBOR encoding error: {0}")]
    CborEncode(String),

    #[error("CBOR decoding error: {0}")]
    CborDecode(String),

    #[error("Insufficient funds: need {need}, have {have}")]
    InsufficientFunds { need: u64, have: u64 },

    #[error("Invalid address: {0}")]
    InvalidAddress(String),

    #[error("UTxORPC error: {0}")]
    UtxorpcError(String),

    #[error("No UTxOs available")]
    NoUtxos,

    #[error("Invalid asset: {0}")]
    InvalidAsset(String),
}

pub type TransactionResult<T> = Result<T, TransactionError>;

/// Transaction builder for Cardano transactions
pub struct TransactionBuilder {
    utxos: Vec<UtxoData>,
    outputs: Vec<TxOutput>,
    fee: u64,
    ttl: Option<u64>,
    change_address: Vec<u8>,
}

/// Transaction output
#[derive(Clone, Debug)]
pub struct TxOutput {
    pub address: Vec<u8>,
    pub lovelace: u64,
    pub assets: Vec<AssetData>,
}

impl TransactionBuilder {
    /// Create a new transaction builder
    pub fn new(change_address: Vec<u8>) -> Self {
        Self {
            utxos: Vec::new(),
            outputs: Vec::new(),
            fee: 0,
            ttl: None,
            change_address,
        }
    }

    /// Add available UTxOs
    pub fn add_utxos(&mut self, utxos: Vec<UtxoData>) {
        self.utxos.extend(utxos);
    }

    /// Add an output
    pub fn add_output(&mut self, address: Vec<u8>, lovelace: u64, assets: Vec<AssetData>) {
        self.outputs.push(TxOutput {
            address,
            lovelace,
            assets,
        });
    }

    /// Set transaction fee
    pub fn set_fee(&mut self, fee: u64) {
        self.fee = fee;
    }

    /// Set TTL
    pub fn set_ttl(&mut self, ttl: u64) {
        self.ttl = Some(ttl);
    }

    /// Build the transaction
    /// Returns (tx_body_cbor, selected_utxos)
    pub fn build(&self) -> TransactionResult<(Vec<u8>, Vec<UtxoData>)> {
        // Calculate total outputs
        let mut output_lovelace = self.fee;
        let mut output_assets: BTreeMap<String, u64> = BTreeMap::new();

        for output in &self.outputs {
            output_lovelace += output.lovelace;
            for asset in &output.assets {
                let key = format!("{}.{}", hex::encode(&asset.policy_id), hex::encode(&asset.asset_name));
                *output_assets.entry(key).or_insert(0) += asset.amount;
            }
        }

        // Select UTxOs (greedy selection)
        let (selected_utxos, change_lovelace, change_assets) = self.select_utxos(output_lovelace, &output_assets)?;

        // Build transaction body
        let tx_body_cbor = self.encode_transaction_body(&selected_utxos, change_lovelace, &change_assets)?;

        Ok((tx_body_cbor, selected_utxos))
    }

    /// Select UTxOs to cover outputs
    fn select_utxos(
        &self,
        needed_lovelace: u64,
        needed_assets: &BTreeMap<String, u64>,
    ) -> TransactionResult<(Vec<UtxoData>, u64, BTreeMap<String, u64>)> {
        if self.utxos.is_empty() {
            return Err(TransactionError::NoUtxos);
        }

        let mut selected = Vec::new();
        let mut total_lovelace = 0u64;
        let mut total_assets: BTreeMap<String, u64> = BTreeMap::new();

        // Greedy selection
        for utxo in &self.utxos {
            selected.push(utxo.clone());
            total_lovelace += utxo.lovelace();

            // Add assets from this UTxO
            for asset in &utxo.assets {
                let key = format!("{}.{}", hex::encode(&asset.policy_id), hex::encode(&asset.asset_name));
                *total_assets.entry(key).or_insert(0) += asset.amount;
            }

            // Check if we have enough
            let mut have_enough = total_lovelace >= needed_lovelace;
            for (key, needed_amount) in needed_assets {
                if total_assets.get(key).unwrap_or(&0) < needed_amount {
                    have_enough = false;
                    break;
                }
            }

            if have_enough {
                break;
            }
        }

        // Verify we have enough
        if total_lovelace < needed_lovelace {
            return Err(TransactionError::InsufficientFunds {
                need: needed_lovelace,
                have: total_lovelace,
            });
        }

        for (key, needed_amount) in needed_assets {
            if total_assets.get(key).unwrap_or(&0) < needed_amount {
                return Err(TransactionError::InvalidAsset(
                    format!("Insufficient asset {}: need {}, have {}", key, needed_amount, total_assets.get(key).unwrap_or(&0))
                ));
            }
        }

        // Calculate change
        let change_lovelace = total_lovelace - needed_lovelace;

        let mut change_assets = BTreeMap::new();
        for (key, total) in total_assets {
            let needed = needed_assets.get(&key).unwrap_or(&0);
            if total > *needed {
                change_assets.insert(key, total - needed);
            }
        }

        Ok((selected, change_lovelace, change_assets))
    }

    /// Encode transaction body to CBOR
    fn encode_transaction_body(
        &self,
        selected_utxos: &[UtxoData],
        change_lovelace: u64,
        change_assets: &BTreeMap<String, u64>,
    ) -> TransactionResult<Vec<u8>> {
        let mut buffer = Vec::new();
        let mut encoder = minicbor::Encoder::new(&mut buffer);

        // Determine number of fields
        let mut num_fields = 3; // inputs, outputs, fee
        if self.ttl.is_some() {
            num_fields += 1;
        }

        // Transaction body is a map
        encoder.map(num_fields).map_err(|e| TransactionError::CborEncode(e.to_string()))?;

        // Field 0: inputs (set of transaction inputs)
        encoder.u8(0).map_err(|e| TransactionError::CborEncode(e.to_string()))?;
        encoder.array(selected_utxos.len() as u64).map_err(|e| TransactionError::CborEncode(e.to_string()))?;
        for utxo in selected_utxos {
            if utxo.tx_hash.len() != 32 {
                return Err(TransactionError::CborEncode("Invalid tx hash length".to_string()));
            }
            encoder.array(2).map_err(|e| TransactionError::CborEncode(e.to_string()))?; // [tx_hash, output_index]
            encoder.bytes(&utxo.tx_hash).map_err(|e| TransactionError::CborEncode(e.to_string()))?;
            encoder.u64(utxo.output_index as u64).map_err(|e| TransactionError::CborEncode(e.to_string()))?;
        }

        // Field 1: outputs
        encoder.u8(1).map_err(|e| TransactionError::CborEncode(e.to_string()))?;

        // Count outputs (user outputs + change if needed)
        let mut num_outputs = self.outputs.len();
        if change_lovelace > 0 || !change_assets.is_empty() {
            num_outputs += 1;
        }

        encoder.array(num_outputs as u64).map_err(|e| TransactionError::CborEncode(e.to_string()))?;

        // Encode user outputs
        for output in &self.outputs {
            encoder.array(2).map_err(|e| TransactionError::CborEncode(e.to_string()))?; // [address, value]
            encoder.bytes(&output.address).map_err(|e| TransactionError::CborEncode(e.to_string()))?;

            // Encode value (just lovelace for now, assets TODO)
            if output.assets.is_empty() {
                encoder.u64(output.lovelace).map_err(|e| TransactionError::CborEncode(e.to_string()))?;
            } else {
                encoder.array(2).map_err(|e| TransactionError::CborEncode(e.to_string()))?; // [coin, multiasset]
                encoder.u64(output.lovelace).map_err(|e| TransactionError::CborEncode(e.to_string()))?;
                self.encode_multiasset(&mut encoder, &output.assets)?;
            }
        }

        // Encode change output if needed
        if change_lovelace > 0 || !change_assets.is_empty() {
            encoder.array(2).map_err(|e| TransactionError::CborEncode(e.to_string()))?; // [address, value]
            encoder.bytes(&self.change_address).map_err(|e| TransactionError::CborEncode(e.to_string()))?;

            // Convert change assets back to AssetData
            let change_assets_vec: Vec<AssetData> = change_assets.iter().map(|(key, amount)| {
                let parts: Vec<&str> = key.split('.').collect();
                let policy_id = hex::decode(parts[0]).unwrap_or_default();
                let asset_name = if parts.len() > 1 {
                    hex::decode(parts[1]).unwrap_or_default()
                } else {
                    Vec::new()
                };
                AssetData {
                    policy_id,
                    asset_name,
                    amount: *amount,
                }
            }).collect();

            if change_assets_vec.is_empty() {
                encoder.u64(change_lovelace).map_err(|e| TransactionError::CborEncode(e.to_string()))?;
            } else {
                encoder.array(2).map_err(|e| TransactionError::CborEncode(e.to_string()))?; // [coin, multiasset]
                encoder.u64(change_lovelace).map_err(|e| TransactionError::CborEncode(e.to_string()))?;
                self.encode_multiasset(&mut encoder, &change_assets_vec)?;
            }
        }

        // Field 2: fee
        encoder.u8(2).map_err(|e| TransactionError::CborEncode(e.to_string()))?;
        encoder.u64(self.fee).map_err(|e| TransactionError::CborEncode(e.to_string()))?;

        // Field 3: TTL (optional)
        if let Some(ttl) = self.ttl {
            encoder.u8(3).map_err(|e| TransactionError::CborEncode(e.to_string()))?;
            encoder.u64(ttl).map_err(|e| TransactionError::CborEncode(e.to_string()))?;
        }

        Ok(buffer)
    }

    /// Encode multiasset map
    fn encode_multiasset(&self, encoder: &mut minicbor::Encoder<&mut Vec<u8>>, assets: &[AssetData]) -> TransactionResult<()> {
        // Group assets by policy ID
        let mut policy_map: BTreeMap<Vec<u8>, BTreeMap<Vec<u8>, u64>> = BTreeMap::new();

        for asset in assets {
            if asset.policy_id.len() != 28 {
                return Err(TransactionError::InvalidAsset(
                    format!("Invalid policy ID length: {}", asset.policy_id.len())
                ));
            }

            policy_map
                .entry(asset.policy_id.clone())
                .or_default()
                .insert(asset.asset_name.clone(), asset.amount);
        }

        // Encode as map of maps
        encoder.map(policy_map.len() as u64).map_err(|e| TransactionError::CborEncode(e.to_string()))?;

        for (policy_id, assets_map) in policy_map {
            encoder.bytes(&policy_id).map_err(|e| TransactionError::CborEncode(e.to_string()))?;
            encoder.map(assets_map.len() as u64).map_err(|e| TransactionError::CborEncode(e.to_string()))?;

            for (asset_name, amount) in assets_map {
                encoder.bytes(&asset_name).map_err(|e| TransactionError::CborEncode(e.to_string()))?;
                encoder.u64(amount).map_err(|e| TransactionError::CborEncode(e.to_string()))?;
            }
        }

        Ok(())
    }
}

/// Sign a transaction body
pub fn sign_transaction(
    tx_body_cbor: &[u8],
    account: &Account,
    stake: bool,
) -> TransactionResult<Vec<u8>> {
    // Hash the transaction body with Blake2b-256
    let mut hasher = Hasher::<256>::new();
    hasher.input(tx_body_cbor);
    let tx_hash = hasher.finalize();

    // Create witnesses
    let mut witnesses = Vec::new();

    // Payment key witness
    {
        let payment_sig: ed25519_bip32::Signature<Vec<u8>> = account.payment_key.sign(tx_hash.as_ref());
        let vkey = account.payment_key.public();

        witnesses.push((vkey.as_ref().to_vec(), payment_sig.as_ref().to_vec()));
    }

    // Stake key witness (if requested)
    if stake {
        let stake_sig: ed25519_bip32::Signature<Vec<u8>> = account.stake_key.sign(tx_hash.as_ref());
        let vkey = account.stake_key.public();

        witnesses.push((vkey.as_ref().to_vec(), stake_sig.as_ref().to_vec()));
    }

    // Build signed transaction
    let mut buffer = Vec::new();
    let mut encoder = minicbor::Encoder::new(&mut buffer);

    // MintedTx is an array: [transaction_body, transaction_witness_set, valid, auxiliary_data]
    encoder.array(4).map_err(|e| TransactionError::CborEncode(e.to_string()))?;

    // transaction_body (raw CBOR bytes)
    encoder.bytes(tx_body_cbor).map_err(|e| TransactionError::CborEncode(e.to_string()))?;

    // transaction_witness_set (map)
    encoder.map(1).map_err(|e| TransactionError::CborEncode(e.to_string()))?; // Only vkeywitness field

    // Field 0: vkeywitness (array of [vkey, signature])
    encoder.u8(0).map_err(|e| TransactionError::CborEncode(e.to_string()))?;
    encoder.array(witnesses.len() as u64).map_err(|e| TransactionError::CborEncode(e.to_string()))?;

    for (vkey, sig) in witnesses {
        encoder.array(2).map_err(|e| TransactionError::CborEncode(e.to_string()))?;
        encoder.bytes(&vkey).map_err(|e| TransactionError::CborEncode(e.to_string()))?;
        encoder.bytes(&sig).map_err(|e| TransactionError::CborEncode(e.to_string()))?;
    }

    // valid (true for valid transaction)
    encoder.bool(true).map_err(|e| TransactionError::CborEncode(e.to_string()))?;

    // auxiliary_data (null for none)
    encoder.null().map_err(|e| TransactionError::CborEncode(e.to_string()))?;

    Ok(buffer)
}

/// Helper: Build a simple send transaction
#[allow(dead_code)]
pub async fn build_send_transaction(
    client: &mut WalletUtxorpcClient,
    _account: &Account,
    payment_addresses: &[Vec<u8>],
    recipient: Vec<u8>,
    amount: u64,
    fee: u64,
    ttl: Option<u64>,
) -> TransactionResult<(Vec<u8>, Vec<UtxoData>)> {
    // Query UTxOs
    let utxos = client.query_utxos(payment_addresses.to_vec())
        .await
        .map_err(|e| TransactionError::UtxorpcError(e.to_string()))?;

    // Build transaction
    let mut builder = TransactionBuilder::new(payment_addresses[0].clone());
    builder.add_utxos(utxos);
    builder.add_output(recipient, amount, Vec::new());
    builder.set_fee(fee);
    if let Some(ttl_value) = ttl {
        builder.set_ttl(ttl_value);
    }

    builder.build()
}

/// Write transaction body to file
pub fn write_tx_body(tx_body: &[u8], path: &str) -> TransactionResult<()> {
    std::fs::write(path, hex::encode(tx_body))?;
    Ok(())
}

/// Write signed transaction to file
pub fn write_signed_tx(signed_tx: &[u8], path: &str) -> TransactionResult<()> {
    std::fs::write(path, hex::encode(signed_tx))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_builder_simple() {
        // Create mock UTxO
        let utxo = UtxoData {
            tx_hash: vec![0u8; 32],
            output_index: 0,
            address: vec![1u8; 57],
            coin: 10_000_000,
            assets: Vec::new(),
            datum_hash: None,
            datum: None,
        };

        // Build transaction
        let mut builder = TransactionBuilder::new(vec![1u8; 57]);
        builder.add_utxos(vec![utxo]);
        builder.add_output(vec![2u8; 57], 5_000_000, Vec::new());
        builder.set_fee(200_000);

        let result = builder.build();
        assert!(result.is_ok());

        let (tx_body, selected) = result.unwrap();
        assert_eq!(selected.len(), 1);
        assert!(!tx_body.is_empty());
    }

    #[test]
    fn test_transaction_builder_with_assets() {
        let asset = AssetData {
            policy_id: vec![0u8; 28],
            asset_name: b"TestToken".to_vec(),
            amount: 1000,
        };

        let utxo = UtxoData {
            tx_hash: vec![0u8; 32],
            output_index: 0,
            address: vec![1u8; 57],
            coin: 10_000_000,
            assets: vec![asset.clone()],
            datum_hash: None,
            datum: None,
        };

        let mut builder = TransactionBuilder::new(vec![1u8; 57]);
        builder.add_utxos(vec![utxo]);
        builder.add_output(vec![2u8; 57], 5_000_000, vec![asset]);
        builder.set_fee(200_000);

        let result = builder.build();
        assert!(result.is_ok());
    }

    #[test]
    fn test_insufficient_funds() {
        let utxo = UtxoData {
            tx_hash: vec![0u8; 32],
            output_index: 0,
            address: vec![1u8; 57],
            coin: 1_000_000,
            assets: Vec::new(),
            datum_hash: None,
            datum: None,
        };

        let mut builder = TransactionBuilder::new(vec![1u8; 57]);
        builder.add_utxos(vec![utxo]);
        builder.add_output(vec![2u8; 57], 5_000_000, Vec::new());
        builder.set_fee(200_000);

        let result = builder.build();
        assert!(matches!(result, Err(TransactionError::InsufficientFunds { .. })));
    }
}
