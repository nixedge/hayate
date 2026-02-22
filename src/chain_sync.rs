// Chain sync for Hayate - supports both TCP (N2N) and Unix socket (N2C) connections

#![allow(dead_code)]

use anyhow::{Context, Result};
use pallas_network::facades::{NodeClient, PeerClient};
use pallas_network::miniprotocols::blockfetch::Client as BlockfetchClient;
use pallas_network::miniprotocols::chainsync::NextResponse;
use amaru_kernel::{Point, Peer};
use amaru_network::chain_sync_client::ChainSyncClient;
use std::path::Path;

/// Hayate chain sync client (supports both N2N and N2C)
pub enum HayateSync {
    /// Node-to-Node connection (TCP, for relays)
    N2N {
        chainsync_client: ChainSyncClient,
        blockfetch_client: BlockfetchClient,
    },
    /// Node-to-Client connection (Unix socket, for local node)
    N2C {
        client: NodeClient,
        peer: Peer,
        start_point: Point,
    },
}

/// Connection target for Cardano node
#[derive(Debug, Clone)]
pub enum NodeConnection {
    /// TCP connection (host:port)
    Tcp(String),
    /// Unix domain socket
    UnixSocket(std::path::PathBuf),
}

impl HayateSync {
    /// Connect to a Cardano node via TCP (host:port format)
    pub async fn connect(host: &str, magic: u64, start_point: Point) -> Result<Self> {
        Self::connect_with(NodeConnection::Tcp(host.to_string()), magic, start_point).await
    }

    /// Connect to a Cardano node via Unix socket
    pub async fn connect_unix(socket_path: impl AsRef<Path>, magic: u64, start_point: Point) -> Result<Self> {
        Self::connect_with(
            NodeConnection::UnixSocket(socket_path.as_ref().to_path_buf()),
            magic,
            start_point
        ).await
    }

    /// Connect to a Cardano node (internal implementation)
    async fn connect_with(connection: NodeConnection, magic: u64, start_point: Point) -> Result<Self> {
        match connection {
            NodeConnection::Tcp(host) => {
                tracing::info!("Connecting via TCP to: {}", host);
                let peer = Peer::new(&host);

                let peer_client: PeerClient = PeerClient::connect(&host, magic)
                    .await
                    .context(format!("Failed to connect to TCP peer: {}", host))?;

                let PeerClient {
                    chainsync,
                    blockfetch,
                    ..
                } = peer_client;

                let intersection_points = vec![start_point.clone()];
                let mut chainsync_client = ChainSyncClient::new(peer, chainsync, intersection_points);

                tracing::info!("Finding intersection...");
                chainsync_client.find_intersection().await
                    .context("Failed to find intersection")?;

                tracing::info!("✓ Connected via N2N");

                Ok(Self::N2N {
                    chainsync_client,
                    blockfetch_client: blockfetch,
                })
            }
            NodeConnection::UnixSocket(path) => {
                let path_str = path.to_string_lossy().to_string();
                tracing::info!("Connecting via Unix socket: {}", path_str);

                // NodeClient::connect uses Path for Unix sockets
                let mut client = NodeClient::connect(&path, magic)
                    .await
                    .context(format!("Failed to connect to Unix socket: {}", path.display()))?;

                // Find intersection point for resume
                // Convert amaru Point to pallas Point
                let pallas_point = match &start_point {
                    Point::Origin => pallas_network::miniprotocols::Point::Origin,
                    Point::Specific(slot, hash) => pallas_network::miniprotocols::Point::Specific(
                        (*slot).into(),
                        hash.to_vec(),
                    ),
                };

                let intersection_points = vec![pallas_point];
                client.chainsync()
                    .find_intersect(intersection_points)
                    .await
                    .context("Failed to find intersection for N2C")?;

                tracing::info!("✓ Connected via N2C (Unix socket)");

                Ok(Self::N2C {
                    client,
                    peer: Peer::new(&path_str),
                    start_point,
                })
            }
        }
    }

    /// Request next block/header
    /// N2N returns HeaderContent, N2C returns BlockContent
    /// For now, we'll handle them separately
    pub async fn request_next_header(&mut self) -> Result<Option<NextResponse<pallas_network::miniprotocols::chainsync::HeaderContent>>> {
        match self {
            Self::N2N { chainsync_client, .. } => {
                Ok(Some(chainsync_client.request_next().await
                    .context("Failed to request next from N2N chain sync")?))
            }
            Self::N2C { .. } => {
                // N2C doesn't return headers directly
                Ok(None)
            }
        }
    }

    /// Request next block (N2C only)
    pub async fn request_next_block(&mut self) -> Result<Option<NextResponse<pallas_network::miniprotocols::chainsync::BlockContent>>> {
        match self {
            Self::N2N { .. } => {
                // N2N returns headers, not full blocks via chainsync
                Ok(None)
            }
            Self::N2C { client, .. } => {
                Ok(Some(client.chainsync()
                    .request_next()
                    .await
                    .context("Failed to request next from N2C chain sync")?))
            }
        }
    }

    /// Unified request_next that works for both N2N and N2C
    /// Returns tuple of (slot, hash_bytes) for the next item
    pub async fn request_next(&mut self) -> Result<NextResponse<Vec<u8>>> {
        match self {
            Self::N2N { chainsync_client, .. } => {
                let response = chainsync_client.request_next().await
                    .context("Failed to request next from N2N chain sync")?;

                // Convert HeaderContent to generic response
                match response {
                    NextResponse::RollForward(header, tip) => {
                        // HeaderContent has cbor field
                        Ok(NextResponse::RollForward(header.cbor.to_vec(), tip))
                    }
                    NextResponse::RollBackward(point, tip) => {
                        Ok(NextResponse::RollBackward(point, tip))
                    }
                    NextResponse::Await => Ok(NextResponse::Await),
                }
            }
            Self::N2C { client, .. } => {
                let response = client.chainsync()
                    .request_next()
                    .await
                    .context("Failed to request next from N2C chain sync")?;

                // Convert BlockContent to generic response
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
    }
}
