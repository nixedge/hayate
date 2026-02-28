// API Query Service Integration Tests
// Tests for src/api/query.rs (871 LOC)
// Target: Basic API contract testing
// Note: No existing tests in src/api/query.rs

mod common;

use common::*;
use anyhow::Result;
use hayate::api::query::query::query_service_server::QueryService;
use hayate::api::query::QueryServiceImpl;
use hayate::api::query::query::{
    ReadUtxosRequest, SearchUtxosRequest, GetChainTipRequest,
    GetTxHistoryRequest, ReadParamsRequest,
};
use tonic::Request;

// Helper: Setup test API service with sample data
async fn setup_test_api() -> Result<(tempfile::TempDir, QueryServiceImpl)> {
    let (_temp, storage_handle) = create_test_storage(Network::Preview).await?;

    // Populate with test data
    let addr1 = generate_mock_address(Network::Preview, 1);
    let addr2 = generate_mock_address(Network::Preview, 2);

    // Create some UTxOs (using JSON format for API compatibility)
    let tx1 = generate_mock_tx_hash(100);
    let utxo1_key = generate_mock_utxo_key(100, 0);
    let utxo1_data = generate_mock_utxo_json(&tx1, 0, &addr1, 50_000_000); // 50 ADA
    storage_handle.insert_utxo(utxo1_key.clone(), utxo1_data).await?;
    storage_handle.add_utxo_to_address_index(addr1.clone(), utxo1_key).await?;

    let tx2 = generate_mock_tx_hash(101);
    let utxo2_key = generate_mock_utxo_key(101, 0);
    let utxo2_data = generate_mock_utxo_json(&tx2, 0, &addr1, 25_000_000); // 25 ADA
    storage_handle.insert_utxo(utxo2_key.clone(), utxo2_data).await?;
    storage_handle.add_utxo_to_address_index(addr1.clone(), utxo2_key).await?;

    let tx3 = generate_mock_tx_hash(102);
    let utxo3_key = generate_mock_utxo_key(102, 0);
    let utxo3_data = generate_mock_utxo_json(&tx3, 0, &addr2, 100_000_000); // 100 ADA
    storage_handle.insert_utxo(utxo3_key.clone(), utxo3_data).await?;
    storage_handle.add_utxo_to_address_index(addr2.clone(), utxo3_key).await?;

    // Add tx history
    storage_handle.add_tx_to_address_history(addr1.clone(), generate_mock_tx_hash(100)).await?;
    storage_handle.add_tx_to_address_history(addr1.clone(), generate_mock_tx_hash(101)).await?;
    storage_handle.add_tx_to_address_history(addr2, generate_mock_tx_hash(102)).await?;

    // Set chain tip
    storage_handle.store_chain_tip(1000, vec![1, 2, 3, 4], 1234567890).await?;

    let service = QueryServiceImpl::new(storage_handle);

    Ok((_temp, service))
}

// Test 1: ReadUtxos - single address
#[tokio::test]
async fn test_read_utxos_single_address() -> Result<()> {
    let (_temp, service) = setup_test_api().await?;

    let addr1 = generate_mock_address(Network::Preview, 1);
    let addr1_bytes = hex::decode(&addr1)?;

    let request = Request::new(ReadUtxosRequest {
        addresses: vec![addr1_bytes],
    });

    let response = service.read_utxos(request).await?;
    let result = response.into_inner();

    // Address 1 should have 2 UTxOs
    assert_eq!(result.items.len(), 2);

    Ok(())
}

// Test 2: ReadUtxos - multiple addresses
#[tokio::test]
async fn test_read_utxos_multiple_addresses() -> Result<()> {
    let (_temp, service) = setup_test_api().await?;

    let addr1 = generate_mock_address(Network::Preview, 1);
    let addr2 = generate_mock_address(Network::Preview, 2);
    let addr1_bytes = hex::decode(&addr1)?;
    let addr2_bytes = hex::decode(&addr2)?;

    let request = Request::new(ReadUtxosRequest {
        addresses: vec![addr1_bytes, addr2_bytes],
    });

    let response = service.read_utxos(request).await?;
    let result = response.into_inner();

    // Should have 3 UTxOs total (2 from addr1 + 1 from addr2)
    assert_eq!(result.items.len(), 3);

    Ok(())
}

// Test 3: ReadUtxos - empty address list
#[tokio::test]
async fn test_read_utxos_empty() -> Result<()> {
    let (_temp, service) = setup_test_api().await?;

    let request = Request::new(ReadUtxosRequest {
        addresses: vec![],
    });

    let response = service.read_utxos(request).await?;
    let result = response.into_inner();

    assert_eq!(result.items.len(), 0);

    Ok(())
}

// Test 4: ReadUtxos - address with no UTxOs
#[tokio::test]
async fn test_read_utxos_no_utxos() -> Result<()> {
    let (_temp, service) = setup_test_api().await?;

    let addr_empty = generate_mock_address(Network::Preview, 999);
    let addr_bytes = hex::decode(&addr_empty)?;

    let request = Request::new(ReadUtxosRequest {
        addresses: vec![addr_bytes],
    });

    let response = service.read_utxos(request).await?;
    let result = response.into_inner();

    assert_eq!(result.items.len(), 0);

    Ok(())
}

// Test 5: SearchUtxos - wildcard pattern (unimplemented)
#[tokio::test]
async fn test_search_utxos_wildcard() -> Result<()> {
    let (_temp, service) = setup_test_api().await?;

    let request = Request::new(SearchUtxosRequest {
        pattern: "*".to_string(),
    });

    let response = service.search_utxos(request).await?;
    let result = response.into_inner();

    // Wildcard is not implemented, should return empty
    assert_eq!(result.items.len(), 0);

    Ok(())
}

// Test 6: SearchUtxos - invalid pattern
#[tokio::test]
async fn test_search_utxos_invalid_pattern() -> Result<()> {
    let (_temp, service) = setup_test_api().await?;

    let request = Request::new(SearchUtxosRequest {
        pattern: "not-hex!".to_string(),
    });

    let result = service.search_utxos(request).await;

    // Should return error for invalid pattern
    assert!(result.is_err());

    Ok(())
}

// Test 7: GetChainTip - retrieve current tip
#[tokio::test]
async fn test_get_chain_tip() -> Result<()> {
    let (_temp, service) = setup_test_api().await?;

    let request = Request::new(GetChainTipRequest {});

    let response = service.get_chain_tip(request).await?;
    let result = response.into_inner();

    // Verify chain tip fields
    assert_eq!(result.slot, 1000);
    assert_eq!(result.hash, vec![1, 2, 3, 4]);
    assert_eq!(result.timestamp, 1234567890);

    Ok(())
}

// Test 8: GetChainTip - no tip set
#[tokio::test]
async fn test_get_chain_tip_empty() -> Result<()> {
    let (_temp, storage_handle) = create_test_storage(Network::Preview).await?;
    let service = QueryServiceImpl::new(storage_handle);

    let request = Request::new(GetChainTipRequest {});

    let response = service.get_chain_tip(request).await?;
    let result = response.into_inner();

    // No tip set, should return zeros
    assert_eq!(result.slot, 0);
    assert_eq!(result.hash, Vec::<u8>::new());

    Ok(())
}

// Test 9: GetTxHistory - basic query
#[tokio::test]
async fn test_get_tx_history() -> Result<()> {
    let (_temp, service) = setup_test_api().await?;

    let addr1 = generate_mock_address(Network::Preview, 1);
    let addr1_bytes = hex::decode(&addr1)?;

    let request = Request::new(GetTxHistoryRequest {
        address: addr1_bytes,
        max_txs: 10,
    });

    let response = service.get_tx_history(request).await?;
    let result = response.into_inner();

    // Address 1 has 2 transactions
    assert_eq!(result.tx_hashes.len(), 2);

    Ok(())
}

// Test 10: GetTxHistory - with limit
#[tokio::test]
async fn test_get_tx_history_limit() -> Result<()> {
    let (_temp, service) = setup_test_api().await?;

    let addr1 = generate_mock_address(Network::Preview, 1);
    let addr1_bytes = hex::decode(&addr1)?;

    let request = Request::new(GetTxHistoryRequest {
        address: addr1_bytes,
        max_txs: 1, // Only return 1
    });

    let response = service.get_tx_history(request).await?;
    let result = response.into_inner();

    assert_eq!(result.tx_hashes.len(), 1);

    Ok(())
}

// Test 11: GetTxHistory - no history
#[tokio::test]
async fn test_get_tx_history_empty() -> Result<()> {
    let (_temp, service) = setup_test_api().await?;

    let addr_empty = generate_mock_address(Network::Preview, 999);
    let addr_bytes = hex::decode(&addr_empty)?;

    let request = Request::new(GetTxHistoryRequest {
        address: addr_bytes,
        max_txs: 10,
    });

    let response = service.get_tx_history(request).await?;
    let result = response.into_inner();

    assert_eq!(result.tx_hashes.len(), 0);

    Ok(())
}

// Test 12: ReadParams - retrieve protocol parameters
#[tokio::test]
async fn test_read_params() -> Result<()> {
    let (_temp, service) = setup_test_api().await?;

    let request = Request::new(ReadParamsRequest {});

    let response = service.read_params(request).await?;
    let result = response.into_inner();

    // ReadParams returns chain tip info
    assert_eq!(result.slot, 1000);
    assert_eq!(result.hash, vec![1, 2, 3, 4]);

    Ok(())
}

// Test 13: Multiple sequential queries
#[tokio::test]
async fn test_sequential_queries() -> Result<()> {
    let (_temp, service) = setup_test_api().await?;

    let addr1 = generate_mock_address(Network::Preview, 1);
    let addr1_bytes = hex::decode(&addr1)?;

    // Query 1: ReadUtxos
    let req1 = Request::new(ReadUtxosRequest {
        addresses: vec![addr1_bytes.clone()],
    });
    let resp1 = service.read_utxos(req1).await?;
    assert_eq!(resp1.into_inner().items.len(), 2);

    // Query 2: GetTxHistory
    let req2 = Request::new(GetTxHistoryRequest {
        address: addr1_bytes,
        max_txs: 10,
    });
    let resp2 = service.get_tx_history(req2).await?;
    assert_eq!(resp2.into_inner().tx_hashes.len(), 2);

    // Query 3: GetChainTip
    let req3 = Request::new(GetChainTipRequest {});
    let resp3 = service.get_chain_tip(req3).await?;
    assert_eq!(resp3.into_inner().slot, 1000);

    Ok(())
}

// Test 14: Large address batch
#[tokio::test]
async fn test_large_address_batch() -> Result<()> {
    let (_temp, storage_handle) = create_test_storage(Network::Preview).await?;

    // Create 20 addresses with 1 UTxO each
    let mut address_bytes = Vec::new();
    for i in 0..20 {
        let addr = generate_mock_address(Network::Preview, i);
        let tx_hash = generate_mock_tx_hash(i);
        let utxo_key = generate_mock_utxo_key(i, 0);
        let utxo_data = generate_mock_utxo_json(&tx_hash, 0, &addr, 10_000_000);

        storage_handle.insert_utxo(utxo_key.clone(), utxo_data).await?;
        storage_handle.add_utxo_to_address_index(addr.clone(), utxo_key).await?;

        address_bytes.push(hex::decode(&addr)?);
    }

    let service = QueryServiceImpl::new(storage_handle);

    let request = Request::new(ReadUtxosRequest {
        addresses: address_bytes,
    });

    let response = service.read_utxos(request).await?;
    let result = response.into_inner();

    // Should have 20 UTxOs
    assert_eq!(result.items.len(), 20);

    Ok(())
}

// Test 15: Service with empty storage
#[tokio::test]
async fn test_empty_storage() -> Result<()> {
    let (_temp, storage_handle) = create_test_storage(Network::Preview).await?;
    let service = QueryServiceImpl::new(storage_handle);

    let addr = generate_mock_address(Network::Preview, 1);
    let addr_bytes = hex::decode(&addr)?;

    // Test ReadUtxos
    let req1 = Request::new(ReadUtxosRequest {
        addresses: vec![addr_bytes.clone()],
    });
    let resp1 = service.read_utxos(req1).await?;
    assert_eq!(resp1.into_inner().items.len(), 0);

    // Test GetTxHistory
    let req2 = Request::new(GetTxHistoryRequest {
        address: addr_bytes,
        max_txs: 10,
    });
    let resp2 = service.get_tx_history(req2).await?;
    assert_eq!(resp2.into_inner().tx_hashes.len(), 0);

    // Test GetChainTip
    let req3 = Request::new(GetChainTipRequest {});
    let resp3 = service.get_chain_tip(req3).await?;
    assert_eq!(resp3.into_inner().slot, 0);

    Ok(())
}
