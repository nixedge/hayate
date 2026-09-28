// Treasury instance configuration
//
// Loaded from a JSON file that describes a deployed SundaeSwap treasury instance.
// Users obtain this from the treasury admin or the SundaeSwap portal.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Configuration for a deployed SundaeSwap treasury instance.
///
/// This is loaded from a JSON config file. The values here are specific to
/// a particular deployed contract instance (mainnet, preview, etc.).
///
/// Example JSON:
/// ```json
/// {
///   "registry_token": "abcdef...",
///   "treasury_address": "addr1...",
///   "vendor_address": "addr1...",
///   "treasury_script_cbor": "590a...",
///   "vendor_script_cbor": "590b...",
///   "expiration": 1893456000000,
///   "payout_upperbound": 1893456000000
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreasuryConfig {
    /// Registry NFT policy ID (hex, 28 bytes = 56 hex chars)
    pub registry_token: String,

    /// Treasury script address (bech32 addr1... or addr_test1...)
    pub treasury_address: String,

    /// Vendor script address (bech32 addr1... or addr_test1...)
    pub vendor_address: String,

    /// Treasury validator script CBOR (hex, double-CBOR encoded Plutus V3 script).
    /// Optional: required only when the script is not available as a reference input.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub treasury_script_cbor: Option<String>,

    /// Vendor validator script CBOR (hex, double-CBOR encoded Plutus V3 script).
    /// Optional: required only when the script is not available as a reference input.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vendor_script_cbor: Option<String>,

    /// Treasury expiration timestamp (POSIX milliseconds).
    /// After this time, anyone can sweep remaining treasury funds.
    pub expiration: i64,

    /// Maximum allowed maturation date for any milestone (POSIX milliseconds).
    /// No payout milestone can mature after this date.
    pub payout_upperbound: i64,
}

impl TreasuryConfig {
    /// Load config from a JSON file
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read treasury config from {}", path.display()))?;
        let config: Self = serde_json::from_str(&contents)
            .with_context(|| format!("Failed to parse treasury config JSON from {}", path.display()))?;
        config.validate()?;
        Ok(config)
    }

    /// Save config to a JSON file
    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let contents = serde_json::to_string_pretty(self)?;
        std::fs::write(path, contents)?;
        Ok(())
    }

    fn validate(&self) -> Result<()> {
        let token_bytes = hex::decode(&self.registry_token)
            .context("registry_token must be valid hex")?;
        if token_bytes.len() != 28 {
            anyhow::bail!(
                "registry_token must be 28 bytes (56 hex chars), got {}",
                token_bytes.len()
            );
        }

        if self.treasury_address.is_empty() {
            anyhow::bail!("treasury_address cannot be empty");
        }
        if self.vendor_address.is_empty() {
            anyhow::bail!("vendor_address cannot be empty");
        }

        if let Some(ref cbor) = self.treasury_script_cbor {
            hex::decode(cbor).context("treasury_script_cbor must be valid hex")?;
        }
        if let Some(ref cbor) = self.vendor_script_cbor {
            hex::decode(cbor).context("vendor_script_cbor must be valid hex")?;
        }

        Ok(())
    }

    /// Decode registry_token to raw bytes
    pub fn registry_token_bytes(&self) -> [u8; 28] {
        let bytes = hex::decode(&self.registry_token).expect("validated in load");
        let mut arr = [0u8; 28];
        arr.copy_from_slice(&bytes);
        arr
    }

    /// Decode treasury script CBOR to bytes (if present)
    pub fn treasury_script_bytes(&self) -> Option<Vec<u8>> {
        self.treasury_script_cbor.as_ref().map(|h| hex::decode(h).expect("validated in load"))
    }

    /// Decode vendor script CBOR to bytes (if present)
    pub fn vendor_script_bytes(&self) -> Option<Vec<u8>> {
        self.vendor_script_cbor.as_ref().map(|h| hex::decode(h).expect("validated in load"))
    }

    /// Generate a template config for the user to fill in
    pub fn template() -> Self {
        Self {
            registry_token: "abcdef1234567890abcdef1234567890abcdef1234567890abcdef12".to_string(),
            treasury_address: "addr_test1...".to_string(),
            vendor_address: "addr_test1...".to_string(),
            treasury_script_cbor: Some("590a...".to_string()),
            vendor_script_cbor: Some("590b...".to_string()),
            expiration: 1_893_456_000_000,
            payout_upperbound: 1_893_456_000_000,
        }
    }
}

/// A UTxO reference provided by the user on the command line
#[derive(Debug, Clone)]
pub struct UtxoRef {
    pub tx_hash: Vec<u8>,
    pub output_index: u32,
    pub lovelace: u64,
    /// Optional inline datum (CBOR hex)
    pub datum_cbor: Option<Vec<u8>>,
}

impl UtxoRef {
    /// Parse from "txhash#index:lovelace" or "txhash#index:lovelace:datum_hex"
    pub fn parse(s: &str) -> Result<Self> {
        let parts: Vec<&str> = s.splitn(4, ':').collect();
        if parts.len() < 2 {
            anyhow::bail!(
                "Invalid UTxO format '{}'. Expected 'txhash#index:lovelace' or 'txhash#index:lovelace:datum_hex'",
                s
            );
        }

        let hash_parts: Vec<&str> = parts[0].splitn(2, '#').collect();
        if hash_parts.len() != 2 {
            anyhow::bail!("UTxO reference must contain '#' separator: '{}'", parts[0]);
        }

        let tx_hash = hex::decode(hash_parts[0])
            .with_context(|| format!("Invalid tx hash hex: '{}'", hash_parts[0]))?;
        if tx_hash.len() != 32 {
            anyhow::bail!("TX hash must be 32 bytes (64 hex chars)");
        }

        let output_index: u32 = hash_parts[1]
            .parse()
            .with_context(|| format!("Invalid output index: '{}'", hash_parts[1]))?;

        let lovelace: u64 = parts[1]
            .parse()
            .with_context(|| format!("Invalid lovelace amount: '{}'", parts[1]))?;

        let datum_cbor = if parts.len() >= 3 && !parts[2].is_empty() {
            Some(hex::decode(parts[2]).context("Invalid datum CBOR hex")?)
        } else {
            None
        };

        Ok(Self { tx_hash, output_index, lovelace, datum_cbor })
    }

    /// Convert to the UtxoData type used by the tx builder
    pub fn to_utxo_data(&self, address_bytes: &[u8]) -> crate::wallet::utxorpc_client::UtxoData {
        crate::wallet::utxorpc_client::UtxoData {
            tx_hash: self.tx_hash.clone(),
            output_index: self.output_index,
            address: address_bytes.to_vec(),
            coin: self.lovelace,
            assets: Vec::new(),
            datum_hash: None,
            datum: self.datum_cbor.clone(),
        }
    }
}

/// Parse a milestone specification "ISO8601-date:lovelace"
/// e.g. "2025-01-31T00:00:00Z:5000000" or "2025-01-31:5000000" (midnight UTC)
pub fn parse_milestone(s: &str) -> Result<(i64, i64)> {
    let parts: Vec<&str> = s.rsplitn(2, ':').collect();
    if parts.len() != 2 {
        anyhow::bail!("Milestone must be 'date:lovelace', got '{}'", s);
    }
    let lovelace: i64 = parts[0].trim().parse()
        .with_context(|| format!("Invalid lovelace amount: '{}'", parts[0]))?;
    let date_str = parts[1].trim();

    // Parse the date as a timestamp in POSIX milliseconds
    let ts_ms = parse_date_to_posix_ms(date_str)?;
    Ok((ts_ms, lovelace))
}

/// Parse a date string to POSIX milliseconds.
/// Accepts RFC3339/ISO8601 or "YYYY-MM-DD" (interpreted as midnight UTC).
pub fn parse_date_to_posix_ms(s: &str) -> Result<i64> {
    use chrono::{DateTime, NaiveDate, TimeZone, Utc};

    // Try full RFC3339 first
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.timestamp_millis());
    }

    // Try "YYYY-MM-DD" as midnight UTC
    if let Ok(nd) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let dt = Utc.from_utc_datetime(&nd.and_hms_opt(0, 0, 0).unwrap());
        return Ok(dt.timestamp_millis());
    }

    anyhow::bail!("Cannot parse date '{}'. Use ISO8601 (e.g. '2025-01-31T00:00:00Z') or 'YYYY-MM-DD'", s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_utxo_ref_parse_basic() {
        let s = "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890#0:5000000";
        let r = UtxoRef::parse(s).unwrap();
        assert_eq!(r.output_index, 0);
        assert_eq!(r.lovelace, 5_000_000);
        assert!(r.datum_cbor.is_none());
    }

    #[test]
    fn test_utxo_ref_parse_with_datum() {
        let s = "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890#1:10000000:d87980";
        let r = UtxoRef::parse(s).unwrap();
        assert_eq!(r.output_index, 1);
        assert_eq!(r.datum_cbor, Some(vec![0xd8, 0x79, 0x80]));
    }

    #[test]
    fn test_parse_milestone_date_only() {
        let (ts, lovelace) = parse_milestone("2025-01-31:5000000").unwrap();
        assert_eq!(lovelace, 5_000_000);
        assert!(ts > 0);
    }

    #[test]
    fn test_parse_milestone_rfc3339() {
        let (ts, lovelace) = parse_milestone("2025-01-31T00:00:00Z:5000000").unwrap();
        assert_eq!(lovelace, 5_000_000);
        assert!(ts > 0);
    }
}
