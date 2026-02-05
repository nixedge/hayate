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
/// NOTE: Currently disabled - use examples/query_server.rs instead
/// TODO: Refactor to work with new QueryServiceImpl API
#[allow(dead_code)]
pub async fn start_utxorpc_server(
    _indexer: Arc<HayateIndexer>,
    _bind_addr: String,
) -> anyhow::Result<()> {
    unimplemented!("Use examples/query_server.rs instead")
}
