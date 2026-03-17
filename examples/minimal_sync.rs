//! Minimal chain sync example to demonstrate the "Await" bug
//!
//! This syncs from the current tip and should continue processing new blocks.
//!
//! Usage: cargo run --example minimal_sync -- /path/to/node.socket

use pallas_network::facades::NodeClient;
use pallas_network::miniprotocols::chainsync::NextResponse;
use pallas_traverse::MultiEraBlock;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("minimal_sync=debug".parse()?),
        )
        .init();

    let socket_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "./.run/sanchonet/cardano-node/node.socket".to_string());

    let magic = 4; // SanchoNet

    tracing::info!("Connecting to node at: {}", socket_path);

    // Connect to node
    let mut client = NodeClient::connect(&socket_path, magic).await?;

    tracing::info!("Connected! Getting chain tip...");

    // Intersect at tip
    let point = client.chainsync().intersect_tip().await?;

    tracing::info!("Intersected at tip: {:?}", point);
    tracing::info!("Starting sync loop...");

    let mut block_count = 0;

    loop {
        // Check agency and get next response
        let has_agency = client.chainsync().has_agency();
        tracing::debug!("Agency check: we have agency = {}", has_agency);

        let next = if has_agency {
            tracing::debug!("Calling request_next()");
            client.chainsync().request_next().await?
        } else {
            tracing::debug!("Calling recv_while_must_reply() - should block until message arrives");
            client.chainsync().recv_while_must_reply().await?
        };

        tracing::debug!(
            "Received response: {:?}",
            match &next {
                NextResponse::RollForward(_, _) => "RollForward",
                NextResponse::RollBackward(_, _) => "RollBackward",
                NextResponse::Await => "Await",
            }
        );

        // Process response
        match next {
            NextResponse::RollForward(block, tip) => {
                let decoded = MultiEraBlock::decode(&block.0)?;
                block_count += 1;
                tracing::info!(
                    "✓ Block #{} - slot: {}, hash: {}, tip_slot: {}",
                    block_count,
                    decoded.slot(),
                    decoded.hash(),
                    tip.0.slot_or_default()
                );
            }
            NextResponse::RollBackward(point, tip) => {
                tracing::warn!(
                    "⚠️  Rollback to {:?}, tip_slot: {}",
                    point,
                    tip.0.slot_or_default()
                );
            }
            NextResponse::Await => {
                tracing::info!("🔵 Caught up - waiting for new blocks...");
                // Just continue the loop - next iteration will check agency and call appropriate method
            }
        }
    }
}
