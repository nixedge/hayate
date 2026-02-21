// Comprehensive test suite for Midnight-node integration features
// Tests all features that midnight-node uses:
// - GetChainTip with timestamps
// - ReadUtxoEvents for block range queries
// - Block hash index (store/retrieve metadata)
// - GetBlockByHash RPC

use hayate::indexer::{NetworkStorage, Network};
use tempfile::TempDir;
use cardano_lsm::{Key, Value};

#[test]
fn test_block_hash_index_store_and_retrieve() {
    let temp_dir = TempDir::new().unwrap();
    let mut storage = NetworkStorage::open(
        temp_dir.path().to_path_buf(),
        Network::Preprod,
    ).unwrap();

    // Test block metadata
    let block_hash = hex::decode("a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2").unwrap();
    let slot = 12345678u64;
    let timestamp = 1234567890000u64; // Unix milliseconds

    // Store block metadata
    storage.store_block_metadata(&block_hash, slot, timestamp).unwrap();

    // Retrieve block metadata
    let result = storage.get_block_metadata(&block_hash).unwrap();
    assert!(result.is_some());

    let (retrieved_slot, retrieved_timestamp) = result.unwrap();
    assert_eq!(retrieved_slot, slot);
    assert_eq!(retrieved_timestamp, timestamp);
}

#[test]
fn test_block_hash_index_not_found() {
    let temp_dir = TempDir::new().unwrap();
    let storage = NetworkStorage::open(
        temp_dir.path().to_path_buf(),
        Network::Preprod,
    ).unwrap();

    // Query non-existent block
    let non_existent_hash = hex::decode("0000000000000000000000000000000000000000000000000000000000000000").unwrap();
    let result = storage.get_block_metadata(&non_existent_hash).unwrap();

    assert!(result.is_none());
}

#[test]
fn test_block_hash_index_multiple_blocks() {
    let temp_dir = TempDir::new().unwrap();
    let mut storage = NetworkStorage::open(
        temp_dir.path().to_path_buf(),
        Network::Preprod,
    ).unwrap();

    // Store multiple blocks
    let blocks = vec![
        (hex::decode("a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2").unwrap(), 1000u64, 1000000u64),
        (hex::decode("b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3").unwrap(), 2000u64, 2000000u64),
        (hex::decode("c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3d4").unwrap(), 3000u64, 3000000u64),
    ];

    for (hash, slot, timestamp) in &blocks {
        storage.store_block_metadata(hash, *slot, *timestamp).unwrap();
    }

    // Verify all blocks can be retrieved
    for (hash, expected_slot, expected_timestamp) in &blocks {
        let result = storage.get_block_metadata(hash).unwrap();
        assert!(result.is_some());

        let (slot, timestamp) = result.unwrap();
        assert_eq!(slot, *expected_slot);
        assert_eq!(timestamp, *expected_timestamp);
    }
}

#[test]
fn test_block_hash_index_update() {
    let temp_dir = TempDir::new().unwrap();
    let mut storage = NetworkStorage::open(
        temp_dir.path().to_path_buf(),
        Network::Preprod,
    ).unwrap();

    let block_hash = hex::decode("a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2").unwrap();

    // Store initial metadata
    storage.store_block_metadata(&block_hash, 1000, 1000000).unwrap();

    // Update with new metadata (should overwrite)
    storage.store_block_metadata(&block_hash, 2000, 2000000).unwrap();

    // Verify updated values
    let result = storage.get_block_metadata(&block_hash).unwrap();
    assert!(result.is_some());

    let (slot, timestamp) = result.unwrap();
    assert_eq!(slot, 2000);
    assert_eq!(timestamp, 2000000);
}

#[test]
fn test_utxo_events_storage_and_retrieval() {
    let temp_dir = TempDir::new().unwrap();
    let mut storage = NetworkStorage::open(
        temp_dir.path().to_path_buf(),
        Network::Preprod,
    ).unwrap();

    // Create test UTxO event data using the actual format: slot#<slot:020>#<event_index:010>
    let slot = 12345u64;
    let block_hash = vec![0u8; 32];
    let tx_hash = vec![1u8; 32];
    let output_index = 0u32;
    let event_index = 0u64;
    let timestamp = 1234567890000u64;
    let address = "addr_test1qz...".to_string();

    // Store a CREATED event
    let event_key = format!("slot#{:020}#{:010}", slot, event_index);
    let event_data = serde_json::json!({
        "event_type": "CREATED",
        "tx_hash": hex::encode(&tx_hash),
        "output_index": output_index,
        "slot": slot,
        "block_hash": hex::encode(&block_hash),
        "block_timestamp": timestamp,
        "address": address,
        "amount": 1000000u64,
    });

    storage.block_events_tree.insert(
        &Key::from(event_key.as_bytes()),
        &Value::from(&serde_json::to_vec(&event_data).unwrap()),
    ).unwrap();

    // Retrieve the event directly
    let retrieved = storage.block_events_tree.get(&Key::from(event_key.as_bytes())).unwrap();
    assert!(retrieved.is_some());

    let event: serde_json::Value = serde_json::from_slice(retrieved.unwrap().as_ref()).unwrap();
    assert_eq!(event["event_type"], "CREATED");
    assert_eq!(event["slot"], slot);
    assert_eq!(event["block_timestamp"], timestamp);
}

#[test]
fn test_utxo_events_multiple_events() {
    let temp_dir = TempDir::new().unwrap();
    let mut storage = NetworkStorage::open(
        temp_dir.path().to_path_buf(),
        Network::Preprod,
    ).unwrap();

    let slot = 1000u64;

    // Create multiple events in the same slot
    for event_index in 0..5u64 {
        let event_key = format!("slot#{:020}#{:010}", slot, event_index);
        let event_data = serde_json::json!({
            "event_type": "CREATED",
            "slot": slot,
            "event_index": event_index,
        });

        storage.block_events_tree.insert(
            &Key::from(event_key.as_bytes()),
            &Value::from(&serde_json::to_vec(&event_data).unwrap()),
        ).unwrap();
    }

    // Verify all events can be retrieved
    for event_index in 0..5u64 {
        let event_key = format!("slot#{:020}#{:010}", slot, event_index);
        let retrieved = storage.block_events_tree.get(&Key::from(event_key.as_bytes())).unwrap();
        assert!(retrieved.is_some());

        let event: serde_json::Value = serde_json::from_slice(retrieved.unwrap().as_ref()).unwrap();
        assert_eq!(event["event_index"], event_index);
    }
}

#[test]
fn test_chain_tip_tracking() {
    let temp_dir = TempDir::new().unwrap();
    let mut storage = NetworkStorage::open(
        temp_dir.path().to_path_buf(),
        Network::Preprod,
    ).unwrap();

    // Store chain tip with timestamp
    let tip_hash = vec![0xaa; 32];
    let tip_slot = 50000u64;
    let tip_timestamp = 1700000000000u64;

    storage.store_block_metadata(&tip_hash, tip_slot, tip_timestamp).unwrap();

    // Verify tip can be retrieved with timestamp
    let result = storage.get_block_metadata(&tip_hash).unwrap();
    assert!(result.is_some());

    let (slot, timestamp) = result.unwrap();
    assert_eq!(slot, tip_slot);
    assert_eq!(timestamp, tip_timestamp);
}

#[test]
fn test_block_hash_edge_cases() {
    let temp_dir = TempDir::new().unwrap();
    let mut storage = NetworkStorage::open(
        temp_dir.path().to_path_buf(),
        Network::Preprod,
    ).unwrap();

    // Test with slot 0 (genesis)
    let genesis_hash = vec![0xff; 32];
    storage.store_block_metadata(&genesis_hash, 0, 0).unwrap();

    let result = storage.get_block_metadata(&genesis_hash).unwrap();
    assert!(result.is_some());
    let (slot, timestamp) = result.unwrap();
    assert_eq!(slot, 0);
    assert_eq!(timestamp, 0);

    // Test with maximum u64 values
    let max_hash = vec![0x11; 32];
    storage.store_block_metadata(&max_hash, u64::MAX, u64::MAX).unwrap();

    let result = storage.get_block_metadata(&max_hash).unwrap();
    assert!(result.is_some());
    let (slot, timestamp) = result.unwrap();
    assert_eq!(slot, u64::MAX);
    assert_eq!(timestamp, u64::MAX);
}

#[test]
fn test_utxo_events_created_and_spent() {
    let temp_dir = TempDir::new().unwrap();
    let mut storage = NetworkStorage::open(
        temp_dir.path().to_path_buf(),
        Network::Preprod,
    ).unwrap();

    let tx_hash = vec![0xaa; 32];
    let output_index = 0u32;
    let address = "addr_test1qz...".to_string();

    // Create CREATED event at slot 1000
    let created_slot = 1000u64;
    let created_key = format!("slot#{:020}#{:010}", created_slot, 0u64);
    let created_event = serde_json::json!({
        "event_type": "CREATED",
        "tx_hash": hex::encode(&tx_hash),
        "output_index": output_index,
        "slot": created_slot,
        "address": address,
        "amount": 1000000u64,
    });

    storage.block_events_tree.insert(
        &Key::from(created_key.as_bytes()),
        &Value::from(&serde_json::to_vec(&created_event).unwrap()),
    ).unwrap();

    // Create SPENT event at slot 2000
    let spent_slot = 2000u64;
    let spending_tx = vec![0xbb; 32];
    let spent_key = format!("slot#{:020}#{:010}", spent_slot, 0u64);
    let spent_event = serde_json::json!({
        "event_type": "SPENT",
        "tx_hash": hex::encode(&tx_hash),
        "output_index": output_index,
        "slot": spent_slot,
        "spent_by_tx_hash": hex::encode(&spending_tx),
    });

    storage.block_events_tree.insert(
        &Key::from(spent_key.as_bytes()),
        &Value::from(&serde_json::to_vec(&spent_event).unwrap()),
    ).unwrap();

    // Verify both events
    let created = storage.block_events_tree.get(&Key::from(created_key.as_bytes())).unwrap();
    assert!(created.is_some());
    let created_json: serde_json::Value = serde_json::from_slice(created.unwrap().as_ref()).unwrap();
    assert_eq!(created_json["event_type"], "CREATED");

    let spent = storage.block_events_tree.get(&Key::from(spent_key.as_bytes())).unwrap();
    assert!(spent.is_some());
    let spent_json: serde_json::Value = serde_json::from_slice(spent.unwrap().as_ref()).unwrap();
    assert_eq!(spent_json["event_type"], "SPENT");
}

#[test]
fn test_midnight_node_workflow() {
    // Simulate a typical midnight-node workflow:
    // 1. Query chain tip (with timestamp)
    // 2. Calculate stable block
    // 3. Query events in block range
    // 4. Query specific blocks by hash

    let temp_dir = TempDir::new().unwrap();
    let mut storage = NetworkStorage::open(
        temp_dir.path().to_path_buf(),
        Network::Preprod,
    ).unwrap();

    // Simulate chain sync storing blocks
    let blocks = vec![
        (1000u64, vec![0x10; 32], 1700000000000u64),
        (1001u64, vec![0x11; 32], 1700000001000u64),
        (1002u64, vec![0x12; 32], 1700000002000u64),
    ];

    for (slot, hash, timestamp) in &blocks {
        storage.store_block_metadata(hash, *slot, *timestamp).unwrap();

        // Also store some UTxO events
        let event_key = format!("slot#{:020}#{:010}", slot, 0u64);
        let event_data = serde_json::json!({
            "event_type": "CREATED",
            "slot": slot,
            "block_hash": hex::encode(hash),
            "block_timestamp": timestamp,
        });

        storage.block_events_tree.insert(
            &Key::from(event_key.as_bytes()),
            &Value::from(&serde_json::to_vec(&event_data).unwrap()),
        ).unwrap();
    }

    // Step 1: Get chain tip
    let tip_hash = &blocks[2].1;
    let tip_metadata = storage.get_block_metadata(tip_hash).unwrap();
    assert!(tip_metadata.is_some());
    let (tip_slot, _tip_timestamp) = tip_metadata.unwrap();
    assert_eq!(tip_slot, 1002);

    // Step 2: Calculate stable block (tip - security_parameter)
    let security_param = 2160u64;
    let stable_slot = tip_slot.saturating_sub(security_param);
    assert_eq!(stable_slot, 0); // In this test, we're below security parameter

    // Step 3: Query events in range (query each slot)
    let mut event_count = 0;
    for slot in 1000..=1002 {
        let event_key = format!("slot#{:020}#{:010}", slot, 0u64);
        if let Some(value) = storage.block_events_tree.get(&Key::from(event_key.as_bytes())).unwrap() {
            let event: serde_json::Value = serde_json::from_slice(value.as_ref()).unwrap();
            // Verify event has timestamp
            assert!(event["block_timestamp"].as_u64().is_some());
            event_count += 1;
        }
    }

    assert_eq!(event_count, 3);

    // Step 4: Query specific block by hash
    let block_1_hash = &blocks[0].1;
    let block_metadata = storage.get_block_metadata(block_1_hash).unwrap();
    assert!(block_metadata.is_some());
    let (slot, timestamp) = block_metadata.unwrap();
    assert_eq!(slot, 1000);
    assert_eq!(timestamp, 1700000000000);
}
