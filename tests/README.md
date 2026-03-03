# Hayate Tests

This directory contains various types of tests for the Hayate indexer.

## Test Categories

### Unit and Integration Tests (Default)

Standard tests that run with `cargo test`:

- `storage_manager_tests.rs` - Storage layer tests
- `block_processor_tests.rs` - Block processing logic
- `rewards_tracker_tests.rs` - Rewards calculation
- `snapshot_manager_tests.rs` - LSM snapshot management
- `config_tests.rs` - Configuration loading
- `property_tests.rs` - Property-based tests
- `api_query_tests.rs` - API query handlers
- `e2e_integration_tests.rs` - End-to-end integration (mocked)
- `fault_injection_tests.rs` - Fault tolerance
- `resource_exhaustion_tests.rs` - Resource limits
- `restart_recovery_tests.rs` - Recovery scenarios

These tests use in-memory/temporary storage and do not require external services.

### Live Integration Tests (Ignored by Default)

Located in `live_integration_tests.rs`, these tests connect to a running Hayate node and test against real blockchain data.

**Requirements:**
- A synced Hayate node running at `localhost:50051` (default)
- The node should be tracking addresses/tokens from your config

**Running live tests:**

```bash
# Use the helper script (recommended)
./run-live-tests.sh

# Or run directly with cargo:

# Run all live integration tests
cargo test --test live_integration_tests -- --ignored --nocapture

# Run a specific live test
cargo test --test live_integration_tests test_live_get_chain_tip -- --ignored --nocapture

# Run with custom endpoint
HAYATE_API=http://127.0.0.1:50053 cargo test --test live_integration_tests -- --ignored --nocapture

# List all available tests
cargo test --test live_integration_tests -- --list
```

**Why these tests are ignored:**

1. They require a running Hayate node with synced data
2. They should not run in CI/nix builds
3. They take longer to execute
4. They depend on external state

**Test Coverage:**

The live integration tests cover:

1. **Basic connectivity** - `test_live_get_chain_tip`
2. **Protocol parameters** - `test_live_read_params`
3. **UTxO queries** - `test_live_read_utxos_by_address`
4. **Multi-address queries** - `test_live_read_utxos_multiple_addresses`
5. **Policy-based search** - `test_live_search_utxos_by_policy`
6. **Transaction history** - `test_live_get_tx_history`
7. **UTxO events** - `test_live_read_utxo_events`
8. **Filtered events** - `test_live_read_utxo_events_filtered`
9. **Chain consistency** - `test_live_chain_tip_consistency`
10. **Empty queries** - `test_live_empty_address_query`
11. **Concurrent requests** - `test_live_concurrent_requests`
12. **Large responses** - `test_live_large_response`
13. **Wallet tracking** - `test_live_wallet_tracking`
14. **Token tracking** - `test_live_token_tracking`
15. **Error handling** - `test_live_invalid_requests`

## Running All Tests

```bash
# Run default tests only (no live tests)
cargo test

# Run all tests including live integration
cargo test && cargo test --test live_integration_tests --ignored

# Run with coverage
cargo tarpaulin --exclude-files 'tests/*' --ignore-tests
```

## Test Configuration

Live tests read configuration from environment variables:

- `HAYATE_API` - gRPC endpoint (default: `http://127.0.0.1:50051`)

## Debugging Tests

```bash
# Run with debug logging
RUST_LOG=debug cargo test --test live_integration_tests test_name --ignored -- --nocapture

# Run with trace logging
RUST_LOG=trace cargo test --test live_integration_tests test_name --ignored -- --nocapture
```

## Adding New Tests

### For mocked/unit tests
Add to the existing test files or create a new file in `tests/`.

### For live integration tests
Add to `live_integration_tests.rs` and mark with `#[ignore]`:

```rust
#[tokio::test]
#[ignore]
async fn test_live_my_new_test() -> Result<()> {
    let mut client = connect_client().await?;
    // Your test code
    Ok(())
}
```
