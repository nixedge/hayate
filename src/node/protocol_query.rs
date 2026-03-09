// Protocol Parameter Querying via Node-to-Client (N2C)
//
// Queries current protocol parameters from a Cardano node using the
// LocalStateQuery protocol via pallas-network.
//
// NOTE: Currently returns network defaults. Full N2C LocalStateQuery
// integration will be implemented in a future update.

use crate::protocol_params::{ProtocolParameters, ProtocolParamError, Result};

/// Protocol parameter query client
///
/// Connects to a Cardano node via the N2C protocol and queries
/// current protocol parameters using LocalStateQuery.
pub struct ProtocolParamQuery {
    relay: String,
    magic: u64,
}

impl ProtocolParamQuery {
    /// Create a new protocol parameter query client
    ///
    /// # Arguments
    /// * `relay` - Node address (e.g., "localhost:3001" or "/path/to/node.socket")
    /// * `magic` - Network magic number
    pub fn new(relay: String, magic: u64) -> Self {
        Self { relay, magic }
    }

    /// Query current protocol parameters from the node
    ///
    /// TODO: Implement actual N2C LocalStateQuery protocol querying.
    /// For now, returns network-specific defaults based on magic number.
    ///
    /// Uses LocalStateQuery to fetch the current protocol parameters
    /// from the node's ledger state.
    pub async fn query_current_params(&mut self) -> Result<ProtocolParameters> {
        tracing::info!(
            "Querying protocol parameters from node: {} (magic: {})",
            self.relay,
            self.magic
        );

        // TODO: Implement actual N2C query using pallas_network LocalStateQuery
        // For now, return defaults based on network magic
        let params = self.get_network_defaults()?;

        tracing::debug!(
            "Using protocol parameters: minFeeA={}, minFeeB={}, utxoCostPerByte={}",
            params.min_fee_a,
            params.min_fee_b,
            params.utxo_cost_per_byte
        );

        Ok(params)
    }

    /// Get protocol parameter defaults based on network magic
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
