// UTxORPC client for wallet operations

use anyhow::{Result, Context};
use tonic::transport::Channel;
use serde::{Deserialize, Serialize};

// Hex serialization helpers for Vec<u8>
mod hex_serde {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(data: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex::encode(data))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = <String>::deserialize(deserializer)?;
        hex::decode(&s).map_err(serde::de::Error::custom)
    }
}

mod opt_hex_serde {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(data: &Option<Vec<u8>>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match data {
            Some(bytes) => serializer.serialize_some(&hex::encode(bytes)),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Vec<u8>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt = <Option<String>>::deserialize(deserializer)?;
        opt.map(|s| hex::decode(&s).map_err(serde::de::Error::custom))
            .transpose()
    }
}

// Import the generated proto types
use crate::api::query::query::{
    query_service_client::QueryServiceClient,
    ReadUtxosRequest, GetChainTipRequest, ReadParamsRequest,
};
use crate::api::submit::submit::{
    submit_service_client::SubmitServiceClient,
    SubmitTxRequest, SubmitTxResponse,
};

/// UTxORPC client wrapper for wallet operations
#[derive(Clone)]
pub struct WalletUtxorpcClient {
    client: QueryServiceClient<Channel>,
    submit_client: SubmitServiceClient<Channel>,
}

impl WalletUtxorpcClient {
    /// Create a new UTxORPC client
    pub async fn connect(endpoint: String) -> Result<Self> {
        let client = QueryServiceClient::connect(endpoint.clone())
            .await
            .context("Failed to connect to UTxORPC endpoint")?;

        let submit_client = SubmitServiceClient::connect(endpoint)
            .await
            .context("Failed to connect to UTxORPC submit endpoint")?;

        Ok(Self { client, submit_client })
    }

    /// Query UTxOs for given addresses
    pub async fn query_utxos(&mut self, addresses: Vec<Vec<u8>>) -> Result<Vec<UtxoData>> {
        let request = ReadUtxosRequest { addresses };

        let response = self.client.read_utxos(request)
            .await
            .context("Failed to query UTxOs")?
            .into_inner();

        // Convert proto UTxOs to our internal format
        let utxos = response.items.into_iter()
            .map(UtxoData::from_proto)
            .collect::<Result<Vec<_>>>()?;

        Ok(utxos)
    }

    /// Get current chain tip
    #[allow(dead_code)]
    pub async fn get_chain_tip(&mut self) -> Result<ChainTip> {
        let request = GetChainTipRequest {};

        let response = self.client.get_chain_tip(request)
            .await
            .context("Failed to get chain tip")?
            .into_inner();

        Ok(ChainTip {
            height: response.height,
            slot: response.slot,
            hash: response.hash,
        })
    }

    /// Query current protocol parameters
    ///
    /// Returns the current protocol parameters from the node, which include:
    /// - Fee calculation parameters (minFeeA, minFeeB)
    /// - UTxO cost parameters (utxoCostPerByte)
    /// - Plutus execution pricing (priceMemory, priceSteps)
    /// - Execution limits and stake parameters
    ///
    /// Returns None if the server does not have protocol parameters available.
    #[allow(dead_code)]
    pub async fn query_protocol_params(&mut self) -> Result<Option<crate::protocol_params::ProtocolParameters>> {
        let request = ReadParamsRequest {};

        let response = self.client.read_params(request)
            .await
            .context("Failed to query protocol parameters")?
            .into_inner();

        // Convert proto protocol params to our ProtocolParameters struct
        if let Some(proto_params) = response.protocol_params {
            use crate::protocol_params::{ProtocolParameters, ExUnits, Rational};

            Ok(Some(ProtocolParameters {
                min_fee_a: proto_params.min_fee_a,
                min_fee_b: proto_params.min_fee_b,
                max_tx_size: proto_params.max_tx_size,
                max_block_body_size: proto_params.max_block_body_size,
                utxo_cost_per_byte: proto_params.utxo_cost_per_byte,
                min_utxo_lovelace: if proto_params.min_utxo_lovelace > 0 {
                    Some(proto_params.min_utxo_lovelace)
                } else {
                    None
                },
                price_memory: proto_params.price_memory.map(|r| Rational {
                    numerator: r.numerator,
                    denominator: r.denominator,
                }),
                price_steps: proto_params.price_steps.map(|r| Rational {
                    numerator: r.numerator,
                    denominator: r.denominator,
                }),
                max_tx_execution_units: proto_params.max_tx_execution_units.map(|u| ExUnits {
                    mem: u.mem,
                    steps: u.steps,
                }),
                max_block_execution_units: proto_params.max_block_execution_units.map(|u| ExUnits {
                    mem: u.mem,
                    steps: u.steps,
                }),
                key_deposit: proto_params.key_deposit,
                pool_deposit: proto_params.pool_deposit,
                min_pool_cost: proto_params.min_pool_cost,
                epoch: proto_params.epoch,
            }))
        } else {
            Ok(None)
        }
    }
}

/// Chain tip information
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ChainTip {
    pub height: u64,
    pub slot: u64,
    pub hash: Vec<u8>,
}

/// UTxO data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct UtxoData {
    #[serde(with = "hex_serde")]
    pub tx_hash: Vec<u8>,
    pub output_index: u32,
    #[allow(dead_code)]
    #[serde(with = "hex_serde")]
    pub address: Vec<u8>,
    pub coin: u64,
    pub assets: Vec<AssetData>,
    #[allow(dead_code)]
    #[serde(with = "opt_hex_serde", skip_serializing_if = "Option::is_none", default)]
    pub datum_hash: Option<Vec<u8>>,
    #[allow(dead_code)]
    #[serde(with = "opt_hex_serde", skip_serializing_if = "Option::is_none", default)]
    pub datum: Option<Vec<u8>>,
}

impl UtxoData {
    fn from_proto(utxo: crate::api::query::query::Utxo) -> Result<Self> {
        let tx_hash = utxo.tx_hash;
        let output_index = utxo.output_index;
        let address = utxo.address;
        let coin = utxo.amount;

        // Parse assets
        let assets = utxo.assets.into_iter()
            .map(AssetData::from_proto)
            .collect();

        // Parse datum if present
        let datum_hash = if utxo.datum_hash.is_empty() {
            None
        } else {
            Some(utxo.datum_hash)
        };

        let datum = if utxo.datum.is_empty() {
            None
        } else {
            Some(utxo.datum)
        };

        Ok(Self {
            tx_hash,
            output_index,
            address,
            coin,
            assets,
            datum_hash,
            datum,
        })
    }

    /// Get total ADA value in lovelace
    pub fn lovelace(&self) -> u64 {
        self.coin
    }

    /// Check if this UTxO has any native assets
    #[allow(dead_code)]
    pub fn has_assets(&self) -> bool {
        !self.assets.is_empty()
    }

    /// Format as TxHash#Index
    #[allow(dead_code)]
    pub fn format_ref(&self) -> String {
        format!("{}#{}", hex::encode(&self.tx_hash), self.output_index)
    }
}

impl WalletUtxorpcClient {
    /// Submit a signed transaction to the Cardano network
    #[allow(dead_code)]
    pub async fn submit_transaction(&mut self, tx_bytes: Vec<u8>) -> Result<SubmitTxResponse> {
        let request = SubmitTxRequest { tx: tx_bytes };

        let response = self.submit_client.submit_tx(request)
            .await
            .context("Failed to submit transaction")?
            .into_inner();

        Ok(response)
    }
}

/// Native asset data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetData {
    #[serde(with = "hex_serde")]
    pub policy_id: Vec<u8>,
    #[serde(with = "hex_serde")]
    pub asset_name: Vec<u8>,
    pub amount: u64,
}

impl AssetData {
    fn from_proto(asset: crate::api::query::query::Asset) -> Self {
        Self {
            policy_id: asset.policy_id,
            asset_name: asset.asset_name,
            amount: asset.amount,
        }
    }

    /// Get the asset fingerprint (policy_id + asset_name hex)
    #[allow(dead_code)]
    pub fn fingerprint(&self) -> String {
        format!("{}.{}", hex::encode(&self.policy_id), hex::encode(&self.asset_name))
    }
}
