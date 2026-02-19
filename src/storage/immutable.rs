// Immutable DB implementation - compatible with Cardano node format

use anyhow::{anyhow, bail, Context, Result};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

use super::{BlockHash, BlockPoint};

/// Immutable DB - stores blocks beyond k-deep (immutable chain)
pub struct ImmutableDB {
    db_path: PathBuf,
    chunk_size: u64,
}

impl ImmutableDB {
    /// Open an existing Immutable DB (read-only for now)
    pub fn open(db_path: impl AsRef<Path>, chunk_size: u64) -> Result<Self> {
        let db_path = db_path.as_ref().to_path_buf();

        if !db_path.exists() {
            bail!("Immutable DB path does not exist: {}", db_path.display());
        }

        info!("Opening Immutable DB at: {}", db_path.display());
        info!("Chunk size: {}", chunk_size);

        Ok(Self { db_path, chunk_size })
    }

    /// Get tip of Immutable DB (highest slot number)
    pub fn get_tip(&self) -> Result<Option<BlockPoint>> {
        // Find the highest chunk number
        let chunks = self.list_chunks()?;

        if chunks.is_empty() {
            return Ok(None);
        }

        let last_chunk = chunks.last().unwrap();

        // Load primary index for last chunk
        let primary = PrimaryIndex::load(&self.primary_path(*last_chunk))?;

        // Find last filled slot
        for i in (0..primary.offsets.len()-1).rev() {
            let start = primary.offsets[i];
            let end = primary.offsets[i + 1];

            if start < end {
                // Found last filled slot
                let relative_slot = i as u64;
                let absolute_slot = last_chunk * self.chunk_size + relative_slot;

                // Load secondary index to get hash
                let secondary = SecondaryIndex::load(
                    &self.secondary_path(*last_chunk),
                    &primary
                )?;

                if let Some(entry) = secondary.entries.get(i) {
                    return Ok(Some(BlockPoint {
                        slot: absolute_slot,
                        hash: entry.header_hash,
                    }));
                }
            }
        }

        Ok(None)
    }

    /// Read a block by slot number
    pub fn read_block(&self, slot: u64) -> Result<Vec<u8>> {
        let (chunk_no, relative_slot) = self.slot_to_chunk(slot);

        debug!("Reading slot {} → chunk {}, relative slot {}",
               slot, chunk_no, relative_slot);

        // Load primary index
        let primary_path = self.primary_path(chunk_no);
        let primary = PrimaryIndex::load(&primary_path)
            .with_context(|| format!("Failed to load primary index for chunk {}", chunk_no))?;

        // Look up in primary index to get secondary index offset
        let (sec_offset, _sec_size) = primary
            .lookup_slot(relative_slot)
            .ok_or_else(|| anyhow!("Slot {} not found in chunk {}", slot, chunk_no))?;

        // Load secondary index file
        let secondary_path = self.secondary_path(chunk_no);
        let sec_bytes = std::fs::read(&secondary_path)
            .with_context(|| format!("Failed to read secondary index: {}", secondary_path.display()))?;

        // Parse secondary entry (56 bytes at sec_offset)
        const ENTRY_SIZE: usize = 56;
        let entry_start = sec_offset as usize;
        let entry_end = entry_start + ENTRY_SIZE;

        if entry_end > sec_bytes.len() {
            bail!("Secondary index entry out of bounds: {} > {}", entry_end, sec_bytes.len());
        }

        let mut entry = SecondaryEntry::parse(&sec_bytes[entry_start..entry_end])
            .context("Failed to parse secondary entry")?;

        // Calculate block size from next entry or chunk file size
        let next_entry_start = entry_end;
        let block_size = if next_entry_start + ENTRY_SIZE <= sec_bytes.len() {
            // There's a next entry - use its block_offset
            let next_entry = SecondaryEntry::parse(&sec_bytes[next_entry_start..next_entry_start + ENTRY_SIZE])
                .context("Failed to parse next secondary entry")?;
            next_entry.block_offset - entry.block_offset
        } else {
            // Last block - use chunk file size
            let chunk_path = self.chunk_path(chunk_no);
            let chunk_size = std::fs::metadata(&chunk_path)
                .context("Failed to get chunk file size")?
                .len();
            chunk_size - entry.block_offset
        };

        entry.block_size = block_size as u32;

        debug!("Secondary entry: offset={}, size={}, checksum={:08x}",
               entry.block_offset, entry.block_size, entry.checksum);

        // Read block from chunk file
        let chunk_path = self.chunk_path(chunk_no);
        let block_bytes = self.read_block_from_chunk(&chunk_path, &entry)?;

        // Verify checksum
        let computed_crc = crc32fast::hash(&block_bytes);
        if computed_crc != entry.checksum {
            bail!(
                "Checksum mismatch for slot {}: expected {:x}, got {:x}",
                slot,
                entry.checksum,
                computed_crc
            );
        }

        debug!("✓ Checksum verified for slot {}", slot);

        Ok(block_bytes)
    }

    /// Read block from chunk file
    fn read_block_from_chunk(&self, chunk_path: &Path, entry: &SecondaryEntry) -> Result<Vec<u8>> {
        let mut file = File::open(chunk_path)
            .with_context(|| format!("Failed to open chunk file: {}", chunk_path.display()))?;

        file.seek(SeekFrom::Start(entry.block_offset))
            .context("Failed to seek to block offset")?;

        let mut block_bytes = vec![0u8; entry.block_size as usize];
        file.read_exact(&mut block_bytes)
            .context("Failed to read block from chunk file")?;

        Ok(block_bytes)
    }

    /// List all chunk numbers present in DB
    pub fn list_chunks(&self) -> Result<Vec<u64>> {
        let mut chunks = Vec::new();

        for entry in std::fs::read_dir(&self.db_path)? {
            let entry = entry?;
            let path = entry.path();

            if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                if filename.ends_with(".chunk") {
                    if let Some(chunk_no_str) = filename.strip_suffix(".chunk") {
                        if let Ok(chunk_no) = chunk_no_str.parse::<u64>() {
                            chunks.push(chunk_no);
                        }
                    }
                }
            }
        }

        chunks.sort_unstable();
        Ok(chunks)
    }

    /// Dump information about a chunk
    pub fn dump_chunk_info(&self, chunk_no: u64) -> Result<ChunkInfo> {
        let primary = PrimaryIndex::load(&self.primary_path(chunk_no))?;
        let secondary = SecondaryIndex::load(&self.secondary_path(chunk_no), &primary)?;

        let chunk_size = std::fs::metadata(self.chunk_path(chunk_no))?.len();

        let filled_slots: Vec<u64> = (0..primary.offsets.len() - 1)
            .filter(|&i| primary.offsets[i] < primary.offsets[i + 1])
            .map(|i| i as u64)
            .collect();

        Ok(ChunkInfo {
            chunk_no,
            chunk_size,
            filled_slots,
            primary_index: primary,
            secondary_entries: secondary.entries,
        })
    }

    // Path helpers
    fn slot_to_chunk(&self, slot: u64) -> (u64, u64) {
        let chunk_no = slot / self.chunk_size;
        let relative_slot = slot % self.chunk_size;
        (chunk_no, relative_slot)
    }

    fn chunk_path(&self, chunk_no: u64) -> PathBuf {
        self.db_path.join(format!("{:05}.chunk", chunk_no))
    }

    fn primary_path(&self, chunk_no: u64) -> PathBuf {
        self.db_path.join(format!("{:05}.primary", chunk_no))
    }

    fn secondary_path(&self, chunk_no: u64) -> PathBuf {
        self.db_path.join(format!("{:05}.secondary", chunk_no))
    }
}

/// Primary Index - maps slot → secondary index offset
#[derive(Debug)]
pub struct PrimaryIndex {
    pub version: u32,
    pub offsets: Vec<u32>,
}

impl PrimaryIndex {
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("Failed to read primary index: {}", path.display()))?;

        if bytes.len() < 4 {
            bail!("Primary index file too small: {} bytes", bytes.len());
        }

        // Read version (first 4 bytes, BIG-ENDIAN)
        let version = u32::from_be_bytes(bytes[0..4].try_into()?);

        // Read offsets (remaining bytes, 4 bytes each, BIG-ENDIAN)
        let count = (bytes.len() - 4) / 4;
        let mut offsets = Vec::with_capacity(count);

        for i in 0..count {
            let start = 4 + i * 4;
            let offset = u32::from_be_bytes(bytes[start..start + 4].try_into()?);
            offsets.push(offset);
        }

        debug!(
            "Loaded primary index: version={}, slots={}, file_size={}",
            version,
            count - 1,
            bytes.len()
        );

        Ok(Self { version, offsets })
    }

    /// Look up a relative slot, returns (sec_offset, sec_size) if filled
    pub fn lookup_slot(&self, relative_slot: u64) -> Option<(u32, u32)> {
        let idx = relative_slot as usize;
        if idx >= self.offsets.len() - 1 {
            return None;
        }

        let start = self.offsets[idx];
        let end = self.offsets[idx + 1];

        if start == end {
            // Empty slot
            None
        } else {
            // Filled slot
            Some((start, end - start))
        }
    }
}

/// Secondary Index - metadata for each block
#[derive(Debug)]
pub struct SecondaryIndex {
    pub entries: Vec<SecondaryEntry>,
}

impl SecondaryIndex {
    pub fn load(path: &Path, _primary: &PrimaryIndex) -> Result<Self> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("Failed to read secondary index: {}", path.display()))?;

        let mut entries = Vec::new();

        // Secondary index is a flat array of 56-byte entries
        const ENTRY_SIZE: usize = 56;
        let entry_count = bytes.len() / ENTRY_SIZE;

        for i in 0..entry_count {
            let start = i * ENTRY_SIZE;
            let end = start + ENTRY_SIZE;

            if end <= bytes.len() {
                let entry_bytes = &bytes[start..end];
                match SecondaryEntry::parse(entry_bytes) {
                    Ok(entry) => entries.push(entry),
                    Err(e) => {
                        warn!("Failed to parse secondary entry {}: {}", i, e);
                        // Skip invalid entries
                    }
                }
            }
        }

        debug!(
            "Loaded secondary index: {} entries, file_size={}",
            entries.len(),
            bytes.len()
        );

        Ok(Self { entries })
    }
}

/// Secondary Index Entry
#[derive(Debug, Clone)]
pub struct SecondaryEntry {
    pub block_offset: u64,
    pub block_size: u32,
    pub header_offset: u16,
    pub header_size: u16,
    pub checksum: u32,
    pub header_hash: BlockHash,
    pub is_ebb: bool,
}

impl SecondaryEntry {
    /// Parse a secondary index entry
    ///
    /// Format (from actual Cardano node, BIG-ENDIAN):
    /// - block_offset: u64 BE (8 bytes) - offset in chunk file
    /// - header_offset: u16 BE (2 bytes) - offset of header within block
    /// - header_size: u16 BE (2 bytes) - size of header
    /// - checksum: u32 BE (4 bytes) - CRC32 of block
    /// - header_hash: [u8; 32] (32 bytes) - Blake2b-256 hash
    /// - block_no: u64 BE (8 bytes) - block number in chain
    /// Total: 56 bytes (0x38)
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 56 {
            bail!("Secondary entry too small: {} bytes (expected 56)", bytes.len());
        }

        let mut pos = 0;

        // Parse block_offset (u64 BE, 8 bytes)
        let block_offset = u64::from_be_bytes(bytes[pos..pos + 8].try_into()?);
        pos += 8;

        // Parse header_offset (u16 BE, 2 bytes)
        let header_offset = u16::from_be_bytes(bytes[pos..pos + 2].try_into()?);
        pos += 2;

        // Parse header_size (u16 BE, 2 bytes)
        let header_size = u16::from_be_bytes(bytes[pos..pos + 2].try_into()?);
        pos += 2;

        // Parse checksum (u32 BE, 4 bytes)
        let checksum = u32::from_be_bytes(bytes[pos..pos + 4].try_into()?);
        pos += 4;

        // Parse header_hash (32 bytes)
        let header_hash: BlockHash = bytes[pos..pos + 32].try_into()?;
        pos += 32;

        // Parse block_no (u64 BE, 8 bytes)
        let _block_no = u64::from_be_bytes(bytes[pos..pos + 8].try_into()?);

        // Calculate block size from chunk file or next entry (done by caller)

        Ok(Self {
            block_offset,
            block_size: 0, // Will be calculated later
            header_offset,
            header_size,
            checksum,
            header_hash,
            is_ebb: false, // TODO: Determine from block data
        })
    }
}

/// Information about a chunk
#[derive(Debug)]
pub struct ChunkInfo {
    pub chunk_no: u64,
    pub chunk_size: u64,
    pub filled_slots: Vec<u64>,
    pub primary_index: PrimaryIndex,
    pub secondary_entries: Vec<SecondaryEntry>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slot_to_chunk() {
        let db = ImmutableDB {
            db_path: PathBuf::from("/tmp/db"),
            chunk_size: 21600,
        };

        assert_eq!(db.slot_to_chunk(0), (0, 0));
        assert_eq!(db.slot_to_chunk(100), (0, 100));
        assert_eq!(db.slot_to_chunk(21600), (1, 0));
        assert_eq!(db.slot_to_chunk(21700), (1, 100));
    }
}
