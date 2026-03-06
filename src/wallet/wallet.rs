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
pub struct Wallet {
    root_key: XPrv,
    account: Account,
    network: Network,
}

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
            &self.root_key,
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

    /// Get the payment key at the given address index
    ///
    /// Returns the extended private key for signing
    pub fn payment_key(&self, address_index: u32) -> DerivationResult<XPrv> {
        use ed25519_bip32::DerivationScheme;

        // m/1852'/1815'/account'/0/address_index
        let payment_chain = self.account.payment_key.derive(DerivationScheme::V2, 0);
        let payment_key = payment_chain.derive(DerivationScheme::V2, address_index);

        Ok(payment_key)
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
