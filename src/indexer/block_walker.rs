// Walk backwards through the chain by following prev_hash references
//
// This module implements proper N-block rollback by following prev_hash
// chain using hayate's storage index.

use anyhow::{Result, anyhow};
use pallas_network::{
    facades::NodeClient,
    miniprotocols::Point,
};
use std::path::PathBuf;
use crate::indexer::storage_manager::StorageHandle;

/// Walk back N blocks from the current tip by following prev_hash chain
/// Uses hayate's storage to efficiently look up block metadata
pub async fn walk_back_n_blocks(
    storage: &StorageHandle,
    socket_path: &PathBuf,
    network_magic: u64,
    n_blocks: u64,
) -> Result<(u64, Vec<u8>)> {
    if n_blocks == 0 {
        return Err(anyhow!("Cannot walk back 0 blocks"));
    }

    tracing::info!("Walking back {} blocks from tip using prev_hash chain", n_blocks);

    // Connect to node
    let mut client = NodeClient::connect(socket_path, network_magic)
        .await
        .map_err(|e| anyhow!("Failed to connect to node: {}", e))?;

    // Get current tip by finding intersection at origin (this returns the tip)
    let (_intersection, tip) = client
        .chainsync()
        .find_intersect(vec![Point::Origin])
        .await
        .map_err(|e| anyhow!("Failed to query tip: {}", e))?;

    // Extract slot and hash from tip Point
    let (tip_slot, tip_hash) = match tip.0 {
        Point::Specific(slot, hash) => (slot, hash),
        Point::Origin => return Ok((0, vec![])),
    };

    tracing::info!("Starting from tip: slot={}, hash={}", tip_slot, hex::encode(&tip_hash));

    // Walk backwards N blocks using storage lookups (much faster than network queries!)
    let mut current_hash = tip_hash;
    let mut blocks_walked = 0u64;

    while blocks_walked < n_blocks {
        // Look up current block's metadata in storage
        let metadata = storage.get_block_metadata(current_hash.clone()).await
            .map_err(|e| anyhow!("Failed to query block metadata from storage: {}", e))?;

        let (_slot, _timestamp, prev_hash_opt) = match metadata {
            Some(m) => m,
            None => {
                return Err(anyhow!(
                    "Block not found in storage: {}. Block may not be indexed yet.",
                    hex::encode(&current_hash)
                ));
            }
        };

        // Get previous block hash
        let prev_hash = match prev_hash_opt {
            Some(h) => h,
            None => {
                // Hit genesis
                tracing::info!("Reached genesis after {} blocks", blocks_walked);
                return Ok((0, vec![]));
            }
        };

        blocks_walked += 1;
        tracing::debug!("Block {}/{}: hash={}, prev_hash={}",
                      blocks_walked, n_blocks, hex::encode(&current_hash), hex::encode(&prev_hash));

        if blocks_walked >= n_blocks {
            // We've walked back N blocks. Look up the slot of the target block.
            let target_metadata = storage.get_block_metadata(prev_hash.clone()).await
                .map_err(|e| anyhow!("Failed to query target block metadata: {}", e))?;

            let (target_slot, _timestamp, _prev) = match target_metadata {
                Some(m) => m,
                None => {
                    return Err(anyhow!(
                        "Target block not found in storage: {}. Block may not be indexed yet.",
                        hex::encode(&prev_hash)
                    ));
                }
            };

            tracing::info!("✅ Walked back {} blocks to slot={}, hash={}",
                         n_blocks, target_slot, hex::encode(&prev_hash));
            return Ok((target_slot, prev_hash));
        }

        // Move to previous block
        current_hash = prev_hash;
    }

    Err(anyhow!("Failed to walk back {} blocks", n_blocks))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires running node and storage
    async fn test_walk_back_blocks() {
        // This test would need a real storage handle and running node
        // For now, just verify the function signature compiles
        let _socket = PathBuf::from("/tmp/node.socket");
        let _magic = 1; // Preprod

        // Note: Would need actual StorageHandle to run this test
        // let storage = StorageHandle::new(...);
        // let result = walk_back_n_blocks(&storage, &_socket, _magic, 10).await;
        // assert!(result.is_ok());
    }
}
