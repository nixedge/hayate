// Chain sync for Hayate - N2C (Node-to-Client) only

use anyhow::{Context, Result};
use pallas_network::facades::NodeClient;
use pallas_network::miniprotocols::chainsync::NextResponse;
use pallas_network::miniprotocols::Point;
use std::path::Path;

/// Hayate chain sync client (N2C only)
pub struct HayateSync {
    client: NodeClient,
}

impl HayateSync {
    /// Connect to a Cardano node via Unix socket
    pub async fn connect(socket_path: impl AsRef<Path>, magic: u64, start_point: Point) -> Result<Self> {
        let path = socket_path.as_ref();
        tracing::info!("Connecting via Unix socket: {}", path.display());

        // NodeClient::connect uses Path for Unix sockets
        let mut client = NodeClient::connect(path, magic)
            .await
            .context(format!("Failed to connect to Unix socket: {}", path.display()))?;

        // Find intersection point for resume
        let intersection_points = vec![start_point];
        client.chainsync()
            .find_intersect(intersection_points)
            .await
            .context("Failed to find intersection for N2C")?;

        tracing::info!("✓ Connected via N2C (Unix socket)");

        Ok(Self { client })
    }

    /// Check if we have agency (i.e., we can request next)
    pub fn has_agency(&mut self) -> bool {
        self.client.chainsync().has_agency()
    }

    /// Wait for the server to send us the next message (when they have agency)
    pub async fn await_next(&mut self) -> Result<NextResponse<Vec<u8>>> {
        let response = self.client.chainsync()
            .recv_while_must_reply()
            .await
            .context("Failed to await next from N2C chain sync")?;

        // Convert BlockContent to generic response with Vec<u8>
        match response {
            NextResponse::RollForward(block, tip) => {
                Ok(NextResponse::RollForward(block.0.to_vec(), tip))
            }
            NextResponse::RollBackward(point, tip) => {
                Ok(NextResponse::RollBackward(point, tip))
            }
            NextResponse::Await => Ok(NextResponse::Await),
        }
    }

    /// Request next block
    pub async fn request_next(&mut self) -> Result<NextResponse<Vec<u8>>> {
        let response = self.client.chainsync()
            .request_next()
            .await
            .context("Failed to request next from N2C chain sync")?;

        // Convert BlockContent to generic response with Vec<u8>
        match response {
            NextResponse::RollForward(block, tip) => {
                // BlockContent is a tuple struct with raw bytes
                Ok(NextResponse::RollForward(block.0.to_vec(), tip))
            }
            NextResponse::RollBackward(point, tip) => {
                Ok(NextResponse::RollBackward(point, tip))
            }
            NextResponse::Await => Ok(NextResponse::Await),
        }
    }
}
