// Plutus data types for SundaeSwap treasury contracts
//
// CBOR encoding follows the Plutus Data spec:
//   Constr n (0-6): CBOR tag (121+n), array of fields
//   Constr n (7+):  CBOR tag 102, [n, [fields]]
//   Map:            CBOR map
//   List:           CBOR array
//   Integer:        CBOR integer
//   Bytes:          CBOR bytes
//
// References:
//   https://github.com/SundaeSwap-finance/treasury-contracts
//   Plutus Core spec section on Data encoding

use anyhow::{bail, Result};

// ── CBOR primitives ──────────────────────────────────────────────────────────

fn cbor_header(major: u8, n: u64) -> Vec<u8> {
    if n <= 23 {
        vec![(major << 5) | n as u8]
    } else if n <= 0xFF {
        vec![(major << 5) | 24, n as u8]
    } else if n <= 0xFFFF {
        vec![(major << 5) | 25, (n >> 8) as u8, (n & 0xFF) as u8]
    } else if n <= 0xFFFF_FFFF {
        vec![
            (major << 5) | 26,
            (n >> 24) as u8,
            ((n >> 16) & 0xFF) as u8,
            ((n >> 8) & 0xFF) as u8,
            (n & 0xFF) as u8,
        ]
    } else {
        vec![
            (major << 5) | 27,
            (n >> 56) as u8,
            ((n >> 48) & 0xFF) as u8,
            ((n >> 40) & 0xFF) as u8,
            ((n >> 32) & 0xFF) as u8,
            ((n >> 24) & 0xFF) as u8,
            ((n >> 16) & 0xFF) as u8,
            ((n >> 8) & 0xFF) as u8,
            (n & 0xFF) as u8,
        ]
    }
}

fn cbor_uint(n: u64) -> Vec<u8> {
    cbor_header(0, n)
}

pub fn cbor_int(n: i64) -> Vec<u8> {
    if n >= 0 {
        cbor_uint(n as u64)
    } else {
        let m = (-n - 1) as u64;
        cbor_header(1, m)
    }
}

pub fn cbor_bytes(b: &[u8]) -> Vec<u8> {
    let mut v = cbor_header(2, b.len() as u64);
    v.extend_from_slice(b);
    v
}

pub fn cbor_array(elements: &[Vec<u8>]) -> Vec<u8> {
    let mut v = cbor_header(4, elements.len() as u64);
    for el in elements {
        v.extend_from_slice(el);
    }
    v
}

pub fn cbor_map(entries: &[(Vec<u8>, Vec<u8>)]) -> Vec<u8> {
    let mut v = cbor_header(5, entries.len() as u64);
    for (k, val) in entries {
        v.extend_from_slice(k);
        v.extend_from_slice(val);
    }
    v
}

fn cbor_tag(tag: u64) -> Vec<u8> {
    cbor_header(6, tag)
}

/// Encode a Plutus Data constructor (Constr n fields).
/// Supports indices 0-6 (tags 121-127) and 7+ (tag 102 with [n, [fields]]).
pub fn constr(n: u64, fields: &[Vec<u8>]) -> Vec<u8> {
    if n <= 6 {
        let mut v = cbor_tag(121 + n);
        v.extend_from_slice(&cbor_array(fields));
        v
    } else {
        let mut v = cbor_tag(102);
        let inner_array = cbor_array(fields);
        v.extend_from_slice(&cbor_array(&[cbor_uint(n), inner_array]));
        v
    }
}

// ── Value (Cardano multi-asset value) ────────────────────────────────────────

/// Cardano Value: a map from PolicyId → map of AssetName → quantity.
/// ADA uses empty-byte PolicyId and empty-byte AssetName.
#[derive(Debug, Clone)]
pub struct CardanoValue {
    /// ADA amount in lovelace
    pub lovelace: i64,
    // Native assets omitted for now; treasury contracts are ADA-only in practice
}

impl CardanoValue {
    pub fn lovelace(amount: i64) -> Self {
        Self { lovelace: amount }
    }

    /// Encode as Plutus Data Value: `{h"": {h"": lovelace}}`
    pub fn to_cbor(&self) -> Vec<u8> {
        let inner = cbor_map(&[(cbor_bytes(b""), cbor_int(self.lovelace))]);
        cbor_map(&[(cbor_bytes(b""), inner)])
    }
}

// ── MultisigScript ────────────────────────────────────────────────────────────

/// SundaeSwap multisig script type (mirrors the Aiken MultisigScript type).
/// Constructor indices match the Aiken enum definition order.
#[derive(Debug, Clone)]
pub enum SundaeMultisigScript {
    Signature { key_hash: Vec<u8> },                          // Constr 0
    AllOf { scripts: Vec<SundaeMultisigScript> },             // Constr 1
    AnyOf { scripts: Vec<SundaeMultisigScript> },             // Constr 2
    AtLeast { required: i64, scripts: Vec<SundaeMultisigScript> }, // Constr 3
    Before { time: i64 },                                      // Constr 4
    After { time: i64 },                                       // Constr 5
    Script { script_hash: Vec<u8> },                          // Constr 6
}

impl SundaeMultisigScript {
    /// Parse from key-hash hex string (creates a Signature variant)
    pub fn from_key_hash_hex(hex_str: &str) -> Result<Self> {
        let bytes = hex::decode(hex_str.trim())?;
        if bytes.len() != 28 {
            bail!("Key hash must be 28 bytes (56 hex chars), got {}", bytes.len());
        }
        Ok(Self::Signature { key_hash: bytes })
    }

    /// Encode to Plutus Data CBOR
    pub fn to_cbor(&self) -> Vec<u8> {
        match self {
            Self::Signature { key_hash } => {
                constr(0, &[cbor_bytes(key_hash)])
            }
            Self::AllOf { scripts } => {
                let encoded: Vec<Vec<u8>> = scripts.iter().map(|s| s.to_cbor()).collect();
                constr(1, &[cbor_array(&encoded)])
            }
            Self::AnyOf { scripts } => {
                let encoded: Vec<Vec<u8>> = scripts.iter().map(|s| s.to_cbor()).collect();
                constr(2, &[cbor_array(&encoded)])
            }
            Self::AtLeast { required, scripts } => {
                let encoded: Vec<Vec<u8>> = scripts.iter().map(|s| s.to_cbor()).collect();
                constr(3, &[cbor_int(*required), cbor_array(&encoded)])
            }
            Self::Before { time } => {
                constr(4, &[cbor_int(*time)])
            }
            Self::After { time } => {
                constr(5, &[cbor_int(*time)])
            }
            Self::Script { script_hash } => {
                constr(6, &[cbor_bytes(script_hash)])
            }
        }
    }
}

// ── PayoutStatus ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum PayoutStatus {
    Active,  // Constr 0 []
    Paused,  // Constr 1 []
}

impl PayoutStatus {
    pub fn to_cbor(&self) -> Vec<u8> {
        match self {
            Self::Active => constr(0, &[]),
            Self::Paused => constr(1, &[]),
        }
    }

    /// Decode from CBOR bytes (first byte determines constructor)
    pub fn from_cbor(bytes: &[u8]) -> Result<Self> {
        // Constr 0 starts with d879, Constr 1 starts with d87a
        if bytes.len() < 2 {
            bail!("PayoutStatus CBOR too short");
        }
        if bytes[0] == 0xd8 && bytes[1] == 0x79 {
            Ok(Self::Active)
        } else if bytes[0] == 0xd8 && bytes[1] == 0x7a {
            Ok(Self::Paused)
        } else {
            bail!("Unknown PayoutStatus constructor: {:02x}{:02x}", bytes[0], bytes[1])
        }
    }
}

// ── Payout ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Payout {
    /// Maturation timestamp in POSIX milliseconds
    pub maturation: i64,
    /// Value to release at maturation
    pub value: CardanoValue,
    /// Current status
    pub status: PayoutStatus,
}

impl Payout {
    pub fn new(maturation_ms: i64, lovelace: i64, status: PayoutStatus) -> Self {
        Self {
            maturation: maturation_ms,
            value: CardanoValue::lovelace(lovelace),
            status,
        }
    }

    pub fn active(maturation_ms: i64, lovelace: i64) -> Self {
        Self::new(maturation_ms, lovelace, PayoutStatus::Active)
    }

    pub fn to_cbor(&self) -> Vec<u8> {
        constr(0, &[
            cbor_int(self.maturation),
            self.value.to_cbor(),
            self.status.to_cbor(),
        ])
    }

    /// True if this payout is matured and active (i.e. withdrawable) at the given POSIX ms time
    pub fn is_withdrawable(&self, current_time_ms: i64) -> bool {
        self.status == PayoutStatus::Active && self.maturation <= current_time_ms
    }
}

// ── VendorDatum ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct VendorDatum {
    /// Multisig script that must be satisfied to withdraw
    pub vendor: SundaeMultisigScript,
    /// Milestone payout schedule
    pub payouts: Vec<Payout>,
}

impl VendorDatum {
    pub fn new(vendor: SundaeMultisigScript, payouts: Vec<Payout>) -> Self {
        Self { vendor, payouts }
    }

    pub fn to_cbor(&self) -> Vec<u8> {
        let payouts_encoded: Vec<Vec<u8>> = self.payouts.iter().map(|p| p.to_cbor()).collect();
        constr(0, &[
            self.vendor.to_cbor(),
            cbor_array(&payouts_encoded),
        ])
    }

    /// Calculate total value of all matured, active payouts at the given time
    pub fn matured_lovelace(&self, current_time_ms: i64) -> i64 {
        self.payouts.iter()
            .filter(|p| p.is_withdrawable(current_time_ms))
            .map(|p| p.value.lovelace)
            .sum()
    }

    /// Return a new VendorDatum with only the unmatured payouts remaining
    pub fn remaining_after_withdrawal(&self, current_time_ms: i64) -> Self {
        let remaining: Vec<Payout> = self.payouts.iter()
            .filter(|p| !p.is_withdrawable(current_time_ms))
            .cloned()
            .collect();
        Self {
            vendor: self.vendor.clone(),
            payouts: remaining,
        }
    }

    /// Apply adjudication: update statuses. Returns error if statuses count doesn't match.
    pub fn with_updated_statuses(&self, statuses: &[PayoutStatus]) -> Result<Self> {
        if statuses.len() != self.payouts.len() {
            bail!(
                "Status count {} must match payout count {}",
                statuses.len(),
                self.payouts.len()
            );
        }
        let updated: Vec<Payout> = self.payouts.iter().zip(statuses.iter())
            .map(|(p, s)| Payout {
                maturation: p.maturation,
                value: p.value.clone(),
                status: s.clone(),
            })
            .collect();
        Ok(Self { vendor: self.vendor.clone(), payouts: updated })
    }
}

// ── Treasury redeemer ─────────────────────────────────────────────────────────

/// Redeemer for spending from the treasury script.
/// Constructor indices must match the Aiken TreasurySpendRedeemer enum order.
#[derive(Debug, Clone)]
pub enum TreasuryRedeemer {
    Reorganize,                      // Constr 0
    SweepTreasury,                   // Constr 1
    Fund { amount: CardanoValue },   // Constr 2
    Disburse { amount: CardanoValue }, // Constr 3
}

impl TreasuryRedeemer {
    pub fn to_cbor(&self) -> Vec<u8> {
        match self {
            Self::Reorganize => constr(0, &[]),
            Self::SweepTreasury => constr(1, &[]),
            Self::Fund { amount } => constr(2, &[amount.to_cbor()]),
            Self::Disburse { amount } => constr(3, &[amount.to_cbor()]),
        }
    }
}

// ── Vendor redeemer ───────────────────────────────────────────────────────────

/// Redeemer for spending from a vendor script UTxO.
/// Constructor indices must match the Aiken VendorSpendRedeemer enum order.
#[derive(Debug, Clone)]
pub enum VendorRedeemer {
    Withdraw,                                   // Constr 0
    Adjudicate { statuses: Vec<PayoutStatus> }, // Constr 1
    // Modify omitted (complex, not needed for MVP)
    SweepVendor,                                // Constr 3
    Malformed,                                  // Constr 4
}

impl VendorRedeemer {
    pub fn to_cbor(&self) -> Vec<u8> {
        match self {
            Self::Withdraw => constr(0, &[]),
            Self::Adjudicate { statuses } => {
                let encoded: Vec<Vec<u8>> = statuses.iter().map(|s| s.to_cbor()).collect();
                constr(1, &[cbor_array(&encoded)])
            }
            Self::SweepVendor => constr(3, &[]),
            Self::Malformed => constr(4, &[]),
        }
    }
}

// ── ScriptHashRegistry datum ──────────────────────────────────────────────────

/// Datum stored in the registry NFT UTxO.
/// Maps treasury and vendor script credentials.
#[derive(Debug, Clone)]
pub struct ScriptHashRegistry {
    pub treasury_hash: Vec<u8>, // 28 bytes
    pub vendor_hash: Vec<u8>,   // 28 bytes
}

impl ScriptHashRegistry {
    pub fn to_cbor(&self) -> Vec<u8> {
        // Credential::ScriptHash is encoded as Constr 1 [bytes(28)]
        let treasury_cred = constr(1, &[cbor_bytes(&self.treasury_hash)]);
        let vendor_cred = constr(1, &[cbor_bytes(&self.vendor_hash)]);
        constr(0, &[treasury_cred, vendor_cred])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constr_0_empty() {
        // Constr 0 [] should be d87980
        let result = constr(0, &[]);
        assert_eq!(result, hex::decode("d87980").unwrap());
    }

    #[test]
    fn test_constr_1_empty() {
        // Constr 1 [] should be d87a80
        let result = constr(1, &[]);
        assert_eq!(result, hex::decode("d87a80").unwrap());
    }

    #[test]
    fn test_payout_status_active() {
        let active = PayoutStatus::Active.to_cbor();
        assert_eq!(active, hex::decode("d87980").unwrap());
    }

    #[test]
    fn test_payout_status_paused() {
        let paused = PayoutStatus::Paused.to_cbor();
        assert_eq!(paused, hex::decode("d87a80").unwrap());
    }

    #[test]
    fn test_vendor_redeemer_withdraw() {
        let redeemer = VendorRedeemer::Withdraw;
        let cbor = redeemer.to_cbor();
        // Should be Constr 0 [] = d87980
        assert_eq!(cbor, hex::decode("d87980").unwrap());
    }

    #[test]
    fn test_cbor_int_positive() {
        // 1_000_000 lovelace
        let result = cbor_int(1_000_000);
        let expected = hex::decode("1a000f4240").unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_ada_value_encoding() {
        // ADA 1 = 1_000_000 lovelace
        let value = CardanoValue::lovelace(1_000_000);
        let cbor = value.to_cbor();
        // {h"": {h"": 1000000}} = a1 40 a1 40 1a000f4240
        let expected = hex::decode("a140a1401a000f4240").unwrap();
        assert_eq!(cbor, expected);
    }

    #[test]
    fn test_multisig_signature() {
        let key_hash = vec![0x42u8; 28];
        let script = SundaeMultisigScript::Signature { key_hash: key_hash.clone() };
        let cbor = script.to_cbor();
        // Constr 0 [bytes(28)] = d879 81 581c <28 bytes>
        assert!(cbor.starts_with(&[0xd8, 0x79]));
    }

    #[test]
    fn test_treasury_redeemer_fund() {
        let redeemer = TreasuryRedeemer::Fund {
            amount: CardanoValue::lovelace(5_000_000),
        };
        let cbor = redeemer.to_cbor();
        // Constr 2 [value_map] starts with d87b
        assert_eq!(cbor[0], 0xd8);
        assert_eq!(cbor[1], 0x7b); // tag 123 = Constr 2
    }

    #[test]
    fn test_vendor_datum_roundtrip_encoding() {
        let vendor = SundaeMultisigScript::Signature { key_hash: vec![0x01u8; 28] };
        let payouts = vec![
            Payout::active(1_700_000_000_000, 5_000_000),
            Payout::active(1_702_000_000_000, 5_000_000),
        ];
        let datum = VendorDatum::new(vendor, payouts);
        let cbor = datum.to_cbor();
        assert!(!cbor.is_empty());
        // Should start with Constr 0 tag
        assert_eq!(cbor[0], 0xd8);
        assert_eq!(cbor[1], 0x79);
    }

    #[test]
    fn test_matured_payouts() {
        let vendor = SundaeMultisigScript::Signature { key_hash: vec![0x01u8; 28] };
        let now = 1_700_000_000_000i64;
        let payouts = vec![
            Payout::active(now - 1000, 3_000_000),  // matured
            Payout::active(now + 1000, 2_000_000),  // not yet
        ];
        let datum = VendorDatum::new(vendor, payouts);
        assert_eq!(datum.matured_lovelace(now), 3_000_000);
        assert_eq!(datum.remaining_after_withdrawal(now).payouts.len(), 1);
    }
}
