// UTxORPC Query Service implementation

use tonic::{Request, Response, Status};
use crate::indexer::HayateIndexer;
use std::sync::Arc;

// Include generated proto code
pub mod query {
    tonic::include_proto!("utxorpc.query.v1");
}

use query::{
    query_service_server::QueryService,
    ReadUtxosRequest, ReadUtxosResponse,
    SearchUtxosRequest, SearchUtxosResponse,
    ReadParamsRequest, ReadParamsResponse,
    GetChainTipRequest, GetChainTipResponse,
};

pub struct QueryServiceImpl {
    indexer: Arc<HayateIndexer>,
}

impl QueryServiceImpl {
    pub fn new(indexer: Arc<HayateIndexer>) -> Self {
        Self { indexer }
    }
}

#[tonic::async_trait]
impl QueryService for QueryServiceImpl {
    async fn read_utxos(
        &self,
        request: Request<ReadUtxosRequest>,
    ) -> Result<Response<ReadUtxosResponse>, Status> {
        let req = request.into_inner();
        
        tracing::debug!("ReadUtxos request for {} addresses", req.addresses.len());
        
        let utxos = Vec::new();
        
        for addr_bytes in req.addresses {
            // Query UTxOs for this address from LSM storage
            // TODO: Implement actual query
            let addr = String::from_utf8_lossy(&addr_bytes).to_string();
            
            // Placeholder - will implement with actual LSM queries
            tracing::debug!("Querying UTxOs for address: {}", addr);
        }
        
        let tip = self.indexer.get_chain_tip().await
            .map_err(|e| Status::internal(format!("Failed to get chain tip: {}", e)))?;
        
        Ok(Response::new(ReadUtxosResponse {
            items: utxos,
            ledger_tip: tip.hash,
        }))
    }
    
    async fn search_utxos(
        &self,
        request: Request<SearchUtxosRequest>,
    ) -> Result<Response<SearchUtxosResponse>, Status> {
        let req = request.into_inner();
        
        tracing::debug!("SearchUtxos with pattern: {}", req.pattern);
        
        // TODO: Implement pattern-based search
        
        Ok(Response::new(SearchUtxosResponse {
            items: vec![],
        }))
    }
    
    async fn read_params(
        &self,
        _request: Request<ReadParamsRequest>,
    ) -> Result<Response<ReadParamsResponse>, Status> {
        let tip = self.indexer.get_chain_tip().await
            .map_err(|e| Status::internal(format!("Failed to get chain tip: {}", e)))?;
        
        Ok(Response::new(ReadParamsResponse {
            slot: tip.slot,
            hash: tip.hash,
        }))
    }
    
    async fn get_chain_tip(
        &self,
        _request: Request<GetChainTipRequest>,
    ) -> Result<Response<GetChainTipResponse>, Status> {
        let tip = self.indexer.get_chain_tip().await
            .map_err(|e| Status::internal(format!("Failed to get chain tip: {}", e)))?;
        
        Ok(Response::new(GetChainTipResponse {
            height: tip.height,
            slot: tip.slot,
            hash: tip.hash,
        }))
    }
}
