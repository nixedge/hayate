// UTxORPC Submit Service - Transaction submission

use tonic::{Request, Response, Status};
use tokio_stream::wrappers::ReceiverStream;
use crate::indexer::HayateIndexer;
use std::sync::Arc;

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
}

impl SubmitServiceImpl {
    #[allow(dead_code)]
    pub fn new(indexer: Arc<HayateIndexer>) -> Self {
        Self { _indexer: indexer }
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
        
        // TODO: Submit to Cardano network
        // For now, return error
        
        Ok(Response::new(SubmitTxResponse {
            tx_hash: vec![],
            accepted: false,
            error: "Transaction submission not yet implemented".to_string(),
        }))
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
