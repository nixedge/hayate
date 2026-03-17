// UTxORPC API implementation

pub mod query;
pub mod watch;
pub mod submit;

use std::sync::Arc;
use crate::indexer::HayateIndexer;
use crate::config::HayateConfig;

// Include the file descriptor set for gRPC reflection
const FILE_DESCRIPTOR_SET: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/proto_descriptor.bin"));


/// Start the UTxORPC gRPC server
pub async fn start_utxorpc_server(
    indexer: Arc<HayateIndexer>,
    config: Arc<HayateConfig>,
    bind_addr: String,
    network: crate::indexer::Network,
    socket_path: Option<String>,
) -> anyhow::Result<()> {
    use tonic::transport::Server;
    use query::query::query_service_server::QueryServiceServer;
    use submit::submit::submit_service_server::SubmitServiceServer;

    // Get storage handle - clone it for API use
    let storage_handle = {
        let networks = indexer.networks.read().await;
        networks.get(&network)
            .ok_or_else(|| anyhow::anyhow!("Network storage not found"))?
            .clone()
    };

    let magic = network.magic();

    // Create query service with socket path and indexer
    let query_service = query::QueryServiceImpl::new_with_indexer(
        storage_handle,
        socket_path,
        magic,
        indexer.clone()
    );

    // Parse bind address
    let addr = bind_addr.parse()?;

    tracing::info!("🚀 UTxORPC server listening on {}", addr);

    // Build reflection service for gRPC introspection
    use tonic_reflection::server::Builder as ReflectionBuilder;
    let reflection_service = ReflectionBuilder::configure()
        .register_encoded_file_descriptor_set(FILE_DESCRIPTOR_SET)
        .build_v1()
        .map_err(|e| anyhow::anyhow!("Failed to build reflection service: {}", e))?;

    // Create submit service
    let submit_service = submit::SubmitServiceImpl::new(indexer.clone(), config);

    // Increase gRPC message size limits to handle large token responses
    // Default is 4MB, increase to 128MB for large native token datasets
    // Current CNight token dataset is ~8.2MB, this provides ample headroom
    let query_server = QueryServiceServer::new(query_service)
        .max_decoding_message_size(128 * 1024 * 1024) // 128MB
        .max_encoding_message_size(128 * 1024 * 1024); // 128MB

    let submit_server = SubmitServiceServer::new(submit_service);

    Server::builder()
        .add_service(query_server)
        .add_service(submit_server)
        .add_service(reflection_service)
        .serve(addr)
        .await?;

    Ok(())
}
