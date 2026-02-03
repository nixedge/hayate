# 疾風 Hayate

**Swift Cardano Indexer with UTxORPC API**

A lightweight, efficient Cardano blockchain indexer that implements the UTxORPC standard for wallet integration.

## Name

**Hayate** (疾風) - Japanese for "swift wind" or "gale". Represents speed and efficiency.

## Features

- 🚀 **UTxORPC API** - Standard gRPC protocol for wallet integration
- 💾 **Lightweight** - Only indexes tracked wallets (not entire chain)
- 🔄 **Multi-network** - Mainnet, preprod, preview, sanchonet, custom networks
- 👛 **Smart tracking** - BIP44 gap limit address discovery
- 📊 **Staking rewards** - Following cardano-wallet pattern
- 🏛️ **Full governance** - All proposals, votes from tracked wallets
- 🎯 **Efficient** - LSM tree storage, ~1MB per 1000 wallets per year
- 🦀 **Pure Rust** - Safe, fast, cross-platform

## What We Index

### For Tracked Wallets
- ✅ UTxOs (payment addresses)
- ✅ Balances (monoidal aggregation)
- ✅ Stake delegations (pool changes)
- ✅ DRep delegations (governance)
- ✅ Reward withdrawals (on-chain)
- ✅ Reward snapshots (queried from node)

### For Everyone
- ✅ All governance proposals
- ✅ All pool registrations/retirements
- ✅ Epoch nonces

## Architecture

```
┌─────────────────────────────────────────┐
│         Hayate Service                  │
│         (UTxORPC Server)                │
├─────────────────────────────────────────┤
│  Multi-Network Indexer                  │
│  ┌────────┬────────┬────────┐          │
│  │Mainnet │Preprod │SanchoNet│         │
│  └───┬────┴───┬────┴───┬────┘          │
│      │        │        │                │
│      ↓        ↓        ↓                │
│  ┌─────────────────────────┐            │
│  │    LSM Storage          │            │
│  │  • UTxOs                │            │
│  │  • Balances             │            │
│  │  • Rewards              │            │
│  │  • Governance           │            │
│  │  • Nonces               │            │
│  └─────────────────────────┘            │
│              ↓                           │
│  ┌─────────────────────────┐            │
│  │  UTxORPC gRPC (:50051)  │            │
│  │  • Query                │            │
│  │  • Watch (streaming)    │            │
│  │  • Submit               │            │
│  └─────────────────────────┘            │
└─────────────────────────────────────────┘
         ↓
    ┌────┴────┬────────┬──────┐
    │  Lace   │  CLI   │  TUI │
    └─────────┴────────┴──────┘
```

## Quick Start

```bash
# Enter development shell
nix develop

# Run on preprod testnet
just run-preprod

# Or manually
cargo run -- --network preprod --from-genesis

# Run tests
just test
```

## Usage

```bash
# Index mainnet
hayate --network mainnet --db-path ./mainnet-db

# Track specific addresses
hayate --network preprod --addresses addr1_alice addr1_bob

# Start from genesis
hayate --from-genesis
```

## Development

```bash
# Watch and test
just watch

# Lint and test
just check

# Format code
just fmt

# Build release
just build-release
```

## Storage

Uses `cardano-lsm` for efficient blockchain storage:
- **UTxO tree** - Transaction outputs
- **Balance tree** - Monoidal aggregation for fast queries
- **Governance tree** - Merkle-verified governance actions
- **Transaction tree** - Complete transaction history

## Performance Targets

- Genesis sync: < 8 hours (mainnet)
- Live block processing: < 50ms per block
- Balance query: < 1ms for any address
- Chain reorg: < 1s for 100 block rollback

## License

Apache-2.0

## Status

🚧 **Under Development** 🚧

Current: Setting up project structure
Next: Implement chain sync and block processing
