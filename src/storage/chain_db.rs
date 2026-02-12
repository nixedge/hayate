// Chain DB - unified view over Immutable + Volatile DBs
// TODO: Implement

use std::path::Path;
use anyhow::Result;

pub struct ChainDB {
    // TODO
}

impl ChainDB {
    pub fn open(_path: impl AsRef<Path>) -> Result<Self> {
        todo!("Implement Chain DB")
    }
}
