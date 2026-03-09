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
    client: Option<WalletUtxorpcClient>,
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
            client: Some(client),
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

    /// Create a builder for offline transaction building
    ///
    /// This creates a builder without a network connection. You must provide
    /// protocol parameters and UTxOs manually using `with_protocol_params()`
    /// and `with_utxos()` or `with_utxos_from_file()`.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use std::sync::Arc;
    /// # use hayate::wallet::{Wallet, Network};
    /// # use hayate::wallet::unified_tx::UnifiedTxBuilder;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let wallet = Arc::new(Wallet::from_mnemonic_str("...", Network::Testnet, 0)?);
    ///
    /// let tx = UnifiedTxBuilder::offline(wallet)
    ///     .with_protocol_params_from_file("protocol_params.json")?
    ///     .with_utxos_from_file("utxos.json")?
    ///     .send_ada("addr_test1...", 5_000_000)?
    ///     .build().await?;  // Returns unsigned transaction
    /// # Ok(())
    /// # }
    /// ```
    pub fn offline(wallet: Arc<Wallet>) -> Self {
        let network = wallet.network();

        Self {
            wallet,
            client: None,  // No client in offline mode
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

    /// Set protocol parameters manually
    ///
    /// Use this in offline mode to provide protocol parameters without querying the network.
    pub fn with_protocol_params(&mut self, params: ProtocolParameters) -> &mut Self {
        self.protocol_params = Some(params);
        self
    }

    /// Load protocol parameters from a JSON file
    ///
    /// # Example JSON format
    /// ```json
    /// {
    ///   "min_fee_a": 44,
    ///   "min_fee_b": 155381,
    ///   "max_tx_size": 16384,
    ///   "max_block_body_size": 90112,
    ///   "utxo_cost_per_byte": 4310,
    ///   "key_deposit": 2000000,
    ///   "pool_deposit": 500000000,
    ///   "min_pool_cost": 340000000,
    ///   "epoch": 450
    /// }
    /// ```
    pub fn with_protocol_params_from_file(&mut self, path: &str) -> Result<&mut Self> {
        let json = std::fs::read_to_string(path)
            .map_err(|e| UnifiedTxError::FeeEstimationError(format!("Failed to read protocol params file: {}", e)))?;

        let params: ProtocolParameters = serde_json::from_str(&json)
            .map_err(|e| UnifiedTxError::FeeEstimationError(format!("Failed to parse protocol params JSON: {}", e)))?;

        self.protocol_params = Some(params);
        Ok(self)
    }

    /// Load UTxOs from a JSON file
    ///
    /// # Example JSON format
    /// ```json
    /// [
    ///   {
    ///     "tx_hash": "abcdef...",
    ///     "output_index": 0,
    ///     "address": "00142857...",
    ///     "coin": 10000000,
    ///     "assets": []
    ///   }
    /// ]
    /// ```
    pub fn with_utxos_from_file(&mut self, path: &str) -> Result<&mut Self> {
        let json = std::fs::read_to_string(path)
            .map_err(|_e| UnifiedTxError::NoUtxos)?;

        let utxos: Vec<UtxoData> = serde_json::from_str(&json)
            .map_err(|_e| UnifiedTxError::NoUtxos)?;

        self.available_utxos = Some(utxos);
        Ok(self)
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

        let client = self.client.as_mut()
            .ok_or_else(|| UnifiedTxError::NoUtxos)?;

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
        let utxos = client.query_utxos(addresses).await?;

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

    /// Mint tokens with a Plutus minting policy
    ///
    /// # Arguments
    /// * `policy_script` - The Plutus minting policy script
    /// * `asset_name` - The asset name (token name)
    /// * `amount` - Amount to mint (positive) or burn (negative)
    /// * `redeemer` - Redeemer for the policy script
    pub fn mint_with_policy(
        &mut self,
        policy_script: PlutusScript,
        asset_name: Vec<u8>,
        amount: i64,
        redeemer: Redeemer,
    ) -> Result<&mut Self> {
        let policy_id = policy_script.policy_id();
        self.mints.push(MintOperation {
            policy_id,
            asset_name,
            amount,
            policy_script: Some(policy_script),
            redeemer: Some(redeemer),
            native_script: None,
        });
        Ok(self)
    }

    /// Mint tokens with a native script
    pub fn mint_with_native_script(
        &mut self,
        native_script: Vec<u8>,
        policy_id: [u8; 28],
        asset_name: Vec<u8>,
        amount: i64,
    ) -> Result<&mut Self> {
        self.mints.push(MintOperation {
            policy_id,
            asset_name,
            amount,
            policy_script: None,
            redeemer: None,
            native_script: Some(native_script),
        });
        Ok(self)
    }

    /// Burn tokens (shortcut for negative mint amount)
    pub fn burn(
        &mut self,
        policy_id: [u8; 28],
        asset_name: Vec<u8>,
        amount: i64,
    ) -> Result<&mut Self> {
        self.mints.push(MintOperation {
            policy_id,
            asset_name,
            amount: -amount.abs(),
            policy_script: None,
            redeemer: None,
            native_script: None,
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

    /// Query and cache protocol parameters
    pub async fn query_protocol_params(&mut self) -> Result<&ProtocolParameters> {
        if self.protocol_params.is_none() {
            // Try to query from client if available
            if let Some(ref mut client) = self.client {
                let params = client.query_protocol_params().await?
                    .ok_or_else(|| UnifiedTxError::FeeEstimationError(
                        "Protocol parameters not available from server".to_string()
                    ))?;

                tracing::debug!(
                    "Cached protocol parameters: minFeeA={}, minFeeB={}",
                    params.min_fee_a,
                    params.min_fee_b
                );

                self.protocol_params = Some(params);
            } else {
                return Err(UnifiedTxError::FeeEstimationError(
                    "Protocol parameters not set. Use with_protocol_params() or with_protocol_params_from_file() in offline mode".to_string()
                ));
            }
        }

        Ok(self.protocol_params.as_ref().unwrap())
    }

    /// Estimate fee for a transaction of given size
    fn estimate_fee(&self, tx_size_bytes: u64, params: &ProtocolParameters) -> u64 {
        match self.fee_strategy {
            FeeStrategy::Fixed(fee) => fee,
            FeeStrategy::Automatic => params.calculate_min_fee(tx_size_bytes),
        }
    }

    /// Calculate minimum UTxO value for an output
    fn calculate_min_utxo(&self, output_size_bytes: u64, params: &ProtocolParameters) -> u64 {
        params.calculate_min_utxo(output_size_bytes).max(1_000_000) // At least 1 ADA
    }

    /// Select coins using greedy algorithm
    fn select_coins(
        &self,
        required_lovelace: u64,
        required_assets: &BTreeMap<String, u64>,
    ) -> Result<(Vec<UtxoData>, u64, BTreeMap<String, u64>)> {
        let utxos = self.available_utxos.as_ref()
            .ok_or(UnifiedTxError::NoUtxos)?;

        let mut selected = Vec::new();
        let mut total_lovelace = 0u64;
        let mut total_assets: BTreeMap<String, u64> = BTreeMap::new();

        // Greedy selection: take UTxOs until requirements met
        for utxo in utxos {
            // Skip UTxOs already used as script inputs
            if self.script_inputs.iter().any(|si| {
                si.utxo.tx_hash == utxo.tx_hash && si.utxo.output_index == utxo.output_index
            }) {
                continue;
            }

            selected.push(utxo.clone());
            total_lovelace = total_lovelace.saturating_add(utxo.coin);

            // Add asset amounts
            for asset in &utxo.assets {
                let key = format!(
                    "{}:{}",
                    hex::encode(&asset.policy_id),
                    hex::encode(&asset.asset_name)
                );
                *total_assets.entry(key).or_insert(0) = total_assets
                    .get(&key)
                    .unwrap_or(&0)
                    .saturating_add(asset.amount);
            }

            // Check if requirements met
            if total_lovelace >= required_lovelace {
                let mut all_assets_sufficient = true;
                for (asset_key, needed) in required_assets {
                    if total_assets.get(asset_key).unwrap_or(&0) < needed {
                        all_assets_sufficient = false;
                        break;
                    }
                }

                if all_assets_sufficient {
                    break;
                }
            }
        }

        // Validate sufficient funds
        if total_lovelace < required_lovelace {
            return Err(UnifiedTxError::InsufficientFunds {
                need: required_lovelace,
                available: total_lovelace,
            });
        }

        // Validate sufficient assets
        for (asset_key, needed) in required_assets {
            let available = total_assets.get(asset_key).unwrap_or(&0);
            if available < needed {
                return Err(UnifiedTxError::InsufficientAssets {
                    asset: asset_key.clone(),
                    need: *needed,
                    available: *available,
                });
            }
        }

        // Calculate change
        let change_lovelace = total_lovelace.saturating_sub(required_lovelace);
        let mut change_assets = BTreeMap::new();
        for (key, total) in total_assets {
            let needed = required_assets.get(&key).unwrap_or(&0);
            if total > *needed {
                change_assets.insert(key, total - needed);
            }
        }

        Ok((selected, change_lovelace, change_assets))
    }

    /// Build the transaction
    ///
    /// This performs the full transaction building process:
    /// 1. Query protocol parameters
    /// 2. Calculate required outputs
    /// 3. Select UTxOs
    /// 4. Calculate fees
    /// 5. Add change output if needed
    /// 6. Build transaction with pallas
    ///
    /// Returns an unsigned transaction ready for signing.
    pub async fn build(&mut self) -> Result<BuiltTransaction> {
        // Validate we have outputs
        if self.outputs.is_empty() && self.mints.is_empty() {
            return Err(UnifiedTxError::NoOutputs);
        }

        // Query protocol parameters
        let params = self.query_protocol_params().await?.clone();

        // Calculate total output requirements
        let mut total_output_lovelace = 0u64;
        let mut required_assets: BTreeMap<String, u64> = BTreeMap::new();

        for output in &self.outputs {
            match output {
                TxOutput::Payment { lovelace, assets, .. } |
                TxOutput::ScriptPayment { lovelace, assets, .. } => {
                    total_output_lovelace = total_output_lovelace.saturating_add(*lovelace);
                    for asset in assets {
                        let key = format!(
                            "{}:{}",
                            hex::encode(&asset.policy_id),
                            hex::encode(&asset.asset_name)
                        );
                        *required_assets.entry(key).or_insert(0) =
                            required_assets.get(&key).unwrap_or(&0).saturating_add(asset.amount);
                    }
                }
            }
        }

        // Estimate transaction size (rough estimate)
        let base_size = 200u64; // Base transaction overhead
        let input_size = 150u64; // Approximate bytes per input
        let output_size = 50u64; // Approximate bytes per output
        let estimated_inputs = ((total_output_lovelace / 10_000_000) + 1).min(10); // Rough estimate
        let estimated_tx_size = base_size + (estimated_inputs * input_size) +
            ((self.outputs.len() as u64 + 1) * output_size); // +1 for change

        // Estimate fee
        let estimated_fee = self.estimate_fee(estimated_tx_size, &params);

        // Add fee to required lovelace
        let required_lovelace_with_fee = total_output_lovelace.saturating_add(estimated_fee);

        tracing::debug!(
            "Building tx: outputs={} lovelace, fee={} lovelace, total_required={} lovelace",
            total_output_lovelace,
            estimated_fee,
            required_lovelace_with_fee
        );

        // Select coins
        let (selected_utxos, change_lovelace, change_assets) =
            self.select_coins(required_lovelace_with_fee, &required_assets)?;

        tracing::info!(
            "Selected {} UTxOs, change={} lovelace",
            selected_utxos.len(),
            change_lovelace
        );

        // Build using PlutusTransactionBuilder
        use crate::wallet::tx_builder::PlutusTransactionBuilder;
        use crate::wallet::tx_builder::PlutusInput;
        use crate::wallet::plutus::Network as PlutusNetwork;

        let network = match self.network {
            Network::Mainnet => PlutusNetwork::Mainnet,
            Network::Testnet => PlutusNetwork::Testnet,
        };

        // Get change address
        let change_addr_bytes = self.wallet.payment_address_bytes(self.change_address_index)?;

        let mut builder = PlutusTransactionBuilder::new(network, change_addr_bytes);

        // Add selected UTxOs as inputs
        for utxo in &selected_utxos {
            let input = PlutusInput::regular(utxo.clone());
            builder.add_input(&input)?;
        }

        // Add script inputs
        for script_input in &self.script_inputs {
            let input = PlutusInput::script(
                script_input.utxo.clone(),
                script_input.script.clone(),
                script_input.redeemer.clone(),
                script_input.datum.clone(),
            );
            builder.add_input(&input)?;
        }

        // Add outputs
        use crate::wallet::tx_builder::PlutusOutput;

        for output in &self.outputs {
            match output {
                TxOutput::Payment { address, lovelace, assets } => {
                    let plutus_output = if assets.is_empty() {
                        PlutusOutput::new(address.clone(), *lovelace)
                    } else {
                        PlutusOutput::with_assets(address.clone(), *lovelace, assets.clone())
                    };
                    builder.add_output(&plutus_output)?;
                }
                TxOutput::ScriptPayment { address, lovelace, assets, datum, script_ref } => {
                    let mut plutus_output = if assets.is_empty() {
                        PlutusOutput::new(address.clone(), *lovelace)
                    } else {
                        PlutusOutput::with_assets(address.clone(), *lovelace, assets.clone())
                    };

                    plutus_output = plutus_output.with_datum(datum.clone());

                    if let Some(script) = script_ref {
                        plutus_output = plutus_output.with_script_ref(script.clone());
                    }

                    builder.add_output(&plutus_output)?;
                }
            }
        }

        // Add change output if significant
        let min_change = self.calculate_min_utxo(50, &params); // ~50 bytes for simple output
        if change_lovelace >= min_change {
            let mut change_output = PlutusOutput::new(
                self.wallet.payment_address_bytes(self.change_address_index)?,
                change_lovelace,
            );

            // Add change assets if any
            if !change_assets.is_empty() {
                let change_asset_data: Vec<AssetData> = change_assets
                    .iter()
                    .filter_map(|(key, amount)| {
                        let parts: Vec<&str> = key.split(':').collect();
                        if parts.len() == 2 {
                            Some(AssetData {
                                policy_id: hex::decode(parts[0]).ok()?,
                                asset_name: hex::decode(parts[1]).ok()?,
                                amount: *amount,
                            })
                        } else {
                            None
                        }
                    })
                    .collect();

                change_output = PlutusOutput::with_assets(
                    self.wallet.payment_address_bytes(self.change_address_index)?,
                    change_lovelace,
                    change_asset_data,
                );
            }

            builder.add_output(&change_output)?;
        } else if change_lovelace > 0 {
            tracing::warn!(
                "Change {} lovelace below minimum {}, adding to fee",
                change_lovelace,
                min_change
            );
        }

        // Add collateral if we have script inputs
        if !self.script_inputs.is_empty() || !self.mints.is_empty() {
            if let Some(ref collateral) = self.collateral {
                for utxo in collateral {
                    builder.add_collateral(utxo)?;
                }
            } else {
                return Err(UnifiedTxError::CollateralRequired);
            }
        }

        // Add mints
        for mint in &self.mints {
            builder.mint_asset(mint.policy_id, mint.asset_name.clone(), mint.amount)?;

            if let Some(ref _script) = mint.policy_script {
                // Add script and redeemer for Plutus policies
                if let Some(ref redeemer) = mint.redeemer {
                    builder.add_mint_redeemer(mint.policy_id, redeemer)?;
                }
            } else if let Some(ref native_script) = mint.native_script {
                builder.add_native_script(native_script.clone())?;
            }
        }

        // Set transaction parameters
        builder.set_fee(estimated_fee).set_network_id();

        if let Some(ttl) = self.ttl {
            builder.set_ttl(ttl);
        }

        if let Some(validity_start) = self.validity_start {
            builder.set_validity_start(validity_start);
        }

        // Set language view if we have Plutus scripts
        if !self.script_inputs.is_empty() || self.mints.iter().any(|m| m.policy_script.is_some()) {
            use crate::wallet::plutus::PlutusVersion;
            builder.set_default_language_view(PlutusVersion::V2);
        }

        // Build the transaction
        let (tx_bytes, tx_hash) = builder.build()?;

        tracing::info!(
            "Built transaction: {} bytes, hash={}",
            tx_bytes.len(),
            hex::encode(&tx_hash)
        );

        Ok(BuiltTransaction {
            tx_bytes,
            tx_hash,
            fee_paid: estimated_fee,
            inputs_used: selected_utxos,
            change_amount: if change_lovelace >= min_change {
                change_lovelace
            } else {
                0
            },
            output_count: self.outputs.len() + if change_lovelace >= min_change { 1 } else { 0 },
        })
    }

    /// Sign a pre-built transaction (for airgap workflow)
    ///
    /// This method signs an unsigned transaction that was built on another machine,
    /// enabling air-gapped signing for maximum security.
    ///
    /// # Airgap Workflow
    ///
    /// **On connected machine:**
    /// 1. Build transaction: `let built_tx = builder.build().await?;`
    /// 2. Save: `built_tx.save_to_file("unsigned_tx.json")?;`
    /// 3. Transfer unsigned_tx.json to air-gapped machine
    ///
    /// **On air-gapped machine:**
    /// 1. Load: `let built_tx = BuiltTransaction::load_from_file("unsigned_tx.json")?;`
    /// 2. Sign: `let signed = UnifiedTxBuilder::sign_transaction(&built_tx, wallet).await?;`
    /// 3. Save: `std::fs::write("signed_tx.cbor", signed)?;`
    /// 4. Transfer signed_tx.cbor back to connected machine
    ///
    /// **Back on connected machine:**
    /// 1. Submit: `client.submit_transaction(signed_tx).await?;`
    ///
    /// # Arguments
    /// * `built_tx` - The BuiltTransaction loaded from JSON
    /// * `wallet` - Wallet for signing (must have the private keys for the inputs)
    ///
    /// # Returns
    /// Signed transaction CBOR bytes ready for submission
    pub async fn sign_transaction(_built_tx: &BuiltTransaction, _wallet: Arc<Wallet>) -> Result<Vec<u8>> {
        tracing::info!("Signing pre-built transaction on air-gapped machine");

        // For airgap signing, we need to rebuild the transaction with the same parameters
        // and sign it. This is necessary because the unsigned tx bytes don't contain
        // enough information to sign directly without rebuilding.
        //
        // TODO: In the future, we could save more state in BuiltTransaction to enable
        // true separate signing without rebuilding.

        tracing::warn!("Airgap signing requires rebuilding the transaction. Ensure protocol params and UTxOs match the original build.");
        tracing::warn!("For now, use build_and_sign() on the air-gapped machine with the same inputs.");

        // Placeholder implementation - in practice, users should:
        // 1. Transfer protocol_params.json and utxos.json to airgap machine
        // 2. Build transaction offline on airgap machine
        // 3. Sign using build_and_sign()
        // This ensures everything is consistent

        Err(UnifiedTxError::BuilderError(
            crate::wallet::tx_builder::TxBuilderError::BuildError(
                "Separate airgap signing not yet fully implemented. Use offline building on airgap machine instead: \
                UnifiedTxBuilder::offline(wallet).with_protocol_params_from_file(...).with_utxos_from_file(...).build_and_sign()".to_string()
            )
        ))
    }

    /// Build and sign the transaction
    ///
    /// Returns signed transaction bytes ready for submission.
    pub async fn build_and_sign(&mut self) -> Result<Vec<u8>> {
        let built_tx = self.build().await?;

        tracing::debug!("Determining which address indices were used");

        // Determine which address indices were used by matching UTxO addresses
        use std::collections::HashSet;
        let mut used_indices = HashSet::new();

        for utxo in &built_tx.inputs_used {
            // Try to match against payment addresses
            for i in 0..self.address_scan_limit {
                let payment_addr = self.wallet.payment_address_bytes(i)?;
                if utxo.address == payment_addr {
                    used_indices.insert(i);
                    break;
                }

                // Also try enterprise addresses
                let enterprise_addr = self.wallet.enterprise_address_bytes(i)?;
                if utxo.address == enterprise_addr {
                    used_indices.insert(i);
                    break;
                }
            }
        }

        tracing::info!("Signing transaction with keys from {} addresses", used_indices.len());

        // Get signing keys for the used indices
        let mut signing_keys = Vec::new();
        for index in used_indices {
            signing_keys.push(self.wallet.payment_signing_key(index)?);
        }

        // Rebuild the transaction with signing
        use crate::wallet::tx_builder::PlutusTransactionBuilder;
        use crate::wallet::tx_builder::PlutusInput;
        use crate::wallet::plutus::Network as PlutusNetwork;
        use crate::wallet::tx_builder::PlutusOutput;

        let network = match self.network {
            Network::Mainnet => PlutusNetwork::Mainnet,
            Network::Testnet => PlutusNetwork::Testnet,
        };

        let change_addr_bytes = self.wallet.payment_address_bytes(self.change_address_index)?;
        let mut builder = PlutusTransactionBuilder::new(network, change_addr_bytes);

        // Re-add all inputs
        for utxo in &built_tx.inputs_used {
            let input = PlutusInput::regular(utxo.clone());
            builder.add_input(&input)?;
        }

        for script_input in &self.script_inputs {
            let input = PlutusInput::script(
                script_input.utxo.clone(),
                script_input.script.clone(),
                script_input.redeemer.clone(),
                script_input.datum.clone(),
            );
            builder.add_input(&input)?;
        }

        // Re-add all outputs (including change if it was added)
        for output in &self.outputs {
            match output {
                TxOutput::Payment { address, lovelace, assets } => {
                    let plutus_output = if assets.is_empty() {
                        PlutusOutput::new(address.clone(), *lovelace)
                    } else {
                        PlutusOutput::with_assets(address.clone(), *lovelace, assets.clone())
                    };
                    builder.add_output(&plutus_output)?;
                }
                TxOutput::ScriptPayment { address, lovelace, assets, datum, script_ref } => {
                    let mut plutus_output = if assets.is_empty() {
                        PlutusOutput::new(address.clone(), *lovelace)
                    } else {
                        PlutusOutput::with_assets(address.clone(), *lovelace, assets.clone())
                    };

                    plutus_output = plutus_output.with_datum(datum.clone());

                    if let Some(script) = script_ref {
                        plutus_output = plutus_output.with_script_ref(script.clone());
                    }

                    builder.add_output(&plutus_output)?;
                }
            }
        }

        // Add change if it was included
        if built_tx.change_amount > 0 {
            let change_output = PlutusOutput::new(
                self.wallet.payment_address_bytes(self.change_address_index)?,
                built_tx.change_amount,
            );
            builder.add_output(&change_output)?;
        }

        // Re-add collateral if present
        if !self.script_inputs.is_empty() || !self.mints.is_empty() {
            if let Some(ref collateral) = self.collateral {
                for utxo in collateral {
                    builder.add_collateral(utxo)?;
                }
            }
        }

        // Re-add mints
        for mint in &self.mints {
            builder.mint_asset(mint.policy_id, mint.asset_name.clone(), mint.amount)?;

            if let Some(ref _script) = mint.policy_script {
                if let Some(ref redeemer) = mint.redeemer {
                    builder.add_mint_redeemer(mint.policy_id, redeemer)?;
                }
            } else if let Some(ref native_script) = mint.native_script {
                builder.add_native_script(native_script.clone())?;
            }
        }

        // Set transaction parameters
        builder.set_fee(built_tx.fee_paid).set_network_id();

        if let Some(ttl) = self.ttl {
            builder.set_ttl(ttl);
        }

        if let Some(validity_start) = self.validity_start {
            builder.set_validity_start(validity_start);
        }

        // Set language view if we have Plutus scripts
        if !self.script_inputs.is_empty() || self.mints.iter().any(|m| m.policy_script.is_some()) {
            use crate::wallet::plutus::PlutusVersion;
            builder.set_default_language_view(PlutusVersion::V2);
        }

        // Build and sign
        let signed_tx = builder.build_and_sign(signing_keys)?;

        tracing::info!("Transaction signed successfully");

        Ok(signed_tx)
    }

    /// Build, sign, and submit the transaction
    ///
    /// Returns the transaction hash.
    ///
    /// Note: Requires a network connection. Not available in offline mode.
    pub async fn build_sign_submit(&mut self) -> Result<Vec<u8>> {
        let signed_tx = self.build_and_sign().await?;

        tracing::info!("Submitting transaction ({} bytes)", signed_tx.len());

        let client = self.client.as_mut()
            .ok_or_else(|| UnifiedTxError::UtxorpcError(anyhow::anyhow!(
                "Cannot submit transaction in offline mode. Use build_and_sign() instead."
            )))?;

        let response = client.submit_transaction(signed_tx).await?;

        tracing::info!("Transaction submitted successfully");

        Ok(response.tx_hash)
    }
}
