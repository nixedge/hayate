// Wallet storage with GPG encrypted mnemonics

use crate::gpg::{Gpg, GpgError};
use crate::wallet::derivation::{derive_root_key, derive_account, Network, Account};
use crate::wallet::mnemonic::{parse_mnemonic, validate_mnemonic, normalize_mnemonic};
use ed25519_bip32::XPrv;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum WalletStorageError {
    #[error("Wallet already exists: {0}")]
    WalletExists(String),

    #[error("Wallet not found: {0}")]
    WalletNotFound(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("GPG error: {0}")]
    Gpg(#[from] GpgError),

    #[error("Mnemonic error: {0}")]
    Mnemonic(#[from] crate::wallet::mnemonic::MnemonicError),

    #[error("Derivation error: {0}")]
    Derivation(#[from] crate::wallet::derivation::DerivationError),

    #[error("Invalid wallet name: {0}")]
    InvalidName(String),
}

pub type WalletStorageResult<T> = Result<T, WalletStorageError>;

/// Wallet metadata stored in JSON
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletMetadata {
    pub name: String,
    pub created_at: u64,
    pub network: String,
    pub encrypted: bool,
    pub account_count: u32,
}

/// Wallet storage manager
pub struct WalletStorage {
    wallet_dir: PathBuf,
}

impl WalletStorage {
    /// Create a new wallet storage manager
    pub fn new(wallet_dir: PathBuf) -> WalletStorageResult<Self> {
        fs::create_dir_all(&wallet_dir)?;
        Ok(Self { wallet_dir })
    }

    /// Get path to wallet directory
    fn wallet_path(&self, name: &str) -> PathBuf {
        self.wallet_dir.join(name)
    }

    /// Get path to mnemonic file
    fn mnemonic_path(&self, name: &str, encrypted: bool) -> PathBuf {
        if encrypted {
            self.wallet_path(name).join("mnemonic.gpg")
        } else {
            self.wallet_path(name).join("mnemonic")
        }
    }

    /// Get path to metadata file
    fn metadata_path(&self, name: &str) -> PathBuf {
        self.wallet_path(name).join("metadata.json")
    }

    /// Validate wallet name
    fn validate_name(name: &str) -> WalletStorageResult<()> {
        if name.is_empty() {
            return Err(WalletStorageError::InvalidName("Wallet name cannot be empty".to_string()));
        }

        // Check for invalid characters
        if !name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
            return Err(WalletStorageError::InvalidName(
                "Wallet name can only contain alphanumeric characters, hyphens, and underscores".to_string()
            ));
        }

        Ok(())
    }

    /// Create a new wallet with encrypted mnemonic
    pub fn create_wallet(
        &self,
        name: &str,
        mnemonic: &str,
        network: Network,
        gpg_recipient: Option<&str>,
    ) -> WalletStorageResult<WalletMetadata> {
        Self::validate_name(name)?;

        let wallet_path = self.wallet_path(name);
        if wallet_path.exists() {
            return Err(WalletStorageError::WalletExists(name.to_string()));
        }

        // Validate mnemonic
        let normalized = normalize_mnemonic(mnemonic);
        validate_mnemonic(&normalized)?;

        // Create wallet directory
        fs::create_dir_all(&wallet_path)?;

        // Determine if we should encrypt
        let encrypted = gpg_recipient.is_some();

        // Save mnemonic (encrypted or plaintext)
        let mnemonic_path = self.mnemonic_path(name, encrypted);
        if let Some(recipient) = gpg_recipient {
            // Encrypt with GPG
            Gpg::encrypt_to_file(&normalized, recipient, &mnemonic_path)?;
        } else {
            // Save plaintext (not recommended for production!)
            fs::write(&mnemonic_path, normalized)?;
        }

        // Create metadata
        let metadata = WalletMetadata {
            name: name.to_string(),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            network: match network {
                Network::Mainnet => "mainnet".to_string(),
                Network::Testnet => "testnet".to_string(),
            },
            encrypted,
            account_count: 1, // Start with one account
        };

        // Save metadata
        let metadata_path = self.metadata_path(name);
        fs::write(&metadata_path, serde_json::to_string_pretty(&metadata)?)?;

        Ok(metadata)
    }

    /// Load wallet metadata
    pub fn load_metadata(&self, name: &str) -> WalletStorageResult<WalletMetadata> {
        let metadata_path = self.metadata_path(name);
        if !metadata_path.exists() {
            return Err(WalletStorageError::WalletNotFound(name.to_string()));
        }

        let contents = fs::read_to_string(&metadata_path)?;
        let metadata: WalletMetadata = serde_json::from_str(&contents)?;
        Ok(metadata)
    }

    /// Load and decrypt mnemonic
    pub fn load_mnemonic(&self, name: &str) -> WalletStorageResult<String> {
        let metadata = self.load_metadata(name)?;

        let mnemonic_path = self.mnemonic_path(name, metadata.encrypted);
        if !mnemonic_path.exists() {
            return Err(WalletStorageError::WalletNotFound(format!(
                "Mnemonic file not found for wallet: {}",
                name
            )));
        }

        let mnemonic = if metadata.encrypted {
            // Decrypt with GPG
            Gpg::decrypt_file(&mnemonic_path)?
        } else {
            // Read plaintext
            fs::read_to_string(&mnemonic_path)?
        };

        Ok(mnemonic.trim().to_string())
    }

    /// Derive root key from wallet
    pub fn derive_root(&self, name: &str) -> WalletStorageResult<XPrv> {
        let mnemonic_str = self.load_mnemonic(name)?;
        let mnemonic = parse_mnemonic(&mnemonic_str)?;
        let root = derive_root_key(&mnemonic)?;
        Ok(root)
    }

    /// Derive account from wallet
    pub fn derive_account(&self, name: &str, account_index: u32) -> WalletStorageResult<Account> {
        let root = self.derive_root(name)?;
        let account = derive_account(&root, account_index)?;
        Ok(account)
    }

    /// List all wallets
    pub fn list_wallets(&self) -> WalletStorageResult<Vec<WalletMetadata>> {
        let mut wallets = Vec::new();

        if !self.wallet_dir.exists() {
            return Ok(wallets);
        }

        for entry in fs::read_dir(&self.wallet_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if let Ok(metadata) = self.load_metadata(name) {
                        wallets.push(metadata);
                    }
                }
            }
        }

        Ok(wallets)
    }

    /// Delete a wallet
    pub fn delete_wallet(&self, name: &str) -> WalletStorageResult<()> {
        let wallet_path = self.wallet_path(name);
        if !wallet_path.exists() {
            return Err(WalletStorageError::WalletNotFound(name.to_string()));
        }

        fs::remove_dir_all(&wallet_path)?;
        Ok(())
    }

    /// Export wallet (mnemonic plaintext) - WARNING: sensitive operation
    pub fn export_mnemonic(&self, name: &str) -> WalletStorageResult<String> {
        self.load_mnemonic(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    const TEST_MNEMONIC: &str = "test walk nut penalty hip pave soap entry language right filter choice";

    #[test]
    fn test_create_and_load_wallet() {
        let temp_dir = TempDir::new().unwrap();
        let storage = WalletStorage::new(temp_dir.path().to_path_buf()).unwrap();

        // Create wallet without encryption
        let metadata = storage.create_wallet(
            "test-wallet",
            TEST_MNEMONIC,
            Network::Testnet,
            None,
        ).unwrap();

        assert_eq!(metadata.name, "test-wallet");
        assert_eq!(metadata.network, "testnet");
        assert!(!metadata.encrypted);

        // Load metadata
        let loaded_metadata = storage.load_metadata("test-wallet").unwrap();
        assert_eq!(loaded_metadata.name, "test-wallet");

        // Load mnemonic
        let loaded_mnemonic = storage.load_mnemonic("test-wallet").unwrap();
        assert_eq!(loaded_mnemonic, TEST_MNEMONIC);
    }

    #[test]
    fn test_list_wallets() {
        let temp_dir = TempDir::new().unwrap();
        let storage = WalletStorage::new(temp_dir.path().to_path_buf()).unwrap();

        storage.create_wallet("wallet1", TEST_MNEMONIC, Network::Mainnet, None).unwrap();
        storage.create_wallet("wallet2", TEST_MNEMONIC, Network::Testnet, None).unwrap();

        let wallets = storage.list_wallets().unwrap();
        assert_eq!(wallets.len(), 2);
    }

    #[test]
    fn test_delete_wallet() {
        let temp_dir = TempDir::new().unwrap();
        let storage = WalletStorage::new(temp_dir.path().to_path_buf()).unwrap();

        storage.create_wallet("test-wallet", TEST_MNEMONIC, Network::Testnet, None).unwrap();
        assert!(storage.load_metadata("test-wallet").is_ok());

        storage.delete_wallet("test-wallet").unwrap();
        assert!(storage.load_metadata("test-wallet").is_err());
    }

    #[test]
    fn test_invalid_wallet_name() {
        let temp_dir = TempDir::new().unwrap();
        let storage = WalletStorage::new(temp_dir.path().to_path_buf()).unwrap();

        // Invalid characters
        let result = storage.create_wallet("test/wallet", TEST_MNEMONIC, Network::Testnet, None);
        assert!(result.is_err());

        // Empty name
        let result = storage.create_wallet("", TEST_MNEMONIC, Network::Testnet, None);
        assert!(result.is_err());
    }
}
