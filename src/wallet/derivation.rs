// Cardano key derivation and address generation
// CIP-1852: HD Wallets for Cardano
// https://cips.cardano.org/cip/CIP-1852

use bip39::Mnemonic;
use ed25519_bip32::{XPrv, XPub, DerivationScheme, XPRV_SIZE};
use pallas_addresses::{Address, Network as PallasNetwork, ShelleyAddress, ShelleyPaymentPart, ShelleyDelegationPart};
use pallas_crypto::key::ed25519::PublicKey;
use pallas_traverse::ComputeHash;
use cryptoxide::{hmac::Hmac, pbkdf2::pbkdf2, sha2::Sha512};
use thiserror::Error;

#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum DerivationError {
    #[error("Failed to derive key: {0}")]
    DerivationFailed(String),

    #[error("Invalid mnemonic: {0}")]
    InvalidMnemonic(String),

    #[error("Address generation failed: {0}")]
    AddressGenerationFailed(String),
}

pub type DerivationResult<T> = Result<T, DerivationError>;

/// Cardano network type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Network {
    Mainnet,
    Testnet,
}

impl Network {
    pub fn to_pallas(&self) -> PallasNetwork {
        match self {
            Network::Mainnet => PallasNetwork::Mainnet,
            Network::Testnet => PallasNetwork::Testnet,
        }
    }
}

/// HD wallet account
#[derive(Clone)]
#[allow(dead_code)]
pub struct Account {
    #[allow(dead_code)]
    pub account_index: u32,
    pub account_key: XPrv,
    pub payment_key: XPrv,
    pub stake_key: XPrv,
}

/// Derive root key from mnemonic
/// Uses ICARUS derivation: PBKDF2-HMAC-SHA512 with 4096 iterations
pub fn derive_root_key(mnemonic: &Mnemonic) -> DerivationResult<XPrv> {
    // Get entropy bytes from mnemonic
    let entropy = mnemonic.to_entropy();

    // ICARUS derivation: PBKDF2-HMAC-SHA512
    // - Password: entropy
    // - Salt: empty string
    // - Iterations: 4096
    // - Output: 96 bytes for ed25519-bip32
    let mut pbkdf2_result = [0; XPRV_SIZE];
    const ITER: u32 = 4096;
    let mut mac = Hmac::new(Sha512::new(), "".as_bytes());
    pbkdf2(&mut mac, &entropy, ITER, &mut pbkdf2_result);

    // Derive root private key using V2 scheme (Shelley)
    Ok(XPrv::normalize_bytes_force3rd(pbkdf2_result))
}

/// Derive account keys for a given account index
/// Follows CIP-1852 path: m/1852'/1815'/account'/role/index
pub fn derive_account(root: &XPrv, account_index: u32) -> DerivationResult<Account> {
    // m/1852'/1815'/account'
    let purpose = root.derive(DerivationScheme::V2, 0x80000000 | 1852); // 1852'
    let coin_type = purpose.derive(DerivationScheme::V2, 0x80000000 | 1815); // 1815'
    let account_key = coin_type.derive(DerivationScheme::V2, 0x80000000 | account_index);

    // Payment key: m/1852'/1815'/account'/0/0
    let payment_chain = account_key.derive(DerivationScheme::V2, 0);
    let payment_key = payment_chain.derive(DerivationScheme::V2, 0);

    // Stake key: m/1852'/1815'/account'/2/0
    let stake_chain = account_key.derive(DerivationScheme::V2, 2);
    let stake_key = stake_chain.derive(DerivationScheme::V2, 0);

    Ok(Account {
        account_index,
        account_key,
        payment_key,
        stake_key,
    })
}

/// Derive payment address for an account
/// Follows CIP-1852 path: m/1852'/1815'/account'/0/address_index
pub fn derive_payment_address(
    account_key: &XPrv,
    address_index: u32,
    _stake_key: &XPrv,
    network: Network,
) -> DerivationResult<String> {
    // Derive using public key derivation (same as keys.rs)
    let account_xpub: XPub = account_key.public();

    // Derive payment key at index
    let payment_chain_xpub = account_xpub.derive(DerivationScheme::V2, 0)
        .map_err(|e| DerivationError::DerivationFailed(format!("Failed to derive payment chain: {}", e)))?;
    let payment_xpub = payment_chain_xpub.derive(DerivationScheme::V2, address_index)
        .map_err(|e| DerivationError::DerivationFailed(format!("Failed to derive payment key: {}", e)))?;

    // Derive stake key
    let stake_chain_xpub = account_xpub.derive(DerivationScheme::V2, 2)
        .map_err(|e| DerivationError::DerivationFailed(format!("Failed to derive stake chain: {}", e)))?;
    let stake_xpub = stake_chain_xpub.derive(DerivationScheme::V2, 0)
        .map_err(|e| DerivationError::DerivationFailed(format!("Failed to derive stake key: {}", e)))?;

    // Get just the public keys (32 bytes each) without chain code and compute hashes
    let payment_pubkey = PublicKey::from(payment_xpub.public_key());
    let stake_pubkey = PublicKey::from(stake_xpub.public_key());

    let payment_hash = payment_pubkey.compute_hash();
    let stake_hash = stake_pubkey.compute_hash();

    let addr = ShelleyAddress::new(
        network.to_pallas(),
        ShelleyPaymentPart::Key(payment_hash),
        ShelleyDelegationPart::Key(stake_hash),
    );

    Address::Shelley(addr).to_bech32().map_err(|e| {
        DerivationError::AddressGenerationFailed(format!("Failed to encode address: {}", e))
    })
}

/// Derive stake address for an account
#[allow(dead_code)]
pub fn derive_stake_address(
    stake_key: &XPrv,
    network: Network,
) -> DerivationResult<String> {
    let stake_pub: XPub = stake_key.public();

    // Get just the public key (32 bytes) without chain code and compute hash
    let stake_pubkey = PublicKey::from(stake_pub.public_key());
    let stake_hash = stake_pubkey.compute_hash();

    let addr = ShelleyAddress::new(
        network.to_pallas(),
        ShelleyPaymentPart::Script(stake_hash), // Using Script part for stake-only address
        ShelleyDelegationPart::Null,
    );

    Address::Shelley(addr).to_bech32().map_err(|e| {
        DerivationError::AddressGenerationFailed(format!("Failed to encode stake address: {}", e))
    })
}

/// Derive enterprise address (payment only, no staking)
#[allow(dead_code)]
pub fn derive_enterprise_address(
    account_key: &XPrv,
    address_index: u32,
    network: Network,
) -> DerivationResult<String> {
    // Derive using public key derivation (same as keys.rs)
    let account_xpub: XPub = account_key.public();
    let payment_chain_xpub = account_xpub.derive(DerivationScheme::V2, 0)
        .map_err(|e| DerivationError::DerivationFailed(format!("Failed to derive payment chain: {}", e)))?;
    let payment_xpub = payment_chain_xpub.derive(DerivationScheme::V2, address_index)
        .map_err(|e| DerivationError::DerivationFailed(format!("Failed to derive payment key: {}", e)))?;

    // Get just the public key (32 bytes) without chain code and compute hash
    let payment_pubkey_bytes = payment_xpub.public_key();
    let payment_pubkey = PublicKey::from(payment_pubkey_bytes);
    let payment_hash = payment_pubkey.compute_hash();

    // Create enterprise address (payment only, no staking)
    let addr = ShelleyAddress::new(
        network.to_pallas(),
        ShelleyPaymentPart::Key(payment_hash),
        ShelleyDelegationPart::Null, // Enterprise address has no stake component
    );

    Address::Shelley(addr).to_bech32().map_err(|e| {
        DerivationError::AddressGenerationFailed(format!("Failed to encode address: {}", e))
    })
}

/// Get account xpub from account keys
#[allow(dead_code)]
pub fn get_account_xpub(account: &Account) -> String {
    hex::encode(account.payment_key.public().as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bip39::Language;

    const TEST_MNEMONIC: &str = "test walk nut penalty hip pave soap entry language right filter choice";

    #[test]
    fn test_derive_root_key() {
        let mnemonic = Mnemonic::parse_in(Language::English, TEST_MNEMONIC).unwrap();
        let root = derive_root_key(&mnemonic).unwrap();
        assert_eq!(root.as_ref().len(), 96);
    }

    #[test]
    fn test_derive_account() {
        let mnemonic = Mnemonic::parse_in(Language::English, TEST_MNEMONIC).unwrap();
        let root = derive_root_key(&mnemonic).unwrap();
        let account = derive_account(&root, 0).unwrap();
        assert_eq!(account.account_index, 0);
    }

    #[test]
    fn test_derive_payment_address() {
        let mnemonic = Mnemonic::parse_in(Language::English, TEST_MNEMONIC).unwrap();
        let root = derive_root_key(&mnemonic).unwrap();
        let account = derive_account(&root, 0).unwrap();

        let address = derive_payment_address(
            &account.account_key,
            0,
            &account.stake_key,
            Network::Testnet,
        ).unwrap();

        // Should be a valid bech32 address
        assert!(address.starts_with("addr_test") || address.starts_with("addr"));
    }

    #[test]
    fn test_derive_enterprise_address() {
        let mnemonic = Mnemonic::parse_in(Language::English, TEST_MNEMONIC).unwrap();
        let root = derive_root_key(&mnemonic).unwrap();
        let account = derive_account(&root, 0).unwrap();

        let address = derive_enterprise_address(
            &account.account_key,
            0,
            Network::Testnet,
        ).unwrap();

        // Should be a valid bech32 enterprise address
        assert!(address.starts_with("addr_test") || address.starts_with("addr"));
    }
}
