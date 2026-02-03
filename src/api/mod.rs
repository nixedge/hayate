// UTxORPC API implementation

pub mod query;
pub mod watch;
pub mod submit;

use tonic::transport::Server;
use std::sync::Arc;
use crate::indexer::HayateIndexer;

pub use query::query::query_service_server::QueryServiceServer;
pub use watch::watch::watch_service_server::WatchServiceServer;
pub use submit::submit::submit_service_server::SubmitServiceServer;

/// Start the UTxORPC gRPC server
pub async fn start_utxorpc_server(
    indexer: Arc<HayateIndexer>,
    bind_addr: String,
) -> anyhow::Result<()> {
    let addr = bind_addr.parse()?;
    
    tracing::info!("🚀 Starting UTxORPC server on {}", addr);
    
    let query_service = query::QueryServiceImpl::new(indexer.clone());
    let watch_service = watch::WatchServiceImpl::new(indexer.clone());
    let submit_service = submit::SubmitServiceImpl::new(indexer.clone());
    
    Server::builder()
        .add_service(QueryServiceServer::new(query_service))
        .add_service(WatchServiceServer::new(watch_service))
        .add_service(SubmitServiceServer::new(submit_service))
        .serve(addr)
        .await?;
    
    Ok(())
}
