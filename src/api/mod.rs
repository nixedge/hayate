// UTxORPC API implementation

pub mod query;
pub mod watch;
pub mod submit;

use std::sync::Arc;
use crate::indexer::HayateIndexer;


/// Start the UTxORPC gRPC server
pub async fn start_utxorpc_server(
    indexer: Arc<HayateIndexer>,
    bind_addr: String,
    network: crate::indexer::Network,
    socket_path: Option<String>,
) -> anyhow::Result<()> {
    use tonic::transport::Server;
    use query::query::query_service_server::QueryServiceServer;

    // Get network storage - we need to extract it to avoid holding the lock
    let storage = {
        let mut networks = indexer.networks.write().await;
        networks.remove(&network)
            .ok_or_else(|| anyhow::anyhow!("Network storage not found"))?
    };

    let magic = network.magic();

    // Create query service with socket path if available
    let query_service = if let Some(socket) = socket_path {
        tracing::info!("Query service configured with node socket: {}", socket);
        query::QueryServiceImpl::new_with_node(storage, socket, magic)
    } else {
        tracing::warn!("Query service started without node socket - GetBlockByHash will not work");
        query::QueryServiceImpl::new(storage)
    };

    // Parse bind address
    let addr = bind_addr.parse()?;

    tracing::info!("🚀 UTxORPC server listening on {}", addr);

    Server::builder()
        .add_service(QueryServiceServer::new(query_service))
        .serve(addr)
        .await?;

    Ok(())
}
