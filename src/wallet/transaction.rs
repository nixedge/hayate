// Transaction building and signing for Cardano

use crate::wallet::derivation::Account;
use crate::wallet::utxorpc_client::{UtxoData, WalletUtxorpcClient};
use pallas_addresses::Address;
use thiserror::Error;

#[derive(Error, Debug)]
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
}

pub type TransactionResult<T> = Result<T, TransactionError>;

/// Simple transaction builder for basic ADA transfers
pub struct SimpleTransactionBuilder {
    utxos: Vec<UtxoData>,
    outputs: Vec<TxOutput>,
    fee: u64,
    ttl: Option<u64>,
    change_address: Vec<u8>,
}

#[derive(Clone)]
pub struct TxOutput {
    pub address: Vec<u8>,
    pub amount: u64,
}

impl SimpleTransactionBuilder {
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
        // Only add UTxOs without native assets for simplicity
        for utxo in utxos {
            if !utxo.has_assets() {
                self.utxos.push(utxo);
            }
        }
    }

    /// Add an output
    pub fn add_output(&mut self, address: Vec<u8>, amount: u64) {
        self.outputs.push(TxOutput { address, amount });
    }

    /// Set transaction fee
    pub fn set_fee(&mut self, fee: u64) {
        self.fee = fee;
    }

    /// Set TTL
    pub fn set_ttl(&mut self, ttl: u64) {
        self.ttl = Some(ttl);
    }

    /// Build transaction body as CBOR bytes
    /// Returns (tx_body_cbor, selected_utxos)
    pub fn build(&self) -> TransactionResult<(Vec<u8>, Vec<UtxoData>)> {
        if self.utxos.is_empty() {
            return Err(TransactionError::NoUtxos);
        }

        // Calculate total output amount
        let output_total: u64 = self.outputs.iter().map(|o| o.amount).sum();
        let total_needed = output_total + self.fee;

        // Select UTxOs (simple greedy selection)
        let (selected_utxos, input_total) = self.select_utxos(total_needed)?;

        // Calculate change
        let change = input_total - total_needed;

        // Build transaction using CBOR encoding directly
        // This is a simplified approach - for production use cardano-serialization-lib
        let tx_body_cbor = self.encode_tx_body(&selected_utxos, change)?;

        Ok((tx_body_cbor, selected_utxos))
    }

    fn select_utxos(&self, amount_needed: u64) -> TransactionResult<(Vec<UtxoData>, u64)> {
        let mut selected = Vec::new();
        let mut total = 0u64;

        for utxo in &self.utxos {
            selected.push(utxo.clone());
            total += utxo.lovelace();

            if total >= amount_needed {
                return Ok((selected, total));
            }
        }

        Err(TransactionError::InsufficientFunds {
            need: amount_needed,
            have: total,
        })
    }

    fn encode_tx_body(&self, selected_utxos: &[UtxoData], change: u64) -> TransactionResult<Vec<u8>> {
        // For now, return a placeholder
        // Full implementation would use pallas or cardano-serialization-lib properly
        // This requires careful handling of CBOR encoding

        // Build a minimal transaction body structure
        use std::io::Write;

        let mut body = Vec::new();

        // This is a simplified stub - proper implementation needs:
        // 1. Correct CBOR map encoding with proper keys
        // 2. Inputs as set of transaction inputs
        // 3. Outputs as array of transaction outputs
        // 4. Fee as coin
        // 5. TTL if present

        // For now, serialize as JSON for debugging
        let tx_struct = serde_json::json!({
            "inputs": selected_utxos.iter().map(|u| u.format_ref()).collect::<Vec<_>>(),
            "outputs": self.outputs.iter().map(|o| {
                serde_json::json!({
                    "address": hex::encode(&o.address),
                    "amount": o.amount
                })
            }).collect::<Vec<_>>(),
            "fee": self.fee,
            "ttl": self.ttl,
            "change": change,
            "change_address": hex::encode(&self.change_address),
        });

        write!(&mut body, "{}", tx_struct.to_string())
            .map_err(|e| TransactionError::CborEncode(e.to_string()))?;

        Ok(body)
    }
}

/// Sign a transaction body (placeholder for now)
pub fn sign_transaction(
    _tx_body_cbor: &[u8],
    _account: &Account,
    _stake: bool,
) -> TransactionResult<Vec<u8>> {
    // This would create proper witness set and signed transaction
    // For now, return error indicating not fully implemented
    Err(TransactionError::CborEncode(
        "Transaction signing requires proper CBOR encoding - use cardano-cli for now".to_string()
    ))
}

/// Helper to parse Bech32 address to bytes
pub fn parse_address(addr_str: &str) -> TransactionResult<Vec<u8>> {
    Address::from_bech32(addr_str)
        .map(|addr| addr.to_vec())
        .map_err(|e| TransactionError::InvalidAddress(e.to_string()))
}

/// Build a simple send transaction
pub async fn build_send_transaction(
    client: &mut WalletUtxorpcClient,
    from_addresses: Vec<Vec<u8>>,
    to_address: &str,
    amount: u64,
    fee: u64,
    change_address: Vec<u8>,
    ttl: Option<u64>,
) -> TransactionResult<(Vec<u8>, Vec<UtxoData>)> {
    // Query UTxOs
    let utxos = client.query_utxos(from_addresses)
        .await
        .map_err(|e| TransactionError::UtxorpcError(e.to_string()))?;

    // Parse destination address
    let to_addr_bytes = parse_address(to_address)?;

    // Build transaction
    let mut builder = SimpleTransactionBuilder::new(change_address);
    builder.add_utxos(utxos);
    builder.add_output(to_addr_bytes, amount);
    builder.set_fee(fee);

    if let Some(ttl_val) = ttl {
        builder.set_ttl(ttl_val);
    }

    builder.build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_builder_basics() {
        let change_addr = vec![0u8; 57]; // Dummy address
        let mut builder = SimpleTransactionBuilder::new(change_addr.clone());

        // Add a dummy UTxO
        let utxo = UtxoData {
            tx_hash: vec![0u8; 32],
            output_index: 0,
            address: change_addr.clone(),
            coin: 10_000_000,
            assets: Vec::new(),
            datum: None,
            script_ref: None,
        };

        builder.add_utxos(vec![utxo]);
        builder.add_output(change_addr, 5_000_000);
        builder.set_fee(200_000);

        let result = builder.build();
        assert!(result.is_ok());

        let (_tx_body, selected) = result.unwrap();
        assert_eq!(selected.len(), 1);
    }

    #[test]
    fn test_insufficient_funds() {
        let change_addr = vec![0u8; 57];
        let mut builder = SimpleTransactionBuilder::new(change_addr.clone());

        let utxo = UtxoData {
            tx_hash: vec![0u8; 32],
            output_index: 0,
            address: change_addr.clone(),
            coin: 1_000_000,
            assets: Vec::new(),
            datum: None,
            script_ref: None,
        };

        builder.add_utxos(vec![utxo]);
        builder.add_output(change_addr, 5_000_000);
        builder.set_fee(200_000);

        let result = builder.build();
        assert!(matches!(result, Err(TransactionError::InsufficientFunds { .. })));
    }
}
