// UTxORPC Watch Service - Streaming updates

use tonic::{Request, Response, Status};
use tokio_stream::wrappers::ReceiverStream;
use crate::indexer::HayateIndexer;
use std::sync::Arc;

pub mod watch {
    tonic::include_proto!("utxorpc.watch.v1");
}

use watch::{
    watch_service_server::WatchService,
    WatchTxRequest, WatchTxResponse,
    FollowTipRequest, FollowTipResponse,
    WatchMempoolRequest, WatchMempoolResponse,
};

pub struct WatchServiceImpl {
    _indexer: Arc<HayateIndexer>,
}

impl WatchServiceImpl {
    #[allow(dead_code)]
    pub fn new(indexer: Arc<HayateIndexer>) -> Self {
        Self { _indexer: indexer }
    }
}

#[tonic::async_trait]
impl WatchService for WatchServiceImpl {
    type WatchTxStream = ReceiverStream<Result<WatchTxResponse, Status>>;
    type FollowTipStream = ReceiverStream<Result<FollowTipResponse, Status>>;
    type WatchMempoolStream = ReceiverStream<Result<WatchMempoolResponse, Status>>;
    
    async fn watch_tx(
        &self,
        _request: Request<WatchTxRequest>,
    ) -> Result<Response<Self::WatchTxStream>, Status> {
        let (_tx, rx) = tokio::sync::mpsc::channel(100);
        
        // TODO: Implement streaming tx updates
        tracing::debug!("WatchTx stream started");
        
        Ok(Response::new(ReceiverStream::new(rx)))
    }
    
    async fn follow_tip(
        &self,
        _request: Request<FollowTipRequest>,
    ) -> Result<Response<Self::FollowTipStream>, Status> {
        let (tx, rx) = tokio::sync::mpsc::channel(100);

        // Subscribe to block updates from the indexer
        let mut block_rx = self._indexer.subscribe_blocks();

        // Spawn task to forward block updates to the stream
        tokio::spawn(async move {
            tracing::info!("FollowTip stream started");

            while let Ok(block_update) = block_rx.recv().await {
                let response = FollowTipResponse {
                    height: block_update.height,
                    slot: block_update.slot,
                    hash: block_update.hash,
                    tx_hashes: block_update.tx_hashes,
                };

                if tx.send(Ok(response)).await.is_err() {
                    tracing::debug!("FollowTip stream client disconnected");
                    break;
                }
            }

            tracing::debug!("FollowTip stream ended");
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
    
    async fn watch_mempool(
        &self,
        _request: Request<WatchMempoolRequest>,
    ) -> Result<Response<Self::WatchMempoolStream>, Status> {
        let (_tx, rx) = tokio::sync::mpsc::channel(100);
        
        // TODO: Implement mempool watching
        tracing::debug!("WatchMempool stream started");
        
        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
