# Incident Report: Missing SSTable Files in Snapshot

**Date:** 2026-03-02
**Severity:** High
**Status:** Resolved

## Summary

Hayate failed to start with error: `Failed to load SSTable run 1 from snapshot: IO error: SSTable files missing for run 1`

The snapshot at `slot-00000000000085636197` contained metadata referencing 4 SSTable runs, but the actual SSTable data files (`.blobs`, `.index`, `.keyops`, `.filter`, `.checksums`) were completely missing.

## Root Cause

A systemd-nspawn container corruption event deleted files from the host's nix store. During this incident, hayate's `/var/lib/hayate/sanchonet/utxos/snapshots/slot-00000000000085636197/` directory lost all SSTable data files, leaving only:
- `metadata` (1787 bytes)
- `metadata.checksum` (8 bytes)

## What Went Wrong

### Expected Behavior
Hayate should have detected the missing/corrupted files during snapshot loading via checksums or file existence checks.

### Actual Behavior
Hayate attempted to load the snapshot based on the metadata file alone, discovered missing SSTable files at runtime, and crashed with an unhelpful error message.

## Impact

- Hayate entered a crash loop (26+ restarts)
- UTxORPC API unavailable
- Midnight node blocked from starting (waits for hayate health check)
- No data loss: previous snapshot at `slot-00000000000085636162` was intact

## Resolution

```bash
systemctl stop hayate
rm -rf /var/lib/hayate/sanchonet/utxos/snapshots/slot-00000000000085636197/
systemctl start hayate
```

Hayate resumed from the previous snapshot and resynced the missing blocks.

## Recommendations for Hayate

### 1. Snapshot Validation on Load
**Current:** Metadata is read, SSTable files are accessed lazily
**Proposed:** Validate all SSTable files exist and checksums match before accepting a snapshot

```rust
fn validate_snapshot(snapshot_path: &Path) -> Result<(), Error> {
    let metadata = read_metadata(snapshot_path)?;

    for run in &metadata.runs {
        // Check all expected files exist
        let required_files = [".blobs", ".index", ".keyops", ".filter", ".checksums"];
        for suffix in required_files {
            let file_path = snapshot_path.join(format!("{:05}{}", run.run_number, suffix));
            if !file_path.exists() {
                return Err(Error::MissingSSTable {
                    run: run.run_number,
                    file: file_path
                });
            }
        }

        // Validate checksums
        validate_sstable_checksums(snapshot_path, run.run_number)?;
    }

    Ok(())
}
```

### 2. Snapshot Checksum on Write
**Proposed:** Create a global snapshot checksum file that covers all SSTable files

```
snapshots/slot-00000000000085636197/
├── metadata
├── metadata.checksum
├── snapshot.checksum      # NEW: checksum of all SSTable files
├── 00001.blobs
├── 00001.index
...
```

### 3. Graceful Degradation
**Proposed:** If latest snapshot is corrupted, automatically fall back to previous snapshot

```rust
fn load_latest_valid_snapshot() -> Result<Snapshot, Error> {
    for snapshot in snapshots.iter().rev() {
        match validate_snapshot(snapshot) {
            Ok(_) => return Ok(load_snapshot(snapshot)?),
            Err(e) => {
                warn!("Snapshot {} invalid: {}, trying previous", snapshot, e);
                continue;
            }
        }
    }
    Err(Error::NoValidSnapshot)
}
```

### 4. Better Error Messages
**Current:** `SSTable files missing for run 1`
**Proposed:**
```
Failed to load snapshot slot-00000000000085636197:
  Missing SSTable files for run 1:
    - /var/lib/hayate/sanchonet/utxos/snapshots/slot-00000000000085636197/00001.blobs
    - /var/lib/hayate/sanchonet/utxos/snapshots/slot-00000000000085636197/00001.index
    ...
  Attempting fallback to previous snapshot slot-00000000000085636162
```

### 5. Atomic Snapshot Creation
**Ensure:** Snapshots are only marked complete after all files are written and checksummed

```rust
fn create_snapshot(data: &Data) -> Result<(), Error> {
    let temp_dir = snapshot_dir.with_extension(".tmp");

    // Write all files to temp location
    write_sstables(&temp_dir, data)?;
    write_metadata(&temp_dir, data)?;

    // Compute and write global checksum
    write_snapshot_checksum(&temp_dir)?;

    // Atomically rename to final location
    fs::rename(temp_dir, snapshot_dir)?;

    Ok(())
}
```

## Related Issues

- Container nix store corruption (separate incident)
- Lack of per-SSTable file validation on snapshot load
- No automatic recovery from corrupted snapshots

## Lessons Learned

1. **Checksums must be validated, not just stored** - Having `.checksum` files is useless if they're not verified on load
2. **Fail gracefully** - One corrupted snapshot shouldn't prevent using older valid snapshots
3. **Validate early** - Check file existence and integrity during initialization, not during operation
4. **Better error reporting** - Specific file paths help operators diagnose issues quickly
