// Volatile DB implementation - stores last ~k blocks (can still be rolled back)
// TODO: Implement

use std::path::Path;
use anyhow::Result;

pub struct VolatileDB {
    // TODO
}

impl VolatileDB {
    pub fn open(_path: impl AsRef<Path>) -> Result<Self> {
        todo!("Implement Volatile DB")
    }
}
