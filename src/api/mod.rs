// UTxORPC API implementation

pub mod query;
pub mod watch;
pub mod submit;

use std::sync::Arc;
use crate::indexer::HayateIndexer;

// Include the file descriptor set for gRPC reflection
const FILE_DESCRIPTOR_SET: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/proto_descriptor.bin"));


/// Start the UTxORPC gRPC server
pub async fn start_utxorpc_server(
    indexer: Arc<HayateIndexer>,
    bind_addr: String,
    network: crate::indexer::Network,
    socket_path: Option<String>,
) -> anyhow::Result<()> {
    use tonic::transport::Server;
    use query::query::query_service_server::QueryServiceServer;

    // Get storage handle - clone it for API use
    let storage_handle = {
        let networks = indexer.networks.read().await;
        networks.get(&network)
            .ok_or_else(|| anyhow::anyhow!("Network storage not found"))?
            .clone()
    };

    let magic = network.magic();

    // Create query service with socket path if available
    let query_service = if let Some(socket) = socket_path {
        tracing::info!("Query service configured with node socket: {}", socket);
        query::QueryServiceImpl::new_with_node(storage_handle, socket, magic)
    } else {
        tracing::warn!("Query service started without node socket - GetBlockByHash will not work");
        query::QueryServiceImpl::new(storage_handle)
    };

    // Parse bind address
    let addr = bind_addr.parse()?;

    tracing::info!("🚀 UTxORPC server listening on {}", addr);

    // Build reflection service for gRPC introspection
    use tonic_reflection::server::Builder as ReflectionBuilder;
    let reflection_service = ReflectionBuilder::configure()
        .register_encoded_file_descriptor_set(FILE_DESCRIPTOR_SET)
        .build_v1()
        .map_err(|e| anyhow::anyhow!("Failed to build reflection service: {}", e))?;

    Server::builder()
        .add_service(QueryServiceServer::new(query_service))
        .add_service(reflection_service)
        .serve(addr)
        .await?;

    Ok(())
}
