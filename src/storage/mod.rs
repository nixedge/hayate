// Storage layer for Hayate - Cardano-compatible Immutable and Volatile DBs

pub mod immutable;
pub mod volatile;
pub mod chain_db;

pub use immutable::ImmutableDB;
pub use volatile::VolatileDB;
pub use chain_db::ChainDB;

/// Block point (slot + hash)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BlockPoint {
    pub slot: u64,
    pub hash: BlockHash,
}

/// Block hash (Blake2b-256)
pub type BlockHash = [u8; 32];

/// Chain hash (genesis or block hash)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ChainHash {
    Genesis,
    Block(BlockHash),
}
