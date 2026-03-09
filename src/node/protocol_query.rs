// Protocol Parameter Querying via Node-to-Client (N2C)
//
// Queries current protocol parameters from a Cardano node using the
// LocalStateQuery protocol via pallas-network.

use crate::protocol_params::{ProtocolParameters, ExUnits, Rational, ProtocolParamError, Result};
use pallas_network::facades::NodeClient;
use pallas_network::miniprotocols::localstate::queries_v16::{self, Request};
use std::path::Path;

/// Protocol parameter query client
///
/// Connects to a Cardano node via the N2C protocol and queries
/// current protocol parameters using LocalStateQuery.
pub struct ProtocolParamQuery {
    socket_path: String,
    magic: u64,
}

impl ProtocolParamQuery {
    /// Create a new protocol parameter query client
    ///
    /// # Arguments
    /// * `socket_path` - Path to the node's Unix socket
    /// * `magic` - Network magic number
    pub fn new(socket_path: String, magic: u64) -> Self {
        Self { socket_path, magic }
    }

    /// Query current protocol parameters from the node
    ///
    /// Uses LocalStateQuery to fetch the current protocol parameters
    /// from the node's ledger state.
    pub async fn query_current_params(&mut self) -> Result<ProtocolParameters> {
        tracing::info!(
            "Querying protocol parameters from node: {} (magic: {})",
            self.socket_path,
            self.magic
        );

        // Connect to node
        let path = Path::new(&self.socket_path);
        let mut client = NodeClient::connect(path, self.magic)
            .await
            .map_err(|e| ProtocolParamError::QueryFailed(format!("Failed to connect to node: {}", e)))?;

        // Acquire the tip point (required before querying)
        let statequery = client.statequery();
        statequery.send_acquire(None)
            .await
            .map_err(|e| ProtocolParamError::QueryFailed(format!("Failed to send acquire: {}", e)))?;

        // Wait for acquired confirmation
        statequery.recv_while_acquiring()
            .await
            .map_err(|e| ProtocolParamError::QueryFailed(format!("Failed to acquire state: {}", e)))?;

        // Get current era first
        let era = queries_v16::get_current_era(statequery)
            .await
            .map_err(|e| ProtocolParamError::QueryFailed(format!("Failed to query current era: {}", e)))?;

        tracing::debug!("Current era: {}", era);

        // Query protocol parameters for the current era
        let pparams = queries_v16::get_current_pparams(statequery, era)
            .await
            .map_err(|e| ProtocolParamError::QueryFailed(format!("Failed to query protocol params: {}", e)))?;

        // Query epoch number
        let epoch = queries_v16::get_block_epoch_number(statequery, era)
            .await
            .map_err(|e| ProtocolParamError::QueryFailed(format!("Failed to query epoch: {}", e)))?;

        tracing::debug!("Current epoch: {}", epoch);

        // Release and disconnect
        statequery.send_release()
            .await
            .ok(); // Ignore release errors
        client.abort().await;

        // Parse the response
        tracing::debug!("Received {} protocol parameters from node", pparams.len());
        self.parse_protocol_params(pparams, epoch as u64)
    }

    /// Parse pallas protocol parameters into our ProtocolParameters struct
    fn parse_protocol_params(&self, pparams: Vec<queries_v16::ProtocolParam>, epoch: u64) -> Result<ProtocolParameters> {
        // The vector typically contains one ProtocolParam with all fields
        let param = pparams.first()
            .ok_or_else(|| ProtocolParamError::QueryFailed("No protocol parameters returned".to_string()))?;

        // Extract values with defaults (converting AnyUInt to u64)
        let min_fee_a = param.minfee_a.unwrap_or(44) as u64;
        let min_fee_b = param.minfee_b.unwrap_or(155_381) as u64;
        let max_tx_size = param.max_transaction_size.unwrap_or(16_384) as u64;
        let max_block_body_size = param.max_block_body_size.unwrap_or(90_112) as u64;
        let utxo_cost_per_byte: u64 = param.ada_per_utxo_byte.map(|v| v.into()).unwrap_or(4_310);
        let key_deposit: u64 = param.key_deposit.map(|v| v.into()).unwrap_or(2_000_000);
        let pool_deposit: u64 = param.pool_deposit.map(|v| v.into()).unwrap_or(500_000_000);
        let min_pool_cost: u64 = param.min_pool_cost.map(|v| v.into()).unwrap_or(340_000_000);

        // Parse execution costs (RationalNumber already has numerator/denominator)
        let (price_memory, price_steps) = if let Some(ref exec_costs) = param.execution_costs {
            let mem = Some(Rational {
                numerator: exec_costs.mem_price.numerator,
                denominator: exec_costs.mem_price.denominator,
            });
            let steps = Some(Rational {
                numerator: exec_costs.step_price.numerator,
                denominator: exec_costs.step_price.denominator,
            });
            (mem, steps)
        } else {
            (None, None)
        };

        // Parse execution units (convert u32 to u64)
        let max_tx_execution_units = param.max_tx_ex_units.as_ref().map(|units| ExUnits {
            mem: units.mem as u64,
            steps: units.steps as u64,
        });

        let max_block_execution_units = param.max_block_ex_units.as_ref().map(|units| ExUnits {
            mem: units.mem as u64,
            steps: units.steps as u64,
        });

        Ok(ProtocolParameters {
            min_fee_a,
            min_fee_b,
            max_tx_size,
            max_block_body_size,
            utxo_cost_per_byte,
            min_utxo_lovelace: None, // Legacy, not in modern params
            price_memory,
            price_steps,
            max_tx_execution_units,
            max_block_execution_units,
            key_deposit,
            pool_deposit,
            min_pool_cost,
            epoch,
        })
    }

    /// Get protocol parameter defaults based on network magic (fallback)
    fn get_network_defaults(&self) -> Result<ProtocolParameters> {
        // Network magic numbers from config.rs
        match self.magic {
            764824073 => Ok(ProtocolParameters::mainnet_defaults()),
            1 => Ok(ProtocolParameters::preprod_defaults()),
            2 => Ok(ProtocolParameters::preview_defaults()),
            4 => Ok(ProtocolParameters::sanchonet_defaults()),
            _ => {
                tracing::warn!(
                    "Unknown network magic {}, using mainnet defaults",
                    self.magic
                );
                Ok(ProtocolParameters::mainnet_defaults())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_param_query_creation() {
        let query = ProtocolParamQuery::new("localhost:3001".to_string(), 1);
        assert_eq!(query.relay, "localhost:3001");
        assert_eq!(query.magic, 1);
    }

    #[tokio::test]
    async fn test_query_mainnet_defaults() {
        let mut query = ProtocolParamQuery::new("localhost:3001".to_string(), 764824073);
        let params = query.query_current_params().await.unwrap();
        assert_eq!(params.min_fee_a, 44);
        assert_eq!(params.min_fee_b, 155_381);
    }

    #[tokio::test]
    async fn test_query_preprod_defaults() {
        let mut query = ProtocolParamQuery::new("localhost:3001".to_string(), 1);
        let params = query.query_current_params().await.unwrap();
        assert_eq!(params.min_fee_a, 44);
    }
}
