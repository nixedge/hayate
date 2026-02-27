// Integration tests for UTxORPC Query Service

use hayate::api::query::{QueryServiceImpl, query::*};
use hayate::api::query::query::query_service_server::QueryService;
use hayate::indexer::{NetworkStorage, Network, ChainTip};
use tempfile::TempDir;
use tonic::Request;

fn create_test_storage() -> (TempDir, NetworkStorage) {
    let temp_dir = TempDir::new().unwrap();
    let mut storage = NetworkStorage::open(temp_dir.path().to_path_buf(), Network::Preview).unwrap();

    // Set up a chain tip
    storage.store_chain_tip(12345, &vec![0xAB; 32]).unwrap();

    (temp_dir, storage)
}

#[tokio::test]
async fn test_get_chain_tip_empty() {
    let temp_dir = TempDir::new().unwrap();
    let storage = NetworkStorage::open(temp_dir.path().to_path_buf(), Network::Preview).unwrap();

    let service = QueryServiceImpl::new(storage);

    let request = Request::new(GetChainTipRequest {});
    let response = service.get_chain_tip(request).await.unwrap();
    let tip = response.into_inner();

    // Empty database should return zeros
    assert_eq!(tip.height, 0);
    assert_eq!(tip.slot, 0);
    assert_eq!(tip.hash.len(), 0);
}

#[tokio::test]
async fn test_get_chain_tip_with_data() {
    let (_temp, storage) = create_test_storage();
    let service = QueryServiceImpl::new(storage);

    let request = Request::new(GetChainTipRequest {});
    let response = service.get_chain_tip(request).await.unwrap();
    let tip = response.into_inner();

    assert_eq!(tip.slot, 12345);
    assert_eq!(tip.hash, vec![0xAB; 32]);
}

#[tokio::test]
async fn test_read_utxos_empty_address() {
    let (_temp, storage) = create_test_storage();
    let service = QueryServiceImpl::new(storage);

    let address = vec![0u8; 57]; // Dummy address
    let request = Request::new(ReadUtxosRequest {
        addresses: vec![address],
    });

    let response = service.read_utxos(request).await.unwrap();
    let result = response.into_inner();

    assert_eq!(result.items.len(), 0);
    assert_eq!(result.ledger_tip, vec![0xAB; 32]);
}

#[tokio::test]
async fn test_read_utxos_with_data() {
    let (_temp, mut storage) = create_test_storage();

    // Add a UTxO to the index
    let address_hex = hex::encode(&vec![0xAA; 57]);
    let utxo_key = "deadbeef#0";

    storage.add_utxo_to_address_index(&address_hex, utxo_key).unwrap();

    // Store the UTxO data
    let utxo_data = serde_json::json!({
        "tx_hash": "deadbeef",
        "output_index": 0,
        "address": address_hex,
        "amount": 1000000,
        "slot": 12345,
        "block_hash": "blockhash",
    });

    use cardano_lsm::{Key, Value};
    let key = Key::from(utxo_key.as_bytes());
    let value = Value::from(&serde_json::to_vec(&utxo_data).unwrap());
    storage.utxo_tree.insert(&key, &value).unwrap();

    let service = QueryServiceImpl::new(storage);

    let address = vec![0xAA; 57];
    let request = Request::new(ReadUtxosRequest {
        addresses: vec![address.clone()],
    });

    let response = service.read_utxos(request).await.unwrap();
    let result = response.into_inner();

    assert_eq!(result.items.len(), 1);
    assert_eq!(result.items[0].output_index, 0);
    assert_eq!(result.items[0].amount, 1000000);
    assert_eq!(result.items[0].address, address);
}

#[tokio::test]
async fn test_read_params() {
    let (_temp, storage) = create_test_storage();
    let service = QueryServiceImpl::new(storage);

    let request = Request::new(ReadParamsRequest {});
    let response = service.read_params(request).await.unwrap();
    let result = response.into_inner();

    assert_eq!(result.slot, 12345);
    assert_eq!(result.hash, vec![0xAB; 32]);
}

#[tokio::test]
async fn test_search_utxos_invalid_pattern() {
    let (_temp, storage) = create_test_storage();
    let service = QueryServiceImpl::new(storage);

    let request = Request::new(SearchUtxosRequest {
        pattern: "not_hex!!!".to_string(),
    });

    let response = service.search_utxos(request).await;
    assert!(response.is_err()); // Should fail with invalid pattern
}

#[tokio::test]
async fn test_search_utxos_hex_pattern() {
    let (_temp, storage) = create_test_storage();
    let service = QueryServiceImpl::new(storage);

    let request = Request::new(SearchUtxosRequest {
        pattern: "00aa".to_string(),
    });

    let response = service.search_utxos(request).await.unwrap();
    let result = response.into_inner();

    // Currently returns empty, but validates pattern
    assert_eq!(result.items.len(), 0);
}

#[tokio::test]
async fn test_get_tx_history_empty() {
    let (_temp, storage) = create_test_storage();
    let service = QueryServiceImpl::new(storage);

    let address = vec![0xBB; 57];
    let request = Request::new(GetTxHistoryRequest {
        address,
        max_txs: 0,
    });

    let response = service.get_tx_history(request).await.unwrap();
    let result = response.into_inner();

    assert_eq!(result.tx_hashes.len(), 0);
}

#[tokio::test]
async fn test_get_tx_history_with_data() {
    let (_temp, mut storage) = create_test_storage();

    let address_hex = hex::encode(&vec![0xCC; 57]);
    let tx1 = "abcd1234";
    let tx2 = "ef567890";

    storage.add_tx_to_address_history(&address_hex, tx1).unwrap();
    storage.add_tx_to_address_history(&address_hex, tx2).unwrap();

    let service = QueryServiceImpl::new(storage);

    let address = vec![0xCC; 57];
    let request = Request::new(GetTxHistoryRequest {
        address,
        max_txs: 0, // 0 = return all
    });

    let response = service.get_tx_history(request).await.unwrap();
    let result = response.into_inner();

    assert_eq!(result.tx_hashes.len(), 2);
}

#[tokio::test]
async fn test_get_tx_history_with_limit() {
    let (_temp, mut storage) = create_test_storage();

    let address_hex = hex::encode(&vec![0xDD; 57]);

    // Add 5 transactions
    for i in 0..5 {
        let tx = format!("tx{}", i);
        storage.add_tx_to_address_history(&address_hex, &tx).unwrap();
    }

    let service = QueryServiceImpl::new(storage);

    let address = vec![0xDD; 57];
    let request = Request::new(GetTxHistoryRequest {
        address,
        max_txs: 3, // Limit to 3
    });

    let response = service.get_tx_history(request).await.unwrap();
    let result = response.into_inner();

    assert_eq!(result.tx_hashes.len(), 3);
}

#[tokio::test]
async fn test_read_multiple_addresses() {
    let (_temp, mut storage) = create_test_storage();

    // Set up UTxOs for two addresses
    let addr1_hex = hex::encode(&vec![0x11; 57]);
    let addr2_hex = hex::encode(&vec![0x22; 57]);

    storage.add_utxo_to_address_index(&addr1_hex, "utxo1#0").unwrap();
    storage.add_utxo_to_address_index(&addr2_hex, "utxo2#0").unwrap();

    // Store UTxO data
    use cardano_lsm::{Key, Value};

    for (utxo_key, addr_hex, amount) in [
        ("utxo1#0", &addr1_hex, 1000000u64),
        ("utxo2#0", &addr2_hex, 2000000u64),
    ] {
        let utxo_data = serde_json::json!({
            "tx_hash": utxo_key.split('#').next().unwrap(),
            "output_index": 0,
            "address": addr_hex,
            "amount": amount,
            "slot": 12345,
            "block_hash": "blockhash",
        });

        let key = Key::from(utxo_key.as_bytes());
        let value = Value::from(&serde_json::to_vec(&utxo_data).unwrap());
        storage.utxo_tree.insert(&key, &value).unwrap();
    }

    let service = QueryServiceImpl::new(storage);

    let request = Request::new(ReadUtxosRequest {
        addresses: vec![vec![0x11; 57], vec![0x22; 57]],
    });

    let response = service.read_utxos(request).await.unwrap();
    let result = response.into_inner();

    assert_eq!(result.items.len(), 2);

    // Check both UTxOs are returned
    let amounts: Vec<u64> = result.items.iter().map(|u| u.amount).collect();
    assert!(amounts.contains(&1000000));
    assert!(amounts.contains(&2000000));
}
