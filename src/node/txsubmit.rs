// Transaction submission to Cardano node
// TODO: Implement proper submission using pallas or cardano-cli

use anyhow::{Context, Result};
use std::path::Path;
use tokio::process::Command;

/// Submit a transaction to the Cardano node using cardano-cli
///
/// # Arguments
/// * `socket_path` - Path to node socket
/// * `magic` - Network magic number
/// * `tx_bytes` - CBOR-encoded transaction bytes
///
/// # Returns
/// * `Ok(tx_hash)` - Transaction hash if accepted
/// * `Err(_)` - If transaction was rejected or submission failed
pub async fn submit_tx(socket_path: &str, _magic: u64, tx_bytes: Vec<u8>) -> Result<Vec<u8>> {
    tracing::info!("Submitting transaction via cardano-cli");
    tracing::debug!("Transaction size: {} bytes", tx_bytes.len());
    tracing::debug!("Node socket: {}", socket_path);

    // Calculate transaction hash (blake2b-256 of the tx bytes)
    use pallas_crypto::hash::Hasher;
    let tx_hash = Hasher::<256>::hash(&tx_bytes);
    tracing::info!("Transaction hash: {}", hex::encode(&tx_hash));

    // Write transaction to temporary file
    let tx_file = format!("/tmp/hayate-tx-{}.signed", hex::encode(&tx_hash));
    tokio::fs::write(&tx_file, &tx_bytes).await
        .context("Failed to write transaction file")?;

    // Submit using cardano-cli
    let output = Command::new("cardano-cli")
        .args(&[
            "transaction", "submit",
            "--tx-file", &tx_file,
            "--socket-path", socket_path,
        ])
        .output()
        .await
        .context("Failed to execute cardano-cli")?;

    // Clean up temp file
    let _ = tokio::fs::remove_file(&tx_file).await;

    if output.status.success() {
        tracing::info!("Transaction submitted successfully: {}", hex::encode(&tx_hash));
        Ok(tx_hash.to_vec())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let error_msg = format!("cardano-cli submission failed: {}", stderr);
        tracing::error!("{}", error_msg);
        Err(anyhow::anyhow!(error_msg))
    }
}
