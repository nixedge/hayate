// Unified Transaction Builder Implementation

use super::types::*;
use super::error::{Result, UnifiedTxError};
use crate::wallet::{Wallet, derivation::Network};
use crate::wallet::utxorpc_client::{WalletUtxorpcClient, UtxoData, AssetData};
use crate::wallet::plutus::{DatumOption, PlutusScript, Redeemer};
use crate::protocol_params::ProtocolParameters;
use std::sync::Arc;
use std::collections::BTreeMap;

/// Unified transaction builder
///
/// Provides a high-level API for building Cardano transactions with automatic
/// UTxO selection, fee calculation, and change management.
///
/// # Example
///
/// ```no_run
/// # use std::sync::Arc;
/// # use hayate::wallet::{Wallet, derivation::Network};
/// # use hayate::wallet::unified_tx::UnifiedTxBuilder;
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let wallet = Arc::new(Wallet::from_mnemonic_str("...", Network::Testnet, 0)?);
///
/// let tx_hash = UnifiedTxBuilder::new(wallet, "http://localhost:50051").await?
///     .query_utxos().await?
///     .send_ada("addr_test1...", 5_000_000)?
///     .build_sign_submit().await?;
/// # Ok(())
/// # }
/// ```
pub struct UnifiedTxBuilder {
    wallet: Arc<Wallet>,
    client: WalletUtxorpcClient,
    network: Network,

    // Builder state
    available_utxos: Option<Vec<UtxoData>>,
    outputs: Vec<TxOutput>,
    mints: Vec<MintOperation>,
    script_inputs: Vec<ScriptInputSpec>,
    collateral: Option<Vec<UtxoData>>,

    // Configuration
    fee_strategy: FeeStrategy,
    address_scan_limit: u32,
    change_address_index: u32,
    ttl: Option<u64>,
    validity_start: Option<u64>,

    // Cached protocol parameters
    protocol_params: Option<ProtocolParameters>,
}

impl UnifiedTxBuilder {
    /// Create a new unified transaction builder
    ///
    /// # Arguments
    /// * `wallet` - Wallet instance for key derivation and signing
    /// * `endpoint` - UTxORPC endpoint URL (e.g., "http://localhost:50051")
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use std::sync::Arc;
    /// # use hayate::wallet::{Wallet, derivation::Network};
    /// # use hayate::wallet::unified_tx::UnifiedTxBuilder;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let wallet = Arc::new(Wallet::from_mnemonic_str("...", Network::Testnet, 0)?);
    /// let builder = UnifiedTxBuilder::new(wallet, "http://localhost:50051").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn new(wallet: Arc<Wallet>, endpoint: impl Into<String>) -> Result<Self> {
        let client = WalletUtxorpcClient::connect(endpoint.into()).await?;
        Ok(Self::with_client(wallet, client))
    }

    /// Create a new builder with an existing client
    ///
    /// Use this when you want to reuse a client connection.
    ///
    /// # Arguments
    /// * `wallet` - Wallet instance for key derivation and signing
    /// * `client` - Existing WalletUtxorpcClient
    pub fn with_client(wallet: Arc<Wallet>, client: WalletUtxorpcClient) -> Self {
        let network = wallet.network();

        Self {
            wallet,
            client,
            network,
            available_utxos: None,
            outputs: Vec::new(),
            mints: Vec::new(),
            script_inputs: Vec::new(),
            collateral: None,
            fee_strategy: FeeStrategy::default(),
            address_scan_limit: 20,
            change_address_index: 0,
            ttl: None,
            validity_start: None,
            protocol_params: None,
        }
    }

    /// Set the number of addresses to scan for UTxOs
    ///
    /// Default: 20 addresses (both payment and enterprise)
    pub fn address_scan_limit(&mut self, limit: u32) -> &mut Self {
        self.address_scan_limit = limit;
        self
    }

    /// Set the change address index
    ///
    /// Default: 0 (first address)
    pub fn change_address(&mut self, index: u32) -> &mut Self {
        self.change_address_index = index;
        self
    }

    /// Set fee strategy
    pub fn fee_strategy(&mut self, strategy: FeeStrategy) -> &mut Self {
        self.fee_strategy = strategy;
        self
    }

    /// Set fixed fee (shortcut for FeeStrategy::Fixed)
    pub fn fee(&mut self, lovelace: u64) -> &mut Self {
        self.fee_strategy = FeeStrategy::Fixed(lovelace);
        self
    }

    /// Set transaction TTL (time to live) in slots
    pub fn ttl(&mut self, slot: u64) -> &mut Self {
        self.ttl = Some(slot);
        self
    }

    /// Set validity start slot
    pub fn validity_start(&mut self, slot: u64) -> &mut Self {
        self.validity_start = Some(slot);
        self
    }

    /// Query UTxOs from the network
    ///
    /// Derives addresses 0..address_scan_limit (default 20) and queries
    /// both payment and enterprise addresses.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use std::sync::Arc;
    /// # use hayate::wallet::{Wallet, derivation::Network};
    /// # use hayate::wallet::unified_tx::UnifiedTxBuilder;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let wallet = Arc::new(Wallet::from_mnemonic_str("...", Network::Testnet, 0)?);
    /// # let mut builder = UnifiedTxBuilder::new(wallet, "http://localhost:50051").await?;
    /// builder.query_utxos().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn query_utxos(&mut self) -> Result<&mut Self> {
        use pallas_addresses::Address;

        let mut addresses = Vec::new();

        // Derive payment addresses (with stake component)
        for i in 0..self.address_scan_limit {
            let addr_str = self.wallet.payment_address(i)?;
            let addr = Address::from_bech32(&addr_str)
                .map_err(|e| UnifiedTxError::InvalidBech32(format!("{}", e)))?;
            addresses.push(addr.to_vec());
        }

        // Derive enterprise addresses (without stake component)
        for i in 0..self.address_scan_limit {
            let addr_str = self.wallet.enterprise_address(i)?;
            let addr = Address::from_bech32(&addr_str)
                .map_err(|e| UnifiedTxError::InvalidBech32(format!("{}", e)))?;
            addresses.push(addr.to_vec());
        }

        tracing::debug!(
            "Querying UTxOs for {} addresses ({} payment + {} enterprise)",
            addresses.len(),
            self.address_scan_limit,
            self.address_scan_limit
        );

        // Query all addresses in one call
        let utxos = self.client.query_utxos(addresses).await?;

        tracing::info!("Found {} UTxOs", utxos.len());

        self.available_utxos = Some(utxos);
        Ok(self)
    }

    /// Manually provide UTxOs (for offline signing)
    ///
    /// Use this instead of `query_utxos()` when you want to provide
    /// UTxOs manually, e.g., for offline transaction building.
    pub fn with_utxos(&mut self, utxos: Vec<UtxoData>) -> &mut Self {
        self.available_utxos = Some(utxos);
        self
    }

    /// Send ADA to a bech32 address
    ///
    /// # Arguments
    /// * `recipient` - Bech32-encoded address (e.g., "addr_test1...")
    /// * `lovelace` - Amount in lovelace (1 ADA = 1,000,000 lovelace)
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use std::sync::Arc;
    /// # use hayate::wallet::{Wallet, derivation::Network};
    /// # use hayate::wallet::unified_tx::UnifiedTxBuilder;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let wallet = Arc::new(Wallet::from_mnemonic_str("...", Network::Testnet, 0)?);
    /// # let mut builder = UnifiedTxBuilder::new(wallet, "http://localhost:50051").await?;
    /// builder.send_ada("addr_test1...", 5_000_000)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn send_ada(&mut self, recipient: &str, lovelace: u64) -> Result<&mut Self> {
        use pallas_addresses::Address;

        let addr = Address::from_bech32(recipient)
            .map_err(|e| UnifiedTxError::InvalidBech32(format!("{}", e)))?;

        self.send_ada_to(addr.to_vec(), lovelace)
    }

    /// Send ADA to an address (raw bytes)
    ///
    /// Use this when you already have the address as bytes.
    pub fn send_ada_to(&mut self, address: Vec<u8>, lovelace: u64) -> Result<&mut Self> {
        self.outputs.push(TxOutput::Payment {
            address,
            lovelace,
            assets: Vec::new(),
        });
        Ok(self)
    }

    /// Send ADA and native assets to an address
    ///
    /// # Arguments
    /// * `recipient` - Bech32-encoded address
    /// * `lovelace` - Amount of ADA in lovelace
    /// * `assets` - Native assets to send
    pub fn send_assets(
        &mut self,
        recipient: &str,
        lovelace: u64,
        assets: Vec<AssetData>,
    ) -> Result<&mut Self> {
        use pallas_addresses::Address;

        let addr = Address::from_bech32(recipient)
            .map_err(|e| UnifiedTxError::InvalidBech32(format!("{}", e)))?;

        self.outputs.push(TxOutput::Payment {
            address: addr.to_vec(),
            lovelace,
            assets,
        });

        Ok(self)
    }

    /// Pay to a script address with datum
    pub fn pay_to_script(
        &mut self,
        script_address: Vec<u8>,
        lovelace: u64,
        datum: DatumOption,
    ) -> Result<&mut Self> {
        self.outputs.push(TxOutput::ScriptPayment {
            address: script_address,
            lovelace,
            assets: Vec::new(),
            datum,
            script_ref: None,
        });
        Ok(self)
    }

    /// Pay to script with assets and optional script reference
    pub fn pay_to_script_with_assets(
        &mut self,
        script_address: Vec<u8>,
        lovelace: u64,
        assets: Vec<AssetData>,
        datum: DatumOption,
        script_ref: Option<PlutusScript>,
    ) -> Result<&mut Self> {
        self.outputs.push(TxOutput::ScriptPayment {
            address: script_address,
            lovelace,
            assets,
            datum,
            script_ref,
        });
        Ok(self)
    }

    /// Spend from a script with redeemer
    pub fn spend_from_script(
        &mut self,
        utxo: UtxoData,
        script: PlutusScript,
        redeemer: Redeemer,
        datum: Option<Vec<u8>>,
    ) -> Result<&mut Self> {
        self.script_inputs.push(ScriptInputSpec {
            utxo,
            script,
            redeemer,
            datum,
        });
        Ok(self)
    }

    /// Manually set collateral
    pub fn with_collateral(&mut self, utxos: Vec<UtxoData>) -> &mut Self {
        self.collateral = Some(utxos);
        self
    }

    /// Automatically select collateral from available UTxOs
    ///
    /// Finds pure ADA UTxOs (no native assets) suitable for collateral.
    pub async fn auto_collateral(&mut self) -> Result<&mut Self> {
        // If collateral already set, return
        if self.collateral.is_some() {
            return Ok(self);
        }

        // Need available_utxos to select from
        let utxos = self.available_utxos.as_ref()
            .ok_or(UnifiedTxError::NoUtxos)?;

        // Find pure ADA UTxOs (no assets) suitable for collateral
        // Typically 5 ADA is enough
        let collateral_amount = 5_000_000u64; // 5 ADA

        let mut collateral_utxos = Vec::new();
        let mut total = 0u64;

        for utxo in utxos {
            // Only use pure ADA UTxOs
            if utxo.assets.is_empty() && utxo.coin >= 2_000_000 {
                collateral_utxos.push(utxo.clone());
                total += utxo.coin;

                if total >= collateral_amount {
                    break;
                }
            }
        }

        if total < collateral_amount {
            return Err(UnifiedTxError::InsufficientFunds {
                need: collateral_amount,
                available: total,
            });
        }

        self.collateral = Some(collateral_utxos);
        Ok(self)
    }

    // More methods will be added in continuation...
}
