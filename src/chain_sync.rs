// Chain sync for Hayate - based on amaru-debug-tools patterns

#![allow(dead_code)]

use anyhow::{Context, Result};
use pallas_network::facades::PeerClient;
use pallas_network::miniprotocols::blockfetch::Client as BlockfetchClient;
use pallas_network::miniprotocols::chainsync::NextResponse;
use amaru_kernel::{Point, peer::Peer};
use amaru_network::chain_sync_client::ChainSyncClient;

pub struct HayateSync {
    chainsync_client: ChainSyncClient,
    blockfetch_client: BlockfetchClient,
}

impl HayateSync {
    pub async fn connect(host: &str, magic: u64, start_point: Point) -> Result<Self> {
        let peer = Peer::new(host);
        tracing::info!("Connecting to peer: {}", peer.name);
        
        let peer_client: PeerClient = pallas_network::facades::PeerClient::connect(peer.name.as_str(), magic)
            .await
            .context(format!("Failed to connect to peer: {}", host))?;
        
        let PeerClient {
            chainsync,
            blockfetch,
            ..
        } = peer_client;
        
        let intersection_points = vec![start_point.clone()];
        let mut chainsync_client = ChainSyncClient::new(peer, chainsync, intersection_points);
        
        tracing::info!("Finding intersection...");
        chainsync_client.find_intersection().await?;
        
        Ok(Self {
            chainsync_client,
            blockfetch_client: blockfetch,
        })
    }
    
    pub async fn request_next(&mut self) -> Result<NextResponse<pallas_network::miniprotocols::chainsync::HeaderContent>> {
        self.chainsync_client.request_next().await
            .context("Failed to request next from chain sync")
    }
}
