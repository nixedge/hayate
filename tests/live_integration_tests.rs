// Live Integration Tests
// These tests connect to a running Hayate node and test against real SanchoNet data
//
// IMPORTANT: These tests require a synced Hayate node running at localhost:50051
// They are marked with #[ignore] to prevent them from running in CI/nix builds
//
// To run these tests:
//   cargo test --test live_integration_tests --ignored
//
// Or to run a specific test:
//   cargo test --test live_integration_tests test_name --ignored

use anyhow::Result;
use tonic::Request;

// Generated protobuf code
mod query {
    tonic::include_proto!("utxorpc.query.v1");
}

use query::{
    query_service_client::QueryServiceClient,
    GetChainTipRequest, ReadUtxosRequest, SearchUtxosRequest, ReadParamsRequest,
    GetTxHistoryRequest, ReadUtxoEventsRequest,
};

/// Default endpoint for local Hayate node
const HAYATE_ENDPOINT: &str = "http://127.0.0.1:50051";

/// Helper to get the endpoint from environment variable or use default
fn get_endpoint() -> String {
    std::env::var("HAYATE_API").unwrap_or_else(|_| HAYATE_ENDPOINT.to_string())
}

/// Connect to the Hayate gRPC service
async fn connect_client() -> Result<QueryServiceClient<tonic::transport::Channel>> {
    let endpoint = get_endpoint();
    let client = QueryServiceClient::connect(endpoint).await?;
    Ok(client)
}

// Test 1: Basic connectivity and GetChainTip
#[tokio::test]
#[ignore]
async fn test_live_get_chain_tip() -> Result<()> {
    let mut client = connect_client().await?;

    let request = Request::new(GetChainTipRequest {});
    let response = client.get_chain_tip(request).await?;
    let tip = response.into_inner();

    // Verify we got a valid response
    println!("Chain Tip - Height: {}, Slot: {}, Hash: {}",
        tip.height, tip.slot, hex::encode(&tip.hash));

    // SanchoNet should have a non-zero tip
    assert!(tip.slot > 0, "Chain tip slot should be greater than 0");
    // Note: height may not be tracked by all indexer implementations
    assert!(!tip.hash.is_empty(), "Chain tip hash should not be empty");
    assert!(tip.hash.len() == 32, "Chain tip hash should be 32 bytes");

    Ok(())
}

// Test 2: ReadParams - verify protocol parameters
#[tokio::test]
#[ignore]
async fn test_live_read_params() -> Result<()> {
    let mut client = connect_client().await?;

    let request = Request::new(ReadParamsRequest {});
    let response = client.read_params(request).await?;
    let params = response.into_inner();

    println!("Params - Slot: {}, Hash: {}",
        params.slot, hex::encode(&params.hash));

    // Should return current slot and hash
    assert!(params.slot > 0, "Params slot should be greater than 0");

    Ok(())
}

// Test 3: ReadUtxos with a known SanchoNet address
#[tokio::test]
#[ignore]
async fn test_live_read_utxos_by_address() -> Result<()> {
    let mut client = connect_client().await?;

    // Use the address from the config file
    // addr_test1wplxjzranravtp574s2wz00md7vz9rzpucu252je68u9a8qzjheng
    let address = "addr_test1wplxjzranravtp574s2wz00md7vz9rzpucu252je68u9a8qzjheng";

    // Decode bech32 address to bytes
    use pallas_addresses::Address;
    let addr_bytes = Address::from_bech32(address)
        .expect("Failed to decode address")
        .to_vec();

    let request = Request::new(ReadUtxosRequest {
        addresses: vec![addr_bytes],
    });

    let response = client.read_utxos(request).await?;
    let result = response.into_inner();

    println!("Found {} UTxOs for address {}", result.items.len(), address);
    println!("Ledger tip: {}", hex::encode(&result.ledger_tip));

    // Print details of each UTxO
    for (i, utxo) in result.items.iter().enumerate() {
        println!("  UTxO {}: {}#{} - {} lovelace",
            i,
            hex::encode(&utxo.tx_hash),
            utxo.output_index,
            utxo.amount
        );

        if !utxo.assets.is_empty() {
            println!("    Assets:");
            for asset in &utxo.assets {
                println!("      {}.{}: {}",
                    hex::encode(&asset.policy_id),
                    hex::encode(&asset.asset_name),
                    asset.amount
                );
            }
        }
    }

    // Verify response structure
    assert!(!result.ledger_tip.is_empty(), "Ledger tip should not be empty");

    // Note: The address may or may not have UTxOs, so we don't assert on count
    // But we verify that if there are UTxOs, they have valid structure
    for utxo in &result.items {
        assert!(!utxo.tx_hash.is_empty(), "UTxO tx_hash should not be empty");
        assert_eq!(utxo.tx_hash.len(), 32, "UTxO tx_hash should be 32 bytes");
        assert!(!utxo.address.is_empty(), "UTxO address should not be empty");
        assert!(utxo.amount > 0, "UTxO amount should be greater than 0");
    }

    Ok(())
}

// Test 4: ReadUtxos with multiple addresses
#[tokio::test]
#[ignore]
async fn test_live_read_utxos_multiple_addresses() -> Result<()> {
    let mut client = connect_client().await?;

    // Use addresses from the config
    let addresses = vec![
        "addr_test1wplxjzranravtp574s2wz00md7vz9rzpucu252je68u9a8qzjheng",
    ];

    use pallas_addresses::Address;
    let addr_bytes: Vec<Vec<u8>> = addresses
        .iter()
        .map(|addr| {
            Address::from_bech32(addr)
                .expect("Failed to decode address")
                .to_vec()
        })
        .collect();

    let request = Request::new(ReadUtxosRequest {
        addresses: addr_bytes,
    });

    let response = client.read_utxos(request).await?;
    let result = response.into_inner();

    println!("Found {} total UTxOs across {} addresses",
        result.items.len(), addresses.len());

    Ok(())
}

// Test 5: SearchUtxos by policy ID
#[tokio::test]
#[ignore]
async fn test_live_search_utxos_by_policy() -> Result<()> {
    let mut client = connect_client().await?;

    // Use the policy ID from the config
    // 250c2ff62ed5ca195009a3557966febf05f14d60f1a52761b547f3da
    let policy_id = "250c2ff62ed5ca195009a3557966febf05f14d60f1a52761b547f3da";

    let request = Request::new(SearchUtxosRequest {
        pattern: policy_id.to_string(),
    });

    let response = client.search_utxos(request).await?;
    let result = response.into_inner();

    println!("Found {} UTxOs containing policy {}", result.items.len(), policy_id);

    // Print details of found UTxOs
    for (i, utxo) in result.items.iter().enumerate().take(10) {
        println!("  UTxO {}: {}#{}",
            i,
            hex::encode(&utxo.tx_hash),
            utxo.output_index
        );

        for asset in &utxo.assets {
            if hex::encode(&asset.policy_id) == policy_id {
                println!("    Found asset: {}.{} = {}",
                    hex::encode(&asset.policy_id),
                    hex::encode(&asset.asset_name),
                    asset.amount
                );
            }
        }
    }

    // Verify all returned UTxOs contain the policy
    for utxo in &result.items {
        let has_policy = utxo.assets.iter().any(|asset| {
            hex::encode(&asset.policy_id) == policy_id
        });
        assert!(has_policy, "All returned UTxOs should contain the searched policy");
    }

    Ok(())
}

// Test 6: GetTxHistory for an address
#[tokio::test]
#[ignore]
async fn test_live_get_tx_history() -> Result<()> {
    let mut client = connect_client().await?;

    let address = "addr_test1wplxjzranravtp574s2wz00md7vz9rzpucu252je68u9a8qzjheng";

    use pallas_addresses::Address;
    let addr_bytes = Address::from_bech32(address)
        .expect("Failed to decode address")
        .to_vec();

    let request = Request::new(GetTxHistoryRequest {
        address: addr_bytes,
        max_txs: 100, // Limit to 100 transactions
    });

    let response = client.get_tx_history(request).await?;
    let result = response.into_inner();

    println!("Found {} transactions for address {}",
        result.tx_hashes.len(), address);

    // Print first few transaction hashes
    for (i, tx_hash) in result.tx_hashes.iter().enumerate().take(5) {
        println!("  Tx {}: {}", i, hex::encode(tx_hash));
        assert_eq!(tx_hash.len(), 32, "Transaction hash should be 32 bytes");
    }

    // Verify max_txs limit was respected
    assert!(result.tx_hashes.len() <= 100, "Should respect max_txs limit");

    Ok(())
}

// Test 7: ReadUtxoEvents in a slot range
#[tokio::test]
#[ignore]
async fn test_live_read_utxo_events() -> Result<()> {
    let mut client = connect_client().await?;

    // First, get the current chain tip to determine a valid slot range
    let tip_request = Request::new(GetChainTipRequest {});
    let tip_response = client.get_chain_tip(tip_request).await?;
    let tip = tip_response.into_inner();

    // Query events in a recent 100-slot range
    let end_slot = tip.slot;
    let start_slot = end_slot.saturating_sub(100);

    println!("Querying UTxO events from slot {} to {}", start_slot, end_slot);

    let request = Request::new(ReadUtxoEventsRequest {
        start_slot,
        end_slot,
        addresses: vec![], // No address filter
        max_events: 50, // Limit to 50 events
    });

    let response = client.read_utxo_events(request).await?;
    let result = response.into_inner();

    println!("Found {} UTxO events in slot range {}-{}",
        result.events.len(), start_slot, end_slot);

    // Print event details
    for (i, event) in result.events.iter().enumerate().take(10) {
        let event_type = match event.event_type {
            0 => "CREATED",
            1 => "SPENT",
            _ => "UNKNOWN",
        };

        println!("  Event {}: {} - {}#{} at slot {}",
            i,
            event_type,
            hex::encode(&event.tx_hash),
            event.output_index,
            event.slot
        );

        // Verify event structure
        assert!(!event.tx_hash.is_empty(), "Event tx_hash should not be empty");
        assert!(event.slot >= start_slot && event.slot <= end_slot,
            "Event slot should be within requested range");
        assert!(!event.block_hash.is_empty(), "Event block_hash should not be empty");
    }

    // Verify max_events limit was respected
    assert!(result.events.len() <= 50, "Should respect max_events limit");

    Ok(())
}

// Test 8: ReadUtxoEvents with address filter
#[tokio::test]
#[ignore]
async fn test_live_read_utxo_events_filtered() -> Result<()> {
    let mut client = connect_client().await?;

    // Get chain tip
    let tip_request = Request::new(GetChainTipRequest {});
    let tip_response = client.get_chain_tip(tip_request).await?;
    let tip = tip_response.into_inner();

    // Query events for specific address in recent range
    let end_slot = tip.slot;
    let start_slot = end_slot.saturating_sub(1000); // Larger range for filtered query

    let address = "addr_test1wplxjzranravtp574s2wz00md7vz9rzpucu252je68u9a8qzjheng";

    use pallas_addresses::Address;
    let addr_bytes = Address::from_bech32(address)
        .expect("Failed to decode address")
        .to_vec();

    let request = Request::new(ReadUtxoEventsRequest {
        start_slot,
        end_slot,
        addresses: vec![addr_bytes],
        max_events: 20,
    });

    let response = client.read_utxo_events(request).await?;
    let result = response.into_inner();

    println!("Found {} UTxO events for address {} in slot range {}-{}",
        result.events.len(), address, start_slot, end_slot);

    for event in &result.events {
        let event_type = match event.event_type {
            0 => "CREATED",
            1 => "SPENT",
            _ => "UNKNOWN",
        };

        println!("  {} - {}#{} at slot {}",
            event_type,
            hex::encode(&event.tx_hash),
            event.output_index,
            event.slot
        );
    }

    Ok(())
}

// Test 9: Chain tip consistency
#[tokio::test]
#[ignore]
async fn test_live_chain_tip_consistency() -> Result<()> {
    let mut client = connect_client().await?;

    // Get chain tip multiple times
    let mut tips = Vec::new();
    for i in 0..3 {
        let request = Request::new(GetChainTipRequest {});
        let response = client.get_chain_tip(request).await?;
        let tip = response.into_inner();

        println!("Query {}: slot={}, height={}", i, tip.slot, tip.height);
        tips.push(tip);

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    // Tips should be monotonically increasing or equal (if no new blocks)
    for i in 1..tips.len() {
        assert!(
            tips[i].slot >= tips[i-1].slot,
            "Chain tip slot should be monotonically increasing"
        );
        assert!(
            tips[i].height >= tips[i-1].height,
            "Chain tip height should be monotonically increasing"
        );
    }

    Ok(())
}

// Test 10: Empty address query
#[tokio::test]
#[ignore]
async fn test_live_empty_address_query() -> Result<()> {
    let mut client = connect_client().await?;

    // Use a deterministic but likely unused address
    // Create a valid but unused one
    // We'll use a known pattern that's syntactically valid
    use pallas_addresses::{Address, Network, ShelleyAddress, ShelleyPaymentPart, ShelleyDelegationPart};

    // Create a payment address with a payment key hash of all zeros (unlikely to have UTxOs)
    let payment_part = ShelleyPaymentPart::Key(pallas_crypto::hash::Hash::new([0u8; 28]));
    let delegation_part = ShelleyDelegationPart::Null;
    let addr = ShelleyAddress::new(Network::Testnet, payment_part, delegation_part);
    let addr_bytes = Address::Shelley(addr).to_vec();

    let request = Request::new(ReadUtxosRequest {
        addresses: vec![addr_bytes],
    });

    let response = client.read_utxos(request).await?;
    let result = response.into_inner();

    println!("UTxOs for unused address: {}", result.items.len());

    // Should return 0 UTxOs but valid response
    assert!(!result.ledger_tip.is_empty(), "Should return valid ledger tip");

    Ok(())
}

// Test 11: Concurrent requests
#[tokio::test]
#[ignore]
async fn test_live_concurrent_requests() -> Result<()> {
    // Spawn multiple concurrent requests
    let mut handles = Vec::new();

    for i in 0..10 {
        let handle = tokio::spawn(async move {
            let mut client = connect_client().await?;
            let request = Request::new(GetChainTipRequest {});
            let response = client.get_chain_tip(request).await?;
            let tip = response.into_inner();

            println!("Concurrent request {}: slot={}", i, tip.slot);

            Ok::<_, anyhow::Error>(tip.slot)
        });
        handles.push(handle);
    }

    // Wait for all requests
    let mut slots = Vec::new();
    for handle in handles {
        let slot = handle.await??;
        slots.push(slot);
    }

    println!("Received {} concurrent responses", slots.len());
    assert_eq!(slots.len(), 10, "All concurrent requests should succeed");

    // All slots should be very close to each other (within a few blocks)
    let min_slot = *slots.iter().min().unwrap();
    let max_slot = *slots.iter().max().unwrap();

    println!("Slot range: {} - {}", min_slot, max_slot);
    assert!(max_slot - min_slot < 100, "Slots should be within 100 of each other");

    Ok(())
}

// Test 12: Large response handling
#[tokio::test]
#[ignore]
async fn test_live_large_response() -> Result<()> {
    let mut client = connect_client().await?;

    // Get chain tip to find a valid range
    let tip_request = Request::new(GetChainTipRequest {});
    let tip_response = client.get_chain_tip(tip_request).await?;
    let tip = tip_response.into_inner();

    // Query a large slot range (will be limited by max_events)
    let end_slot = tip.slot;
    let start_slot = end_slot.saturating_sub(10000); // Large range

    let request = Request::new(ReadUtxoEventsRequest {
        start_slot,
        end_slot,
        addresses: vec![],
        max_events: 1000, // Request many events
    });

    let response = client.read_utxo_events(request).await?;
    let result = response.into_inner();

    println!("Large query returned {} events", result.events.len());

    // Should respect max limit
    assert!(result.events.len() <= 1000, "Should respect max_events limit");

    Ok(())
}

// Test 13: Wallet-specific test using config data
#[tokio::test]
#[ignore]
async fn test_live_wallet_tracking() -> Result<()> {
    let mut client = connect_client().await?;

    // Use the wallet from config
    // acct_xvk10lmrmwnjep30tt0k8dy33a75vr90qjk9jvejk8atp6m3yyrp00mzcv4x3g4sq9wn50hpfjgq2a5a6qlnx84dpt086z08wlwva5n6ahs4x2g7w
    // This is an account extended public key - we'd need to derive addresses from it
    // For now, use the known address from config

    let address = "addr_test1wplxjzranravtp574s2wz00md7vz9rzpucu252je68u9a8qzjheng";

    use pallas_addresses::Address;
    let addr_bytes = Address::from_bech32(address)
        .expect("Failed to decode address")
        .to_vec();

    // Get UTxOs
    let utxo_request = Request::new(ReadUtxosRequest {
        addresses: vec![addr_bytes.clone()],
    });
    let utxo_response = client.read_utxos(utxo_request).await?;
    let utxos = utxo_response.into_inner();

    // Get transaction history
    let tx_request = Request::new(GetTxHistoryRequest {
        address: addr_bytes,
        max_txs: 50,
    });
    let tx_response = client.get_tx_history(tx_request).await?;
    let history = tx_response.into_inner();

    println!("Wallet {} has:", address);
    println!("  {} UTxOs", utxos.items.len());
    println!("  {} transactions in history", history.tx_hashes.len());

    // Calculate total balance
    let total_ada: u64 = utxos.items.iter().map(|u| u.amount).sum();
    println!("  Total balance: {} ADA", total_ada as f64 / 1_000_000.0);

    Ok(())
}

// Test 14: Token tracking
#[tokio::test]
#[ignore]
async fn test_live_token_tracking() -> Result<()> {
    let mut client = connect_client().await?;

    let policy_id = "250c2ff62ed5ca195009a3557966febf05f14d60f1a52761b547f3da";

    let request = Request::new(SearchUtxosRequest {
        pattern: policy_id.to_string(),
    });

    let response = client.search_utxos(request).await?;
    let result = response.into_inner();

    println!("Token policy {} appears in {} UTxOs", policy_id, result.items.len());

    // Aggregate token amounts
    let mut total_by_asset: std::collections::HashMap<String, u64> = std::collections::HashMap::new();

    for utxo in &result.items {
        for asset in &utxo.assets {
            if hex::encode(&asset.policy_id) == policy_id {
                let asset_key = format!("{}.{}",
                    hex::encode(&asset.policy_id),
                    hex::encode(&asset.asset_name)
                );
                *total_by_asset.entry(asset_key).or_insert(0) += asset.amount;
            }
        }
    }

    println!("Token distribution:");
    for (asset, amount) in total_by_asset {
        println!("  {}: {}", asset, amount);
    }

    Ok(())
}

// Test 15: Error handling - invalid address format
#[tokio::test]
#[ignore]
async fn test_live_invalid_requests() -> Result<()> {
    let mut client = connect_client().await?;

    // Test with invalid address bytes (wrong length)
    let invalid_addr = vec![0u8; 10]; // Too short for a valid Cardano address

    let request = Request::new(ReadUtxosRequest {
        addresses: vec![invalid_addr],
    });

    // This should either return empty results or handle gracefully
    let response = client.read_utxos(request).await;

    match response {
        Ok(r) => {
            println!("Invalid address handled gracefully, returned {} UTxOs",
                r.into_inner().items.len());
        }
        Err(e) => {
            println!("Invalid address returned error (expected): {:?}", e);
        }
    }

    Ok(())
}
