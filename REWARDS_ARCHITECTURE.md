# Rewards Tracking in Hayate

## The Cardano Rewards Challenge

Cardano calculates staking rewards **off-chain** at epoch boundaries. The reward amounts are updated in the ledger state but **no transaction** records this change.

## How Cardano-Wallet Solves This

Cardano-wallet uses a **snapshot + delta** approach:

### 1. Query Current State
```
node.query_rewards(stake_key) → current_balance
```

### 2. Track Withdrawals (On-Chain)
```
For each transaction:
  if tx.withdrawals contains stake_key:
    store(epoch, stake_key, amount)
```

### 3. Snapshot at Epoch Boundaries
```
At epoch N boundary:
  for each tracked_stake_key:
    balance = node.query_rewards(stake_key)
    store_snapshot(epoch_N, stake_key, balance)
```

### 4. Calculate Historical Rewards
```
rewards_earned_in_epoch_N = 
  snapshot(epoch_N).balance 
  - snapshot(epoch_N-1).balance
  + withdrawals_in_epoch_N
```

## What Hayate Stores

### Minimal Data (Only Tracked Wallets!)

```
Reward Snapshots:
  epoch:stake_key → balance
  Size: ~8 bytes per stake key per epoch
  
Withdrawals:
  stake_key:epoch:slot → withdrawal
  Size: ~50 bytes per withdrawal
  
Delegations:
  stake_key:slot → pool_id
  Size: ~40 bytes per delegation change
  
Nonces:
  epoch → nonce
  Size: 32 bytes per epoch
```

**For 1000 wallets over 1 year:**
- Snapshots: 1000 × 8 bytes × 73 epochs = ~584 KB
- Withdrawals: 1000 × 50 bytes × ~10/year = ~500 KB
- Delegations: 1000 × 40 bytes × ~5 changes = ~200 KB
- Nonces: 32 bytes × 73 = ~2.3 KB

**Total: ~1.3 MB per year!** (vs db-sync: 500+ GB)

## Limitations (Same as Cardano-Wallet)

### When Restoring a Wallet

**Scenario**: User adds a wallet that's been active for 2 years

**What Hayate Can Show:**
- ✅ Current reward balance (query node)
- ✅ Current delegation (query node)
- ✅ Withdrawal history (from chain)
- ❌ Historical rewards per epoch (before Hayate started)

**Response:**
```json
{
  "current_rewards": 2500000,
  "lifetime_withdrawn": 50000000,
  "estimated_lifetime": 52500000,
  "history_available_from_epoch": 450,
  "note": "Historical rewards before epoch 450 unavailable"
}
```

### When Running Continuously

If Hayate runs from epoch 450 onward:
- ✅ Full history from epoch 450
- ✅ Per-epoch breakdown
- ✅ Accurate lifetime rewards (from epoch 450)

## API Response

### UTxORPC Extension

```proto
message GetStakeInfoRequest {
  bytes stake_credential = 1;
}

message GetStakeInfoResponse {
  uint64 current_rewards = 1;
  bytes delegated_pool = 2;
  uint64 lifetime_withdrawn = 3;
  uint64 history_start_epoch = 4;  // When we started tracking
  repeated EpochReward epoch_rewards = 5;
}

message EpochReward {
  uint64 epoch = 1;
  uint64 amount = 2;
  bytes pool_id = 3;
}
```

## Implementation Notes

### Epoch Boundary Detection

```rust
// Cardano epochs are 432,000 slots (5 days)
const SLOTS_PER_EPOCH: u64 = 432_000;

fn is_epoch_boundary(slot: u64) -> bool {
    slot % SLOTS_PER_EPOCH == 0
}

fn slot_to_epoch(slot: u64) -> u64 {
    slot / SLOTS_PER_EPOCH
}
```

### Nonce Extraction

```rust
// From block header
fn extract_nonce(header: &Header) -> Option<Hash<32>> {
    header.header_body.nonce
}
```

### Critical: Snapshot BEFORE Epoch Rolls

```rust
// Must query rewards BEFORE the epoch changes
// Otherwise we lose the previous epoch's data!

async fn handle_near_epoch_boundary(slot: u64) -> Result<()> {
    let next_epoch_slot = ((slot / SLOTS_PER_EPOCH) + 1) * SLOTS_PER_EPOCH;
    let slots_until_boundary = next_epoch_slot - slot;
    
    // If within last 10 blocks of epoch
    if slots_until_boundary < 10 {
        // Snapshot NOW before it rolls over!
        self.snapshot_all_tracked_rewards().await?;
    }
    
    Ok(())
}
```

## This is Exactly What Cardano-Wallet Does!

✅ Lightweight (only track our wallets)
✅ Accurate (query authoritative ledger state)
✅ Incremental (build history over time)
❌ No historical data before we started (acceptable limitation)

**This is the right approach!** 🎯
