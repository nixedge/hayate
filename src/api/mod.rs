// UTxORPC API implementation

pub mod query;
pub mod watch;
pub mod submit;

use std::sync::Arc;
use crate::indexer::HayateIndexer;


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
