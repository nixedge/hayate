// UTxORPC client for wallet operations

use anyhow::{Result, Context};
use tonic::transport::Channel;

// Import the generated proto types
use crate::api::query::query::{
    query_service_client::QueryServiceClient,
    ReadUtxosRequest, GetChainTipRequest,
};

/// UTxORPC client wrapper for wallet operations
pub struct WalletUtxorpcClient {
    client: QueryServiceClient<Channel>,
}

impl WalletUtxorpcClient {
    /// Create a new UTxORPC client
    pub async fn connect(endpoint: String) -> Result<Self> {
        let client = QueryServiceClient::connect(endpoint)
            .await
            .context("Failed to connect to UTxORPC endpoint")?;

        Ok(Self { client })
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
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct UtxoData {
    pub tx_hash: Vec<u8>,
    pub output_index: u32,
    #[allow(dead_code)]
    pub address: Vec<u8>,
    pub coin: u64,
    pub assets: Vec<AssetData>,
    #[allow(dead_code)]
    pub datum_hash: Option<Vec<u8>>,
    #[allow(dead_code)]
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

/// Native asset data
#[derive(Debug, Clone)]
pub struct AssetData {
    pub policy_id: Vec<u8>,
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
