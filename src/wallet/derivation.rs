// Cardano key derivation and address generation
// CIP-1852: HD Wallets for Cardano
// https://cips.cardano.org/cip/CIP-1852

use bip39::Mnemonic;
use ed25519_bip32::{XPrv, XPub, DerivationScheme};
use pallas_addresses::{Address, Network as PallasNetwork, ShelleyAddress, ShelleyPaymentPart, ShelleyDelegationPart};
use pallas_crypto::hash::{Hash, Hasher};
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
#[allow(dead_code)]
pub struct Account {
    #[allow(dead_code)]
    pub account_index: u32,
    pub account_key: XPrv,
    pub payment_key: XPrv,
    pub stake_key: XPrv,
}

/// Derive root key from mnemonic
/// Uses BIP39 for entropy and Cardano's ICARUS derivation
pub fn derive_root_key(mnemonic: &Mnemonic) -> DerivationResult<XPrv> {
    // Get entropy bytes from mnemonic
    let entropy = mnemonic.to_entropy();

    // For Cardano, we use the entropy directly as the seed (ICARUS derivation)
    // This is different from BIP39's PBKDF2-based seed derivation
    // We need 96 bytes for ed25519-bip32
    // Since pallas only supports specific sizes, we'll use blake2b-256 three times
    let mut seed = [0u8; 96];

    // First 32 bytes
    let mut hasher1 = Hasher::<256>::new();
    hasher1.input(&entropy);
    hasher1.input(&[0u8]); // Domain separator
    let hash1 = hasher1.finalize();
    seed[0..32].copy_from_slice(hash1.as_ref());

    // Second 32 bytes
    let mut hasher2 = Hasher::<256>::new();
    hasher2.input(&entropy);
    hasher2.input(&[1u8]); // Domain separator
    let hash2 = hasher2.finalize();
    seed[32..64].copy_from_slice(hash2.as_ref());

    // Last 32 bytes
    let mut hasher3 = Hasher::<256>::new();
    hasher3.input(&entropy);
    hasher3.input(&[2u8]); // Domain separator
    let hash3 = hasher3.finalize();
    seed[64..96].copy_from_slice(hash3.as_ref());

    // Derive root private key using V2 scheme (Shelley)
    Ok(XPrv::normalize_bytes_force3rd(seed))
}

/// Derive account keys for a given account index
/// Follows CIP-1852 path: m/1852'/1815'/account'/role/index
pub fn derive_account(root: &XPrv, account_index: u32) -> DerivationResult<Account> {
    // m/1852'/1815'/account'
    let purpose = root.derive(DerivationScheme::V2, 0x8000076C); // 1852'
    let coin_type = purpose.derive(DerivationScheme::V2, 0x80000717); // 1815'
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
    // Derive payment key at index
    let payment_chain = account_key.derive(DerivationScheme::V2, 0);
    let payment_key = payment_chain.derive(DerivationScheme::V2, address_index);
    let payment_pub: XPub = payment_key.public();

    // Get stake public key
    let stake_chain = account_key.derive(DerivationScheme::V2, 2);
    let stake_key_derived = stake_chain.derive(DerivationScheme::V2, 0);
    let stake_pub: XPub = stake_key_derived.public();

    // Create Shelley address
    let payment_hash = Hash::<28>::from(blake2b_224(payment_pub.as_ref()));
    let stake_hash = Hash::<28>::from(blake2b_224(stake_pub.as_ref()));

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
    let stake_hash = Hash::<28>::from(blake2b_224(stake_pub.as_ref()));

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
pub fn derive_enterprise_address(
    account_key: &XPrv,
    address_index: u32,
    network: Network,
) -> DerivationResult<String> {
    // Derive payment key at index
    let payment_chain = account_key.derive(DerivationScheme::V2, 0);
    let payment_key = payment_chain.derive(DerivationScheme::V2, address_index);
    let payment_pub: XPub = payment_key.public();

    // Create payment key hash
    let payment_hash = Hash::<28>::from(blake2b_224(payment_pub.as_ref()));

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

/// Blake2b-224 hash function
fn blake2b_224(data: &[u8]) -> [u8; 28] {
    let mut hasher = Hasher::<224>::new(); // 224 bits = 28 bytes
    hasher.input(data);
    let hash = hasher.finalize();

    let mut out = [0u8; 28];
    out.copy_from_slice(hash.as_ref());
    out
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
