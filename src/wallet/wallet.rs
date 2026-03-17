// High-level wallet abstraction for Cardano operations
//
// Provides a simplified API over the low-level derivation functions

use super::derivation::{self, Account, DerivationResult, Network};
use bip39::Mnemonic;
use ed25519_bip32::XPrv;
use pallas_addresses::Address;

/// High-level wallet interface
///
/// Wraps BIP39 mnemonic and provides convenient methods for:
/// - Address derivation (CIP-1852)
/// - Key management
/// - Signing operations
///
/// # Example
/// ```no_run
/// use hayate::wallet::{Wallet, Network};
/// use bip39::Mnemonic;
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let mnemonic = Mnemonic::parse("your mnemonic words here")?;
/// let wallet = Wallet::from_mnemonic(mnemonic, Network::Testnet, 0)?;
///
/// // Get payment address at index 0
/// let addr = wallet.payment_address(0)?;
/// println!("Payment address: {}", addr);
///
/// // Get payment key for signing
/// let payment_key = wallet.payment_key(0)?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct Wallet {
    #[allow(dead_code)]
    root_key: XPrv,
    account: Account,
    network: Network,
}

#[allow(dead_code)]
impl Wallet {
    /// Create a wallet from a BIP39 mnemonic
    ///
    /// # Arguments
    /// * `mnemonic` - BIP39 mnemonic phrase
    /// * `network` - Cardano network (mainnet or testnet)
    /// * `account_index` - Account index for CIP-1852 derivation
    ///
    /// Follows CIP-1852 path: m/1852'/1815'/account'
    pub fn from_mnemonic(
        mnemonic: Mnemonic,
        network: Network,
        account_index: u32,
    ) -> DerivationResult<Self> {
        let root_key = derivation::derive_root_key(&mnemonic)?;
        let account = derivation::derive_account(&root_key, account_index)?;

        Ok(Self {
            root_key,
            account,
            network,
        })
    }

    /// Create a wallet from mnemonic string
    ///
    /// Convenience method that parses the mnemonic first
    pub fn from_mnemonic_str(
        mnemonic_str: &str,
        network: Network,
        account_index: u32,
    ) -> DerivationResult<Self> {
        let mnemonic = Mnemonic::parse(mnemonic_str)
            .map_err(|e| derivation::DerivationError::InvalidMnemonic(e.to_string()))?;
        Self::from_mnemonic(mnemonic, network, account_index)
    }

    /// Get the account index for this wallet
    pub fn account_index(&self) -> u32 {
        self.account.account_index
    }

    /// Get the network for this wallet
    pub fn network(&self) -> Network {
        self.network
    }

    /// Derive a payment address at the given index
    ///
    /// Follows CIP-1852: m/1852'/1815'/account'/0/address_index
    ///
    /// Returns bech32-encoded Shelley address (addr1... or addr_test1...)
    pub fn payment_address(&self, address_index: u32) -> DerivationResult<String> {
        derivation::derive_payment_address(
            &self.account.account_key,
            address_index,
            &self.account.stake_key,
            self.network,
        )
    }

    /// Derive a payment address and return as raw bytes
    ///
    /// Useful for transaction building where you need the address bytes directly
    pub fn payment_address_bytes(&self, address_index: u32) -> DerivationResult<Vec<u8>> {
        let bech32 = self.payment_address(address_index)?;
        let addr = Address::from_bech32(&bech32)
            .map_err(|e| derivation::DerivationError::AddressGenerationFailed(format!("Failed to decode bech32: {}", e)))?;
        Ok(addr.to_vec())
    }

    /// Derive an enterprise address (payment only, no staking)
    ///
    /// Enterprise addresses don't have a staking component and are used for
    /// exchanges, faucets, and other scenarios where staking is not needed.
    pub fn enterprise_address(&self, address_index: u32) -> DerivationResult<String> {
        derivation::derive_enterprise_address(
            &self.account.account_key,
            address_index,
            self.network,
        )
    }

    /// Derive an enterprise address and return as raw bytes
    ///
    /// Useful for transaction building where you need the address bytes directly
    pub fn enterprise_address_bytes(&self, address_index: u32) -> DerivationResult<Vec<u8>> {
        let bech32 = self.enterprise_address(address_index)?;
        let addr = Address::from_bech32(&bech32)
            .map_err(|e| derivation::DerivationError::AddressGenerationFailed(format!("Failed to decode bech32: {}", e)))?;
        Ok(addr.to_vec())
    }

    /// Get the payment key at the given address index
    ///
    /// Returns the extended private key for signing
    pub fn payment_key(&self, address_index: u32) -> DerivationResult<XPrv> {
        use ed25519_bip32::DerivationScheme;

        // m/1852'/1815'/account'/0/address_index
        let payment_chain = self.account.account_key.derive(DerivationScheme::V2, 0);
        let payment_key = payment_chain.derive(DerivationScheme::V2, address_index);

        Ok(payment_key)
    }

    /// Get the account for debugging
    pub fn account(&self) -> &Account {
        &self.account
    }

    /// Get the stake key for this account
    pub fn stake_key(&self) -> &XPrv {
        &self.account.stake_key
    }

    /// Get the root extended private key
    ///
    /// Use with caution - this is the master key for the entire wallet
    pub fn root_key(&self) -> &XPrv {
        &self.root_key
    }

    /// Get the payment key hash for a given address index
    ///
    /// This computes the BLAKE2b-224 hash of the 32-byte Ed25519 public key
    /// (without chain code), which is used in Cardano addresses and transaction
    /// signing requirements.
    ///
    /// # Returns
    /// A 28-byte hash of the public key, as used in Cardano key hashes
    pub fn payment_key_hash(&self, address_index: u32) -> DerivationResult<[u8; 28]> {
        use pallas_crypto::key::ed25519::PublicKey;
        use pallas_traverse::ComputeHash;

        // Derive payment key at the given index
        let payment_key = self.payment_key(address_index)?;

        // Extract just the 32-byte public key (without chain code)
        let payment_xpub = payment_key.public();
        let pubkey_bytes = payment_xpub.public_key(); // [u8; 32]

        // Create pallas PublicKey and compute BLAKE2b-224 hash
        let public_key = PublicKey::from(pubkey_bytes);
        let hash = public_key.compute_hash();

        // Convert Hash<28> to [u8; 28]
        let mut result = [0u8; 28];
        result.copy_from_slice(hash.as_ref());
        Ok(result)
    }

    /// Get the payment key at given index as pallas SecretKeyExtended for signing
    ///
    /// This is useful for transaction signing
    pub fn payment_signing_key(&self, address_index: u32) -> DerivationResult<pallas_crypto::key::ed25519::SecretKeyExtended> {
        let payment_key = self.payment_key(address_index)?;
        xprv_to_pallas_privatekey(&payment_key)
    }
}

/// Convert ed25519_bip32::XPrv to pallas SecretKeyExtended
#[allow(dead_code)]
fn xprv_to_pallas_privatekey(xprv: &XPrv) -> DerivationResult<pallas_crypto::key::ed25519::SecretKeyExtended> {
    // XPrv has a method to get just the 64-byte extended secret key (without chain code)
    let extended_secret_key_bytes = xprv.extended_secret_key();

    // Create pallas SecretKeyExtended using from_bytes_unchecked (safe because XPrv guarantees valid bytes)
    use pallas_crypto::key::ed25519::SecretKeyExtended;
    let secret_key_extended = unsafe {
        SecretKeyExtended::from_bytes_unchecked(extended_secret_key_bytes)
    };

    // TODO: pallas_wallet removed in pallas 1.0 - return SecretKeyExtended directly
    Ok(secret_key_extended)
}

/// Convert Ed25519 secret key bytes to pallas SecretKey
///
/// This is useful for signing with temporary keys (e.g., for NFT minting)
pub fn ed25519_secret_to_privatekey(secret_bytes: &[u8]) -> derivation::DerivationResult<pallas_crypto::key::ed25519::SecretKey> {
    if secret_bytes.len() != 32 {
        return Err(derivation::DerivationError::DerivationFailed(format!(
            "Expected 32 bytes for Ed25519 secret key, got {}",
            secret_bytes.len()
        )));
    }

    // Convert to [u8; 32]
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(secret_bytes);

    // Create pallas SecretKey
    use pallas_crypto::key::ed25519::SecretKey;
    let secret_key = SecretKey::from(bytes);

    // TODO: pallas_wallet removed in pallas 1.0 - return SecretKey directly
    Ok(secret_key)
}

/// Convert Ed25519 secret key bytes to pallas SecretKeyExtended
///
/// This creates a 64-byte extended key by appending a zero chain code to the 32-byte secret.
/// This is useful for transaction signing where SecretKeyExtended is required.
pub fn ed25519_secret_to_extended_privatekey(secret_bytes: &[u8]) -> derivation::DerivationResult<pallas_crypto::key::ed25519::SecretKeyExtended> {
    if secret_bytes.len() != 32 {
        return Err(derivation::DerivationError::DerivationFailed(format!(
            "Expected 32 bytes for Ed25519 secret key, got {}",
            secret_bytes.len()
        )));
    }

    // Create 64-byte extended key (32-byte secret + 32-byte zero chain code)
    let mut extended_bytes = [0u8; 64];
    extended_bytes[0..32].copy_from_slice(secret_bytes);
    // Chain code remains zeros (bytes 32..64)

    // Create pallas SecretKeyExtended
    use pallas_crypto::key::ed25519::SecretKeyExtended;
    let secret_key = SecretKeyExtended::from_bytes(extended_bytes)
        .map_err(|e| derivation::DerivationError::DerivationFailed(format!(
            "Failed to create SecretKeyExtended: {:?}",
            e
        )))?;

    Ok(secret_key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wallet_creation() {
        let mnemonic_str = "test walk nut penalty hip pave soap entry language right filter choice";
        let wallet = Wallet::from_mnemonic_str(mnemonic_str, Network::Testnet, 0);
        assert!(wallet.is_ok());

        let wallet = wallet.unwrap();
        assert_eq!(wallet.account_index(), 0);
        assert_eq!(wallet.network(), Network::Testnet);
    }

    #[test]
    fn test_payment_address_derivation() {
        let mnemonic_str = "test walk nut penalty hip pave soap entry language right filter choice";
        let wallet = Wallet::from_mnemonic_str(mnemonic_str, Network::Testnet, 0).unwrap();

        let addr = wallet.payment_address(0);
        assert!(addr.is_ok());

        let addr_str = addr.unwrap();
        assert!(addr_str.starts_with("addr_test1"), "Address should be testnet: {}", addr_str);
    }

    #[test]
    fn test_payment_address_bytes() {
        let mnemonic_str = "test walk nut penalty hip pave soap entry language right filter choice";
        let wallet = Wallet::from_mnemonic_str(mnemonic_str, Network::Testnet, 0).unwrap();

        let addr_bytes = wallet.payment_address_bytes(0);
        assert!(addr_bytes.is_ok());

        let bytes = addr_bytes.unwrap();
        // Shelley address should be 57 bytes (1 header + 28 payment + 28 stake)
        assert_eq!(bytes.len(), 57, "Shelley address should be 57 bytes");
    }

    #[test]
    fn test_multiple_addresses() {
        let mnemonic_str = "test walk nut penalty hip pave soap entry language right filter choice";
        let wallet = Wallet::from_mnemonic_str(mnemonic_str, Network::Testnet, 0).unwrap();

        let addr0 = wallet.payment_address(0).unwrap();
        let addr1 = wallet.payment_address(1).unwrap();
        let addr2 = wallet.payment_address(2).unwrap();

        // All addresses should be different
        assert_ne!(addr0, addr1);
        assert_ne!(addr1, addr2);
        assert_ne!(addr0, addr2);
    }

    #[test]
    fn test_deterministic_addresses() {
        let mnemonic_str = "test walk nut penalty hip pave soap entry language right filter choice";

        let wallet1 = Wallet::from_mnemonic_str(mnemonic_str, Network::Testnet, 0).unwrap();
        let wallet2 = Wallet::from_mnemonic_str(mnemonic_str, Network::Testnet, 0).unwrap();

        let addr1 = wallet1.payment_address(0).unwrap();
        let addr2 = wallet2.payment_address(0).unwrap();

        // Same mnemonic should produce same addresses
        assert_eq!(addr1, addr2, "Addresses should be deterministic");
    }
}
