# LSM Usage Comparison: Haskell Consensus vs Hayate

**Date:** 2026-03-02
**Status:** Analysis Complete

## Executive Summary

This document compares how Haskell `ouroboros-consensus` uses the `lsm-tree` library versus Hayate's implementation. Based on this analysis, we've identified 7 key recommendations for improving Hayate's LSM usage, with 2 critical items that should be addressed soon.

---

## 1. Session Management ⚠️ CRITICAL GAP

### Haskell Consensus Pattern

```haskell
-- Uses Session abstraction from LSM library
session <- LSM.openSession tracer hasFS blockIO salt path
-- Multiple tables share ONE session
table1 <- LSM.openTable session config1
table2 <- LSM.openTable session config2
-- Session closed once, releases all tables
LSM.closeSession session
```

**Benefits:**
- Single directory lock for multiple tables
- Shared resources (compaction threads, memory)
- Atomic cleanup on shutdown

**Location:** `/ouroboros-consensus/src/ouroboros-consensus-lsm/Ouroboros/Consensus/Storage/LedgerDB/V2/LSM.hs:648-664`

### Hayate Current Pattern

**Location:** `src/indexer/mod.rs:161-173`

```rust
// Opens each tree independently
let utxo_tree = LsmTree::open(path.join("utxos"), config)?;
let balance_tree = MonoidalLsmTree::open(path.join("balances"), config)?;
let governance_tree = LsmTree::open(path.join("governance"), config)?;
// ... 10+ trees opened separately
```

**Issues:**
- Each tree acquires its own `SessionLock`
- No resource sharing between trees
- 10+ separate compaction strategies running concurrently
- Higher memory usage
- More file handles

### ⚠️ RECOMMENDATION 1: Add Session Abstraction to Rust Port

**Priority:** 🔴 CRITICAL
**Effort:** Medium (2-3 days)
**Impact:** High (resource efficiency, correctness)

**Implementation:**

```rust
// In cardano-lsm-rust/src/lib.rs
pub struct Session {
    path: PathBuf,
    config: LsmConfig,
    session_lock: SessionLock,
    // Shared compaction scheduler across tables
    compactor: Arc<Compactor>,
}

impl Session {
    pub fn open(path: impl AsRef<Path>, config: LsmConfig) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let session_lock = SessionLock::acquire(&path)?;
        let compactor = Arc::new(Compactor::new(
            config.compaction_strategy.clone(),
            path.clone(),
        ));

        Ok(Session {
            path,
            config,
            session_lock,
            compactor,
        })
    }

    pub fn open_table(&self, name: &str) -> Result<LsmTree> {
        let table_path = self.path.join(name);
        LsmTree::open_in_session(
            table_path,
            self.config.clone(),
            self.compactor.clone()
        )
    }
}
```

**Usage in Hayate:**

```rust
// In src/indexer/mod.rs
impl NetworkStorage {
    pub fn open(base_path: PathBuf, network: Network) -> Result<Self> {
        let network_path = base_path.join(network.as_str());

        // Single session for all tables
        let session = Session::open(&network_path, LsmConfig::default())?;

        let utxo_tree = session.open_table("utxos")?;
        let balance_tree = session.open_table("balances")?;
        let governance_tree = session.open_table("governance")?;
        // ... all other trees

        Ok(Self {
            session, // Store session to keep lock
            utxo_tree,
            balance_tree,
            // ...
        })
    }
}
```

**Benefits:**
- Single lock for all trees under `/var/lib/hayate/sanchonet/`
- Shared compaction scheduling (better resource utilization)
- Lower memory footprint
- Cleaner shutdown semantics

---

## 2. Snapshot Policy ⚠️ CRITICAL

### Haskell Consensus Policy

**Location:** `/ouroboros-consensus/src/ouroboros-consensus/Ouroboros/Consensus/Storage/LedgerDB/Snapshots.hs:522-561`

```haskell
SnapshotPolicy {
  onDiskNumSnapshots = 2,  -- Keep 2 snapshots minimum
  onDiskShouldTakeSnapshot = \timeSince blockDistance ->
    -- Snapshot based on:
    -- 1. Time since last snapshot
    -- 2. Number of blocks applied since last snapshot
    -- 3. On startup if many blocks were replayed
    timeSince > 300 || blockDistance > k
}
```

**Key insights:**
- Keep **minimum 2 snapshots** (guards against corruption during write)
- Snapshot on **block distance** (not just time)
- Snapshot **on startup** if replay was long

**Rationale from Haskell docs:**
> A higher number of on-disk snapshots is primarily a safe-guard against disk corruption: it trades disk space for reliability.
>
> * `1`: Delete the previous snapshot immediately after writing the next. **Dangerous policy**: if for some reason the deletion happens before the new snapshot is written entirely to disk (we don't fsync), we have no choice but to start at the genesis snapshot on the next startup.
> * `2`: Always keep 2 snapshots around. This means that when we write the next snapshot, we delete the oldest one, leaving the middle one available in case of truncation of the write. This is probably a **sane value in most circumstances**.

### Hayate Current Policy

**Location:** `src/snapshot_manager.rs:16-82`

```rust
SnapshotManager {
    tip_interval: 300s,      // 5 minutes at tip
    bulk_interval: 600s,     // 10 minutes during sync
    max_snapshots: 10,       // Keep 10
    // Only time-based, no block-distance trigger
    // No startup replay trigger
}
```

**Issues:**
- Only time-based triggers (misses important events)
- No minimum snapshot count (could have 0 after cleanup)
- No startup replay trigger (long restart = long replay)

### ⚠️ RECOMMENDATION 2: Enhance Snapshot Policy

**Priority:** 🔴 CRITICAL
**Effort:** Small (1 day)
**Impact:** High (safety, recovery time)

**Implementation:**

```rust
// In src/snapshot_manager.rs
pub struct SnapshotPolicy {
    /// Minimum snapshots to keep (default: 2)
    min_snapshots: usize,
    /// Maximum snapshots to keep (default: 10)
    max_snapshots: usize,

    /// Time-based trigger (seconds)
    time_interval_secs: u64,

    /// Block-distance trigger (snapshot every N blocks)
    block_interval: u64,

    /// Snapshot on startup if replay > N blocks
    startup_replay_threshold: u64,

    // Internal state
    last_snapshot_time: Instant,
    last_snapshot_block: u64,
}

impl SnapshotPolicy {
    pub fn default() -> Self {
        Self {
            min_snapshots: 2,
            max_snapshots: 10,
            time_interval_secs: 300,      // 5 minutes
            block_interval: 2160,         // ~1 hour at 1 block/20sec
            startup_replay_threshold: 500, // Snapshot if we replayed >500 blocks
            last_snapshot_time: Instant::now(),
            last_snapshot_block: 0,
        }
    }

    pub fn should_snapshot(
        &self,
        current_block: u64,
        startup_replay_count: Option<u64>,
    ) -> bool {
        // Time trigger
        if self.last_snapshot_time.elapsed().as_secs() >= self.time_interval_secs {
            return true;
        }

        // Block distance trigger
        let blocks_since_last = current_block.saturating_sub(self.last_snapshot_block);
        if blocks_since_last >= self.block_interval {
            return true;
        }

        // Startup trigger (if we replayed many blocks)
        if let Some(replayed) = startup_replay_count {
            if replayed >= self.startup_replay_threshold {
                tracing::info!(
                    "Taking snapshot after replaying {} blocks on startup",
                    replayed
                );
                return true;
            }
        }

        false
    }

    pub fn record_snapshot(&mut self, block_number: u64) {
        self.last_snapshot_time = Instant::now();
        self.last_snapshot_block = block_number;
    }
}
```

**Cleanup logic should respect minimum:**

```rust
pub fn cleanup_old_snapshots(&self, tree_path: &Path) -> Result<Vec<String>> {
    let snapshots = self.list_snapshots(tree_path)?;

    // Always keep at least min_snapshots
    let to_keep = self.min_snapshots.max(self.max_snapshots);

    if snapshots.len() <= to_keep {
        return Ok(Vec::new());
    }

    let to_delete = &snapshots[to_keep..];

    for snapshot in to_delete {
        // Delete snapshot
        tracing::info!("Deleting old snapshot: {}", snapshot);
        // ...
    }

    Ok(to_delete.to_vec())
}
```

**Benefits:**
- **Safety:** Minimum 2 snapshots prevents data loss during snapshot write
- **Performance:** Snapshot after N blocks prevents long replay on restart
- **Recovery:** Automatic snapshot on startup if we had to replay many blocks

---

## 3. Error Handling & Recovery

### Haskell Consensus Pattern

**Location:** `/ouroboros-consensus/src/ouroboros-consensus/Ouroboros/Consensus/Storage/LedgerDB/API.hs:534-611`

```haskell
tryNewestFirst :: [DiskSnapshot] -> m (InitLog blk, db, Word64)
tryNewestFirst [] = initFromGenesis
tryNewestFirst (s : ss) = do
  eInitDb <- initFromSnapshot s
  case eInitDb of
    -- Missing metadata? Try next
    Left err@(ReadMetadataError _ MetadataFileDoesNotExist) ->
      tryNewestFirst ss
    -- Wrong backend? Try next
    Left err@(ReadMetadataError _ MetadataBackendMismatch) ->
      tryNewestFirst ss
    -- Any other failure? DELETE and try next
    Left err -> do
      deleteSnapshotIfTemporary s
      tryNewestFirst ss
    Right (db, pt) ->
      return (InitFromSnapshot s pt, db)
```

### Hayate Current Pattern

**Location:** `src/indexer/mod.rs:140-237` (after your recent changes)

```rust
fn open_lsm_tree_with_snapshot(tree_path: PathBuf) -> Result<LsmTree> {
    // List snapshots in reverse chronological order
    let mut snapshots = list_snapshots(&tree_path)?;
    snapshots.sort();
    snapshots.reverse();

    for snapshot_name in snapshots {
        match PersistentSnapshot::load(&tree_path, &snapshot_name) {
            Ok(snapshot) => {
                match snapshot.validate() {
                    Ok(()) => {
                        match LsmTree::open_snapshot(&tree_path, &snapshot_name) {
                            Ok(tree) => return Ok(tree),
                            Err(e) => {
                                // Delete and try next
                                snapshot.delete()?;
                                continue;
                            }
                        }
                    }
                    Err(e) => {
                        // Delete corrupted and try next
                        snapshot.delete()?;
                        continue;
                    }
                }
            }
            Err(e) => {
                // Delete corrupted directory and try next
                std::fs::remove_dir_all(snapshot_path)?;
                continue;
            }
        }
    }

    // Fall back to genesis
    LsmTree::open(tree_path, LsmConfig::default())
}
```

### ✅ Status: Already Aligned!

Your recent changes (from the incident investigation) already implement the same recovery pattern as Haskell consensus:
- ✅ Try snapshots in reverse chronological order
- ✅ Validate before use
- ✅ Delete corrupted snapshots
- ✅ Fall back to previous snapshots
- ✅ Fall back to genesis if all fail

**One minor enhancement:**

### ✅ RECOMMENDATION 3: Add Structured Event Logging

**Priority:** 🟡 IMPORTANT
**Effort:** Small (1 day)
**Impact:** Medium (observability, metrics)

**Implementation:**

```rust
// In src/indexer/events.rs (new file)
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum SnapshotEvent {
    ValidatingSnapshot {
        tree: String,
        snapshot: String,
        #[serde(with = "chrono::serde::ts_seconds")]
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    SnapshotValid {
        tree: String,
        snapshot: String,
        slot: u64,
    },
    SnapshotCorrupted {
        tree: String,
        snapshot: String,
        error: String,
        missing_files: Vec<String>,
    },
    DeletingSnapshot {
        tree: String,
        snapshot: String,
        reason: String,
    },
    SnapshotDeleted {
        tree: String,
        snapshot: String
    },
    RestoredFromSnapshot {
        tree: String,
        snapshot: String,
        slot: u64,
        replay_blocks_needed: u64,
    },
    AllSnapshotsCorrupted {
        tree: String,
        attempted_count: usize,
        deleted_snapshots: Vec<String>,
    },
    StartingFromGenesis {
        tree: String,
        reason: String,
    },
}

impl SnapshotEvent {
    pub fn emit(&self) {
        // Structured JSON logging
        tracing::info!(
            target: "hayate::snapshot",
            event = ?self,
            "{}",
            serde_json::to_string(self).unwrap_or_default()
        );
    }
}
```

**Usage:**

```rust
// Replace string logs with structured events
SnapshotEvent::SnapshotCorrupted {
    tree: tree_name.to_string(),
    snapshot: snapshot_name.clone(),
    error: e.to_string(),
    missing_files: vec!["00001.blobs", "00001.index"], // from validation
}.emit();
```

**Benefits:**
- Machine-parseable logs (JSON)
- Easy metrics extraction (corrupted snapshot rate)
- Better alerting (can parse structured fields)
- Audit trail for incident investigation

---

## 4. Resource Management

### Haskell Consensus Pattern

**Location:** `/ouroboros-consensus/src/ouroboros-consensus-lsm/Ouroboros/Consensus/Storage/LedgerDB/V2/LSM.hs:222-227`

```haskell
-- Uses ResourceRegistry for automatic cleanup
allocate
  registry
  (\_ -> acquireResource)      -- How to acquire
  (releaseResource)            -- How to release (ALWAYS called)

-- On error or normal exit, ALL resources released in reverse order
```

**Benefits:**
- Exception-safe cleanup (even on panic)
- Ordered cleanup (LIFO)
- Traceable resource lifecycle

### Hayate Current Pattern

```rust
// Manual Drop implementation
impl Drop for NetworkStorage {
    fn drop(&mut self) {
        // Rust Drop doesn't guarantee cleanup on panic
        // (unless caught and handled)
    }
}
```

### ✅ RECOMMENDATION 4: Add Explicit Cleanup

**Priority:** 🟡 IMPORTANT
**Effort:** Small (1 day)
**Impact:** Medium (robustness, debuggability)

**Implementation:**

```rust
// In src/indexer/mod.rs
impl NetworkStorage {
    /// Explicit close method for graceful shutdown
    pub fn close(self) -> Result<()> {
        tracing::info!(
            "Closing NetworkStorage for {}",
            self.network.as_str()
        );

        // Explicit cleanup with error handling
        // Trees are dropped in LIFO order (reverse of creation)
        drop(self.block_hash_index);
        tracing::debug!("Closed block_hash_index");

        drop(self.block_events_tree);
        tracing::debug!("Closed block_events_tree");

        // ... drop all trees in order

        tracing::info!(
            "Successfully closed NetworkStorage for {}",
            self.network.as_str()
        );

        Ok(())
    }
}

impl Drop for NetworkStorage {
    fn drop(&mut self) {
        tracing::warn!(
            "NetworkStorage for {} dropped without explicit close()",
            self.network.as_str()
        );
        // Implicit cleanup happens here
    }
}
```

**Usage:**

```rust
// In main shutdown handler
async fn shutdown(storage: NetworkStorage) -> Result<()> {
    tracing::info!("Shutting down gracefully");
    storage.close()?;
    Ok(())
}
```

**Benefits:**
- Explicit cleanup with error handling
- Logged shutdown sequence (debugging)
- Warning if cleanup is implicit (detect bugs)

---

## 5. Configuration Management

### Haskell Consensus Pattern

**Location:** `/ouroboros-consensus/src/ouroboros-consensus-lsm/Ouroboros/Consensus/Storage/LedgerDB/V2/LSM.hs:621-631`

```haskell
data LSMArgs = LSMArgs {
  lsmPath :: FsPath,
  lsmSalt :: Salt,  -- Random salt for bloom filters
  mkBlockIO :: ResourceRegistry m -> m (HasFS, HasBlockIO)
}
```

**Centralized configuration passed to session**

### Hayate Current Pattern

**Location:** Multiple files, scattered

```rust
// Uses LsmConfig::default() everywhere
let tree = LsmTree::open(path, LsmConfig::default())?;
```

**Issues:**
- No centralized configuration
- Hard to tune for different workloads
- Defaults may not be optimal for UTxO workload

### ⚠️ RECOMMENDATION 5: Centralize LSM Configuration

**Priority:** 🟡 IMPORTANT
**Effort:** Small (1 day)
**Impact:** Medium (performance tuning, maintainability)

**Implementation:**

```rust
// In src/indexer/config.rs (new file)
use cardano_lsm::{LsmConfig, CompactionStrategy};

#[derive(Debug, Clone)]
pub struct HayateLsmConfig {
    /// Compaction strategy (Leveled for UTxO workload)
    pub compaction_strategy: CompactionStrategy,

    /// Memtable capacity in bytes (default: 64MB)
    pub memtable_capacity: usize,

    /// Bloom filter bits per key (default: 10)
    pub bloom_filter_bits_per_key: usize,

    /// Enable bloom filters (default: true)
    pub enable_bloom_filters: bool,

    /// Enable compression (default: true for SanchoNet)
    pub enable_compression: bool,
}

impl Default for HayateLsmConfig {
    fn default() -> Self {
        Self {
            // Leveled compaction optimized for UTxO workload
            // - High fanout (10) reduces write amplification
            // - More levels (7) handles large datasets
            compaction_strategy: CompactionStrategy::Leveled {
                fanout: 10,
                max_level: 7,
            },

            // 64MB memtable - good balance for UTxO writes
            memtable_capacity: 64 * 1024 * 1024,

            // 10 bits/key gives ~1% false positive rate
            bloom_filter_bits_per_key: 10,

            // Bloom filters essential for UTxO lookups
            enable_bloom_filters: true,

            // Compress on SanchoNet to save disk space
            enable_compression: true,
        }
    }

    pub fn to_lsm_config(&self) -> LsmConfig {
        LsmConfig {
            compaction_strategy: self.compaction_strategy.clone(),
            memtable_capacity: self.memtable_capacity,
            bloom_filter_bits_per_key: self.bloom_filter_bits_per_key,
            enable_bloom_filters: self.enable_bloom_filters,
            enable_compression: self.enable_compression,
        }
    }

    /// Configuration optimized for mainnet (higher write volume)
    pub fn mainnet() -> Self {
        Self {
            memtable_capacity: 128 * 1024 * 1024, // 128MB
            ..Self::default()
        }
    }

    /// Configuration for testing (smaller footprint)
    pub fn test() -> Self {
        Self {
            memtable_capacity: 4 * 1024 * 1024,  // 4MB
            compaction_strategy: CompactionStrategy::Leveled {
                fanout: 4,
                max_level: 3,
            },
            ..Self::default()
        }
    }
}
```

**Usage:**

```rust
// In src/indexer/mod.rs
impl NetworkStorage {
    pub fn open(base_path: PathBuf, network: Network) -> Result<Self> {
        let lsm_config = match network {
            Network::Mainnet => HayateLsmConfig::mainnet(),
            Network::SanchoNet => HayateLsmConfig::default(),
            Network::Preview => HayateLsmConfig::default(),
        }.to_lsm_config();

        // Use consistent config for all trees
        let utxo_tree = LsmTree::open(
            network_path.join("utxos"),
            lsm_config.clone()
        )?;

        // ...
    }
}
```

**Benefits:**
- Centralized tuning point
- Documented performance characteristics
- Easy A/B testing of configurations
- Network-specific optimizations

---

## 6. Snapshot Suffix Protection (Optional)

### Haskell Consensus Feature

**Location:** `/ouroboros-consensus/src/ouroboros-consensus/Ouroboros/Consensus/Storage/LedgerDB/Snapshots.hs:145-161`

```haskell
-- Snapshots with suffixes are NEVER deleted automatically
DiskSnapshot {
  dsNumber = 12345,
  dsSuffix = Just "last_Byron"  -- User-named, protected
}

-- In deletion logic:
when (diskSnapshotIsTemporary ss) $ do
  -- Only delete if no suffix
  removeDirectoryRecursive (snapshotToDirPath ss)
```

**Use case:** Operator manually names important epoch boundaries

### Hayate Current Behavior

Deletes ALL corrupted snapshots (no suffix support)

### ✅ RECOMMENDATION 6: Add Snapshot Suffix Support

**Priority:** 🟢 NICE TO HAVE
**Effort:** Small (1 day)
**Impact:** Low (operator convenience)

**Implementation:**

```rust
// In cardano-lsm-rust/src/snapshot.rs
impl PersistentSnapshot {
    pub fn create_with_suffix(
        lsm_path: &Path,
        slot: u64,
        suffix: Option<&str>,  // NEW: optional suffix
        label: &str,
        sstables: &[SsTableHandle],
        sequence_number: u64,
        config: &LsmConfig,
    ) -> Result<Self> {
        let name = match suffix {
            Some(s) => format!("slot-{:020}_{}", slot, s),
            None => format!("slot-{:020}", slot),
        };

        // ... rest of creation logic uses `name`
    }

    pub fn has_suffix(snapshot_name: &str) -> bool {
        snapshot_name.contains('_')
    }
}

// In hayate cleanup logic:
pub fn should_delete_snapshot(snapshot_name: &str) -> bool {
    // Don't delete if it has a suffix (user-protected)
    !PersistentSnapshot::has_suffix(snapshot_name)
}
```

**Usage:**

```bash
# Operator can manually create protected snapshots
hayate snapshot create --slot 12345678 --suffix "before_hard_fork"
# Creates: slot-00000000012345678_before_hard_fork
# This snapshot will never be auto-deleted
```

**Benefits:**
- Operators can preserve important snapshots
- Useful for epoch boundaries, hard forks
- No risk of auto-deletion

---

## 7. Table Duplication Verification (Already Good)

### Haskell Consensus Pattern

```haskell
-- When taking snapshot:
duplicatedTable <- LSM.duplicate originalTable
-- duplicatedTable shares same on-disk files (cheap!)
-- Only in-memory state is duplicated
snapshot <- LSM.snapshot duplicatedTable
```

### Hayate Current Pattern

**Location:** `cardano-lsm-rust/src/snapshot.rs:87`

```rust
// Uses hard-links (not copies)
let _linked_handle = sstable.hard_link_to(&snapshot_dir, new_run_number)?;
```

### ✅ Status: Already Correct!

**Verification:**

```bash
# After taking snapshot, verify hard-links:
$ ls -li /var/lib/hayate/sanchonet/utxos/active/00001.blobs
123456 -rw-r--r-- 2 hayate hayate 1048576 Mar 02 12:00 00001.blobs

$ ls -li /var/lib/hayate/sanchonet/utxos/snapshots/slot-00000000000012345/00001.blobs
123456 -rw-r--r-- 2 hayate hayate 1048576 Mar 02 12:00 00001.blobs
       ^^^^^^^ Same inode! Hard-linked, not copied

# Link count is 2 (shared between active and snapshot)
```

**No action needed** - snapshots are already efficient!

---

## Summary of Recommendations

### 🔴 CRITICAL (Do Soon)

| # | Recommendation | Priority | Effort | Impact | Files Affected |
|---|----------------|----------|--------|--------|----------------|
| 1 | Add Session abstraction to Rust port | 🔴 Critical | 2-3 days | High | `cardano-lsm-rust/src/lib.rs`, `hayate/src/indexer/mod.rs` |
| 2 | Enhance Snapshot Policy (min 2 snapshots, block-distance trigger) | 🔴 Critical | 1 day | High | `hayate/src/snapshot_manager.rs` |

### 🟡 IMPORTANT (Do This Quarter)

| # | Recommendation | Priority | Effort | Impact | Files Affected |
|---|----------------|----------|--------|--------|----------------|
| 3 | Add structured event logging | 🟡 Important | 1 day | Medium | `hayate/src/indexer/events.rs` (new), `hayate/src/indexer/mod.rs` |
| 4 | Add explicit close() method | 🟡 Important | 1 day | Medium | `hayate/src/indexer/mod.rs` |
| 5 | Centralize LSM configuration | 🟡 Important | 1 day | Medium | `hayate/src/indexer/config.rs` (new) |

### 🟢 NICE TO HAVE (Future)

| # | Recommendation | Priority | Effort | Impact | Files Affected |
|---|----------------|----------|--------|--------|----------------|
| 6 | Add snapshot suffix support | 🟢 Nice | 1 day | Low | `cardano-lsm-rust/src/snapshot.rs` |
| 7 | Verify snapshot efficiency | ✅ Done | 0 | - | Already using hard-links correctly |

---

## Estimated Implementation Timeline

**Phase 1 (Critical - Week 1-2):**
- Day 1-3: Implement Session abstraction in `cardano-lsm-rust`
- Day 4-5: Update Hayate to use Session abstraction
- Day 6-7: Enhance snapshot policy with min snapshots + block triggers
- Day 8: Testing and verification

**Phase 2 (Important - Week 3-4):**
- Day 9: Add structured event logging
- Day 10: Add explicit close() method
- Day 11: Centralize LSM configuration
- Day 12-14: Testing and verification

**Phase 3 (Nice to have - Future):**
- Add snapshot suffix support when operator tooling is built

---

## Testing Plan

### Critical Items Testing

**Session Abstraction:**
```bash
# Test: Multiple trees share one lock
lsof /var/lib/hayate/sanchonet/.session.lock
# Should show ONE process, multiple file descriptors

# Test: Graceful shutdown releases all resources
systemctl stop hayate
# No "failed to release" errors in logs
```

**Snapshot Policy:**
```bash
# Test: Minimum 2 snapshots always kept
hayate snapshot list
# Should always show at least 2 snapshots

# Test: Snapshot on block distance
# Verify snapshot created after N blocks, not just time
```

### Integration Testing

```bash
# Run live integration tests
./run-live-tests.sh

# Monitor snapshot events
journalctl -u hayate -f | jq 'select(.event)'

# Verify no resource leaks
watch -n 1 'lsof -p $(pgrep hayate) | wc -l'
```

---

## Metrics to Track Post-Implementation

### Resource Utilization
- File descriptor count (should decrease with Session)
- Memory usage (should decrease with shared compaction)
- Disk I/O (should remain similar or improve)

### Snapshot Health
- Snapshot creation rate (should increase with block triggers)
- Corrupted snapshot rate (should remain low)
- Average replay time on restart (should decrease)
- Snapshot count over time (should stay >= 2)

### Recovery Performance
- Time to restore from snapshot
- Number of blocks replayed on startup
- Frequency of genesis fallback (should be nearly 0)

---

## References

### Haskell Source Files
- `/ouroboros-consensus/src/ouroboros-consensus/Ouroboros/Consensus/Storage/LedgerDB/API.hs` - Initialization and recovery
- `/ouroboros-consensus/src/ouroboros-consensus/Ouroboros/Consensus/Storage/LedgerDB/Snapshots.hs` - Snapshot management
- `/ouroboros-consensus/src/ouroboros-consensus-lsm/Ouroboros/Consensus/Storage/LedgerDB/V2/LSM.hs` - LSM backend implementation
- `/lsm-tree/lsm-tree/src-core/Database/LSMTree/Internal/Snapshot.hs` - LSM snapshot internals

### Hayate Files Modified in This Analysis
- `src/indexer/mod.rs:140-237` - Snapshot recovery (recently improved)
- `src/snapshot_manager.rs` - Snapshot policy
- `cardano-lsm-rust/src/snapshot.rs` - Snapshot validation (recently added)

### Related Documentation
- [LSM Tree Port README](../cardano-lsm-rust/README.md)
- [Incident Report](./INCIDENT-2026-03-02-missing-sstables.md)
- [Conventional Commits](https://www.conventionalcommits.org/) - Commit format spec

---

## Appendix: Key Differences Summary

| Aspect | Haskell Consensus | Hayate Current | Status |
|--------|-------------------|----------------|--------|
| **Session Management** | Single session, multiple tables | One session per table | ⚠️ Needs implementation |
| **Snapshot Policy** | Time + block distance + startup | Time only | ⚠️ Needs enhancement |
| **Minimum Snapshots** | 2 minimum enforced | No minimum | ⚠️ Needs enforcement |
| **Error Recovery** | Try all, delete corrupted, fallback | Same (recently added) | ✅ Already aligned |
| **Snapshot Validation** | On read (checksum) | Early validation + checksum | ✅ Better than Haskell |
| **Resource Management** | ResourceRegistry (explicit) | RAII Drop (implicit) | 🟡 Consider explicit |
| **Configuration** | Centralized Args | Scattered defaults | 🟡 Needs centralization |
| **Event Logging** | Structured typed events | String logs | 🟡 Needs structuring |
| **Snapshot Suffixes** | Supported (protected) | Not supported | 🟢 Nice to have |
| **Hard-link Snapshots** | Yes (cheap) | Yes (cheap) | ✅ Already correct |

---

**Document Version:** 1.0
**Last Updated:** 2026-03-02
**Next Review:** After Phase 1 implementation
