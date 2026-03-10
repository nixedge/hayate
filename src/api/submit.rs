// UTxORPC Submit Service - Transaction submission

use tonic::{Request, Response, Status};
use tokio_stream::wrappers::ReceiverStream;
use crate::indexer::HayateIndexer;
use crate::config::HayateConfig;
use std::sync::Arc;

#[allow(clippy::module_inception)]
pub mod submit {
    tonic::include_proto!("utxorpc.submit.v1");
}

use submit::{
    submit_service_server::SubmitService,
    SubmitTxRequest, SubmitTxResponse,
    WaitForTxRequest, WaitForTxResponse,
    ReadMempoolRequest, ReadMempoolResponse,
};

pub struct SubmitServiceImpl {
    _indexer: Arc<HayateIndexer>,
    config: Arc<HayateConfig>,
}

impl SubmitServiceImpl {
    pub fn new(indexer: Arc<HayateIndexer>, config: Arc<HayateConfig>) -> Self {
        Self {
            _indexer: indexer,
            config,
        }
    }
}

#[tonic::async_trait]
impl SubmitService for SubmitServiceImpl {
    type ReadMempoolStream = ReceiverStream<Result<ReadMempoolResponse, Status>>;
    
    async fn submit_tx(
        &self,
        request: Request<SubmitTxRequest>,
    ) -> Result<Response<SubmitTxResponse>, Status> {
        let req = request.into_inner();

        tracing::info!("Submitting transaction ({} bytes)", req.tx.len());

        // Get socket path from config
        // Use the hardcoded socket path for now (TODO: make configurable)
        let socket_path = "/home/sam/work/iohk/midnight-playground/.run/sanchonet/cardano-node/node.socket";
        let magic = 4; // SanchoNet magic

        // Submit to Cardano node
        match crate::node::txsubmit::submit_tx(socket_path, magic, req.tx).await {
            Ok(tx_hash) => {
                tracing::info!("Transaction accepted: {}", hex::encode(&tx_hash));
                Ok(Response::new(SubmitTxResponse {
                    tx_hash,
                    accepted: true,
                    error: String::new(),
                }))
            }
            Err(e) => {
                tracing::error!("Transaction submission failed: {}", e);
                Ok(Response::new(SubmitTxResponse {
                    tx_hash: vec![],
                    accepted: false,
                    error: format!("Transaction submission failed: {}", e),
                }))
            }
        }
    }
    
    async fn wait_for_tx(
        &self,
        request: Request<WaitForTxRequest>,
    ) -> Result<Response<WaitForTxResponse>, Status> {
        let req = request.into_inner();
        
        tracing::debug!("Waiting for tx: {}", hex::encode(&req.tx_hash));
        
        // TODO: Wait for confirmation
        
        Ok(Response::new(WaitForTxResponse {
            tx_hash: req.tx_hash,
            block_height: 0,
            block_slot: 0,
            confirmed: false,
        }))
    }
    
    async fn read_mempool(
        &self,
        _request: Request<ReadMempoolRequest>,
    ) -> Result<Response<Self::ReadMempoolStream>, Status> {
        let (_tx, rx) = tokio::sync::mpsc::channel(100);
        
        // TODO: Stream mempool state
        
        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
