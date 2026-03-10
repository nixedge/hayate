// Plutus transaction builder wrapping pallas-txbuilder

use crate::wallet::plutus::{Network, PlutusScript, PlutusVersion, Redeemer};
use crate::wallet::tx_builder::{PlutusInput, PlutusOutput, TxBuilderError, TxBuilderResult};
use pallas_addresses::Address as PallasAddress;
use pallas_crypto::hash::Hash;
use pallas_txbuilder::{BuildConway, Input, Output, ScriptKind, StagingTransaction};

/// Transaction builder for Plutus script transactions
///
/// This wraps pallas-txbuilder's StagingTransaction to provide a higher-level
/// API for building Plutus script transactions with redeemers, datums, and collateral.
///
/// # Example
///
/// ```no_run
/// use hayate::wallet::plutus::{
///     Network, PlutusScript, PlutusVersion, Redeemer, RedeemerTag, DatumOption
/// };
/// use hayate::wallet::tx_builder::{PlutusTransactionBuilder, PlutusInput, PlutusOutput};
/// use hayate::wallet::utxorpc_client::UtxoData;
///
/// // Create a builder for testnet
/// let mut builder = PlutusTransactionBuilder::new(
///     Network::Testnet,
///     vec![0x00; 57] // change address
/// );
///
/// // Add a script input with redeemer
/// let utxo = UtxoData {
///     tx_hash: vec![0u8; 32],
///     output_index: 0,
///     address: vec![0x71; 29], // script address
///     coin: 10_000_000,
///     assets: Vec::new(),
///     datum_hash: None,
///     datum: Some(vec![1, 2, 3]), // inline datum
/// };
///
/// let script = PlutusScript::v2_from_cbor(vec![/* script bytes */]).unwrap();
/// let redeemer = Redeemer::empty(RedeemerTag::Spend, 0);
///
/// builder.add_script_input(&utxo, script, redeemer, None).unwrap();
///
/// // Add an output with inline datum
/// let output = PlutusOutput::with_inline_datum(
///     vec![0x71; 29], // recipient script address
///     5_000_000,
///     vec![4, 5, 6] // datum bytes
/// );
/// builder.add_output(&output).unwrap();
///
/// // Add collateral (required for script transactions)
/// let collateral = UtxoData {
///     tx_hash: vec![0u8; 32],
///     output_index: 1,
///     address: vec![0x00; 57],
///     coin: 5_000_000,
///     assets: Vec::new(),
///     datum_hash: None,
///     datum: None,
/// };
/// builder.add_collateral(&collateral).unwrap();
///
/// // Set transaction parameters
/// builder.set_fee(200_000)
///     .set_ttl(1000)
///     .set_network_id()
///     .set_default_language_view(PlutusVersion::V2);
///
/// // Build the transaction
/// let (tx_bytes, tx_hash) = builder.build().unwrap();
/// ```
pub struct PlutusTransactionBuilder {
    network: Network,
    staging_tx: StagingTransaction,
    _change_address: Vec<u8>,
}

impl PlutusTransactionBuilder {
    /// Create a new Plutus transaction builder
    pub fn new(network: Network, change_address: Vec<u8>) -> Self {
        Self {
            network,
            staging_tx: StagingTransaction::new(),
            _change_address: change_address,
        }
    }

    /// Add a regular input (vkey witness)
    pub fn add_input(&mut self, utxo: &PlutusInput) -> TxBuilderResult<&mut Self> {
        let utxo_data = utxo.utxo();

        // Convert tx_hash to Hash<32>
        let tx_hash: [u8; 32] = utxo_data
            .tx_hash
            .as_slice()
            .try_into()
            .map_err(|_| TxBuilderError::InvalidInput("Invalid tx_hash length".to_string()))?;

        let input = Input::new(Hash::from(tx_hash), utxo_data.output_index as u64);

        self.staging_tx = self.staging_tx.clone().input(input.clone());

        // If this is a script input, add the script, redeemer, and datum
        if let PlutusInput::Script {
            script,
            redeemer,
            datum,
            ..
        } = utxo
        {
            // Add script to witness set
            let script_kind = match script.version() {
                PlutusVersion::V1 => ScriptKind::PlutusV1,
                PlutusVersion::V2 => ScriptKind::PlutusV2,
                PlutusVersion::V3 => ScriptKind::PlutusV3,
            };

            self.staging_tx = self
                .staging_tx
                .clone()
                .script(script_kind, script.cbor().to_vec());

            // Add redeemer
            self.staging_tx = self.staging_tx.clone().add_spend_redeemer(
                input,
                redeemer.data_bytes().to_vec(),
                Some(pallas_txbuilder::ExUnits {
                    mem: redeemer.ex_units.mem,
                    steps: redeemer.ex_units.steps,
                }),
            );

            // Add datum if not inline
            if let Some(datum_bytes) = datum {
                self.staging_tx = self.staging_tx.clone().datum(datum_bytes.clone());
            }
        }

        Ok(self)
    }

    /// Add a script input with redeemer (convenience method)
    pub fn add_script_input(
        &mut self,
        utxo_data: &crate::wallet::utxorpc_client::UtxoData,
        script: PlutusScript,
        redeemer: Redeemer,
        datum: Option<Vec<u8>>,
    ) -> TxBuilderResult<&mut Self> {
        let input = PlutusInput::script(utxo_data.clone(), script, redeemer, datum);
        self.add_input(&input)
    }

    /// Add an output
    pub fn add_output(&mut self, output: &PlutusOutput) -> TxBuilderResult<&mut Self> {
        // Parse address
        let pallas_addr = PallasAddress::from_bytes(&output.address)
            .map_err(|e| TxBuilderError::InvalidOutput(format!("Invalid address: {}", e)))?;

        let mut pallas_output = Output::new(pallas_addr, output.lovelace);

        // Add assets if any
        for asset in &output.assets {
            let policy_id: [u8; 28] = asset
                .policy_id
                .as_slice()
                .try_into()
                .map_err(|_| TxBuilderError::InvalidOutput("Invalid policy_id length".to_string()))?;

            pallas_output = pallas_output
                .add_asset(Hash::from(policy_id), asset.asset_name.clone(), asset.amount)
                .map_err(|e| TxBuilderError::BuildError(e.to_string()))?;
        }

        // Add datum if present
        if let Some(ref datum) = output.datum {
            match datum {
                crate::wallet::plutus::DatumOption::Inline(bytes) => {
                    pallas_output = pallas_output.set_inline_datum(bytes.clone());
                }
                crate::wallet::plutus::DatumOption::Hash(hash_bytes) => {
                    let datum_hash: [u8; 32] = hash_bytes.as_slice().try_into().map_err(|_| {
                        TxBuilderError::InvalidOutput("Invalid datum hash length".to_string())
                    })?;

                    pallas_output = pallas_output.set_datum_hash(Hash::from(datum_hash));

                    // Add the datum bytes to witness set for hash references
                    // Note: The actual datum bytes should be provided separately
                }
            }
        }

        // Add script reference if present
        if let Some(ref script) = output.script_ref {
            let script_kind = match script.version() {
                PlutusVersion::V1 => ScriptKind::PlutusV1,
                PlutusVersion::V2 => ScriptKind::PlutusV2,
                PlutusVersion::V3 => ScriptKind::PlutusV3,
            };

            pallas_output = pallas_output.set_inline_script(script_kind, script.cbor().to_vec());
        }

        self.staging_tx = self.staging_tx.clone().output(pallas_output);

        Ok(self)
    }

    /// Add collateral input (required for Plutus transactions)
    pub fn add_collateral(
        &mut self,
        utxo: &crate::wallet::utxorpc_client::UtxoData,
    ) -> TxBuilderResult<&mut Self> {
        let tx_hash: [u8; 32] = utxo
            .tx_hash
            .as_slice()
            .try_into()
            .map_err(|_| TxBuilderError::InvalidInput("Invalid collateral tx_hash".to_string()))?;

        let input = Input::new(Hash::from(tx_hash), utxo.output_index as u64);

        self.staging_tx = self.staging_tx.clone().collateral_input(input);

        Ok(self)
    }

    /// Set transaction fee
    pub fn set_fee(&mut self, fee: u64) -> &mut Self {
        self.staging_tx = self.staging_tx.clone().fee(fee);
        self
    }

    /// Set TTL (invalid_from_slot in Conway)
    pub fn set_ttl(&mut self, ttl: u64) -> &mut Self {
        self.staging_tx = self.staging_tx.clone().invalid_from_slot(ttl);
        self
    }

    /// Set validity start slot
    pub fn set_validity_start(&mut self, slot: u64) -> &mut Self {
        self.staging_tx = self.staging_tx.clone().valid_from_slot(slot);
        self
    }

    /// Set network ID (0 = testnet, 1 = mainnet)
    pub fn set_network_id(&mut self) -> &mut Self {
        let network_id = match self.network {
            Network::Testnet => 0u8,
            Network::Mainnet => 1u8,
        };
        self.staging_tx = self.staging_tx.clone().network_id(network_id);
        self
    }

    /// Set language view (cost model) for script data hash calculation
    ///
    /// This is required for Plutus script transactions.
    /// For convenience, consider using `set_default_language_view()` instead.
    pub fn set_language_view(&mut self, plutus_version: PlutusVersion, cost_model: Vec<i64>) -> &mut Self {
        let script_kind = match plutus_version {
            PlutusVersion::V1 => ScriptKind::PlutusV1,
            PlutusVersion::V2 => ScriptKind::PlutusV2,
            PlutusVersion::V3 => ScriptKind::PlutusV3,
        };

        self.staging_tx = self.staging_tx.clone().language_view(script_kind, cost_model);
        self
    }

    /// Set language view using default cost model
    ///
    /// This is a convenience method that automatically uses the standard mainnet cost model
    /// for the given Plutus version. This is required for Plutus script transactions.
    pub fn set_default_language_view(&mut self, plutus_version: PlutusVersion) -> &mut Self {
        let cost_model = crate::wallet::plutus::default_cost_model(plutus_version);
        self.set_language_view(plutus_version, cost_model)
    }

    /// Mint an asset
    ///
    /// # Arguments
    /// * `policy_id` - The minting policy ID (28 bytes)
    /// * `asset_name` - The asset name (max 32 bytes)
    /// * `amount` - The amount to mint (positive) or burn (negative)
    ///
    /// # Example
    /// ```no_run
    /// # use hayate::wallet::plutus::Network;
    /// # use hayate::wallet::tx_builder::PlutusTransactionBuilder;
    /// let mut builder = PlutusTransactionBuilder::new(Network::Testnet, vec![0; 57]);
    /// let policy_id = [1u8; 28];
    /// builder.mint_asset(policy_id, b"NFT".to_vec(), 1).unwrap();
    /// ```
    pub fn mint_asset(
        &mut self,
        policy_id: [u8; 28],
        asset_name: Vec<u8>,
        amount: i64,
    ) -> TxBuilderResult<&mut Self> {
        self.staging_tx = self
            .staging_tx
            .clone()
            .mint_asset(Hash::from(policy_id), asset_name, amount)
            .map_err(|e| TxBuilderError::BuildError(e.to_string()))?;
        Ok(self)
    }

    /// Add a native script for minting
    ///
    /// Native scripts are simple scripts that can check signatures, time locks, etc.
    pub fn add_native_script(&mut self, script_bytes: Vec<u8>) -> TxBuilderResult<&mut Self> {
        self.staging_tx = self
            .staging_tx
            .clone()
            .script(ScriptKind::Native, script_bytes);
        Ok(self)
    }

    /// Add a Plutus script to the witness set
    ///
    /// Use this for Plutus minting policies or validators
    pub fn add_plutus_script(&mut self, script: PlutusScript) -> TxBuilderResult<&mut Self> {
        let script_kind = match script.version() {
            PlutusVersion::V1 => ScriptKind::PlutusV1,
            PlutusVersion::V2 => ScriptKind::PlutusV2,
            PlutusVersion::V3 => ScriptKind::PlutusV3,
        };

        self.staging_tx = self
            .staging_tx
            .clone()
            .script(script_kind, script.cbor().to_vec());
        Ok(self)
    }

    /// Add a mint redeemer for Plutus minting policy
    ///
    /// Use this when minting with a Plutus minting policy (not native scripts).
    pub fn add_mint_redeemer(
        &mut self,
        policy_id: [u8; 28],
        redeemer: &Redeemer,
    ) -> TxBuilderResult<&mut Self> {
        self.staging_tx = self.staging_tx.clone().add_mint_redeemer(
            Hash::from(policy_id),
            redeemer.data_bytes().to_vec(),
            Some(pallas_txbuilder::ExUnits {
                mem: redeemer.ex_units.mem,
                steps: redeemer.ex_units.steps,
            }),
        );
        Ok(self)
    }

    /// Build the transaction
    ///
    /// Returns the transaction CBOR bytes and transaction hash
    pub fn build(&self) -> TxBuilderResult<(Vec<u8>, Vec<u8>)> {
        let built_tx = self
            .staging_tx
            .clone()
            .build_conway_raw()
            .map_err(|e| TxBuilderError::BuildError(e.to_string()))?;

        Ok((built_tx.tx_bytes.0, built_tx.tx_hash.0.to_vec()))
    }

    /// Build and sign the transaction
    ///
    /// This is a convenience method that builds the transaction and adds signatures
    pub fn build_and_sign(
        &self,
        signing_keys: Vec<pallas_wallet::PrivateKey>,
    ) -> TxBuilderResult<Vec<u8>> {
        let mut built_tx = self
            .staging_tx
            .clone()
            .build_conway_raw()
            .map_err(|e| TxBuilderError::BuildError(e.to_string()))?;

        // Add signatures
        for key in signing_keys {
            built_tx = built_tx
                .sign(key)
                .map_err(|e| TxBuilderError::BuildError(e.to_string()))?;
        }

        Ok(built_tx.tx_bytes.0)
    }

    /// Get the underlying StagingTransaction for advanced usage
    pub fn staging_transaction(&self) -> &StagingTransaction {
        &self.staging_tx
    }

    /// Get a mutable reference to the underlying StagingTransaction for advanced usage
    pub fn staging_transaction_mut(&mut self) -> &mut StagingTransaction {
        &mut self.staging_tx
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wallet::plutus::RedeemerTag;
    use crate::wallet::utxorpc_client::UtxoData;

    #[test]
    fn test_builder_creation() {
        let builder = PlutusTransactionBuilder::new(Network::Testnet, vec![1u8; 57]);
        assert!(builder.staging_transaction().inputs.is_none());
        assert!(builder.staging_transaction().outputs.is_none());
    }

    #[test]
    fn test_add_regular_input() {
        let mut builder = PlutusTransactionBuilder::new(Network::Testnet, vec![1u8; 57]);

        let utxo = UtxoData {
            tx_hash: vec![0u8; 32],
            output_index: 0,
            address: vec![1u8; 57],
            coin: 10_000_000,
            assets: Vec::new(),
            datum_hash: None,
            datum: None,
        };

        let input = PlutusInput::regular(utxo);
        builder.add_input(&input).unwrap();

        let inputs = builder.staging_transaction().inputs.as_ref().unwrap();
        assert_eq!(inputs.len(), 1);
    }

    #[test]
    fn test_add_script_input() {
        let mut builder = PlutusTransactionBuilder::new(Network::Testnet, vec![1u8; 57]);

        let utxo = UtxoData {
            tx_hash: vec![0u8; 32],
            output_index: 0,
            address: vec![1u8; 29],
            coin: 10_000_000,
            assets: Vec::new(),
            datum_hash: None,
            datum: Some(vec![1, 2, 3]),
        };

        let script = PlutusScript::v2_from_cbor(vec![1, 2, 3, 4]).unwrap();
        let redeemer = Redeemer::empty(RedeemerTag::Spend, 0);

        let input = PlutusInput::script(utxo, script, redeemer, None);
        builder.add_input(&input).unwrap();

        let inputs = builder.staging_transaction().inputs.as_ref().unwrap();
        assert_eq!(inputs.len(), 1);

        // Should have script in witness set
        let scripts = builder.staging_transaction().scripts.as_ref().unwrap();
        assert_eq!(scripts.len(), 1);
    }

    #[test]
    fn test_add_output() {
        let mut builder = PlutusTransactionBuilder::new(Network::Testnet, vec![1u8; 57]);

        let output = PlutusOutput::new(vec![1u8; 57], 5_000_000);
        builder.add_output(&output).unwrap();

        let outputs = builder.staging_transaction().outputs.as_ref().unwrap();
        assert_eq!(outputs.len(), 1);
    }

    #[test]
    fn test_add_collateral() {
        let mut builder = PlutusTransactionBuilder::new(Network::Testnet, vec![1u8; 57]);

        let utxo = UtxoData {
            tx_hash: vec![0u8; 32],
            output_index: 1,
            address: vec![1u8; 57],
            coin: 5_000_000,
            assets: Vec::new(),
            datum_hash: None,
            datum: None,
        };

        builder.add_collateral(&utxo).unwrap();

        let collateral = builder.staging_transaction().collateral_inputs.as_ref().unwrap();
        assert_eq!(collateral.len(), 1);
    }

    #[test]
    fn test_set_fee() {
        let mut builder = PlutusTransactionBuilder::new(Network::Testnet, vec![1u8; 57]);
        builder.set_fee(200_000);

        assert_eq!(builder.staging_transaction().fee, Some(200_000));
    }

    #[test]
    fn test_set_ttl() {
        let mut builder = PlutusTransactionBuilder::new(Network::Testnet, vec![1u8; 57]);
        builder.set_ttl(1000);

        assert_eq!(builder.staging_transaction().invalid_from_slot, Some(1000));
    }

    #[test]
    fn test_set_network_id() {
        let mut builder = PlutusTransactionBuilder::new(Network::Testnet, vec![1u8; 57]);
        builder.set_network_id();

        assert_eq!(builder.staging_transaction().network_id, Some(0));

        let mut builder_mainnet = PlutusTransactionBuilder::new(Network::Mainnet, vec![1u8; 57]);
        builder_mainnet.set_network_id();

        assert_eq!(builder_mainnet.staging_transaction().network_id, Some(1));
    }
}
