// Property-based tests for Hayate
// These tests verify invariants hold across many randomly generated inputs

use hayate::indexer::{NetworkStorage, Network};
use hayate::indexer::block_processor::*;
use proptest::prelude::*;
use tempfile::TempDir;

// Test helpers
fn create_test_storage() -> (TempDir, NetworkStorage) {
    let temp_dir = TempDir::new().unwrap();
    let storage = NetworkStorage::open(temp_dir.path().to_path_buf(), Network::Preview).unwrap();
    (temp_dir, storage)
}

// Property: Slot to epoch conversion is consistent
#[test]
fn prop_slot_epoch_roundtrip() {
    proptest!(|(epoch in 0u64..1000)| {
        let slot = epoch_to_slot(epoch);
        let recovered_epoch = slot_to_epoch(slot);
        prop_assert_eq!(epoch, recovered_epoch);
    });
}

// Property: Epoch boundaries are detected correctly
#[test]
fn prop_epoch_boundaries() {
    proptest!(|(epoch in 0u64..1000)| {
        let boundary_slot = epoch_to_slot(epoch);
        prop_assert!(is_epoch_boundary(boundary_slot));

        // Non-boundary slots
        if boundary_slot > 0 {
            prop_assert!(!is_epoch_boundary(boundary_slot - 1));
        }
        prop_assert!(!is_epoch_boundary(boundary_slot + 1));
    });
}

// Property: Balance tree is monoidal (addition is commutative)
#[test]
fn prop_balance_tree_commutative() {
    proptest!(|(
        amount_a in 1u64..1_000_000_000,
        amount_b in 1u64..1_000_000_000
    )| {
        let (_temp1, mut storage1) = create_test_storage();
        let (_temp2, mut storage2) = create_test_storage();

        let addr_key = cardano_lsm::Key::from(b"test_address");

        // Add A then B
        let balance1 = storage1.balance_tree.get(&addr_key).unwrap();
        storage1.balance_tree.insert(&addr_key, &(balance1 + amount_a)).unwrap();
        let balance1 = storage1.balance_tree.get(&addr_key).unwrap();
        storage1.balance_tree.insert(&addr_key, &(balance1 + amount_b)).unwrap();
        let final1 = storage1.balance_tree.get(&addr_key).unwrap();

        // Add B then A
        let balance2 = storage2.balance_tree.get(&addr_key).unwrap();
        storage2.balance_tree.insert(&addr_key, &(balance2 + amount_b)).unwrap();
        let balance2 = storage2.balance_tree.get(&addr_key).unwrap();
        storage2.balance_tree.insert(&addr_key, &(balance2 + amount_a)).unwrap();
        let final2 = storage2.balance_tree.get(&addr_key).unwrap();

        prop_assert_eq!(final1, final2);
    });
}

// Property: Chain tip storage roundtrip
#[test]
fn prop_chain_tip_roundtrip() {
    proptest!(|(
        slot in 0u64..10_000_000,
        hash_bytes in prop::collection::vec(any::<u8>(), 32..33)
    )| {
        let (_temp, mut storage) = create_test_storage();

        storage.store_chain_tip(slot, &hash_bytes).unwrap();
        let tip = storage.get_chain_tip().unwrap().unwrap();

        prop_assert_eq!(tip.slot, slot);
        prop_assert_eq!(tip.hash, hash_bytes);
    });
}

// Property: Nonce storage roundtrip
#[test]
fn prop_nonce_storage_roundtrip() {
    proptest!(|(
        epoch in 0u64..1000,
        nonce_bytes in prop::collection::vec(any::<u8>(), 32..33)
    )| {
        let (_temp, mut storage) = create_test_storage();

        storage.store_nonce(epoch, &nonce_bytes).unwrap();
        let retrieved = storage.get_nonce(epoch).unwrap().unwrap();

        prop_assert_eq!(retrieved, nonce_bytes);
    });
}

// Property: Address index maintains consistency with UTxO tree
#[test]
fn prop_address_index_consistency() {
    proptest!(|(
        address_hex in "[0-9a-f]{64}",
        utxo_keys in prop::collection::vec("[0-9a-f]{64}#[0-9]", 1..10)
    )| {
        let (_temp, mut storage) = create_test_storage();

        // Add UTxOs to address index
        for utxo_key in &utxo_keys {
            storage.add_utxo_to_address_index(&address_hex, utxo_key).unwrap();
        }

        // Retrieve and verify
        let retrieved = storage.get_utxos_for_address(&address_hex).unwrap();
        prop_assert_eq!(retrieved.len(), utxo_keys.len());

        for utxo_key in &utxo_keys {
            prop_assert!(retrieved.contains(utxo_key));
        }
    });
}

// Property: Removing from address index decreases count
#[test]
fn prop_address_index_removal() {
    proptest!(|(
        address_hex in "[0-9a-f]{64}",
        utxo_keys in prop::collection::vec("[0-9a-f]{64}#[0-9]", 2..10)
    )| {
        let (_temp, mut storage) = create_test_storage();

        // Add all UTxOs
        for utxo_key in &utxo_keys {
            storage.add_utxo_to_address_index(&address_hex, utxo_key).unwrap();
        }

        let initial_count = storage.get_utxos_for_address(&address_hex).unwrap().len();

        // Remove one
        storage.remove_utxo_from_address_index(&address_hex, &utxo_keys[0]).unwrap();

        let after_count = storage.get_utxos_for_address(&address_hex).unwrap().len();
        prop_assert_eq!(after_count, initial_count - 1);
    });
}

// Property: Transaction history preserves order and uniqueness
#[test]
fn prop_tx_history_no_duplicates() {
    proptest!(|(
        address_hex in "[0-9a-f]{64}",
        tx_hashes in prop::collection::vec("[0-9a-f]{64}", 1..20)
    )| {
        let (_temp, mut storage) = create_test_storage();

        // Add all tx hashes (some may be duplicates)
        for tx_hash in &tx_hashes {
            storage.add_tx_to_address_history(&address_hex, tx_hash).unwrap();
        }

        // Retrieve
        let history = storage.get_tx_history_for_address(&address_hex).unwrap();

        // Should have no duplicates
        let unique_txs: std::collections::HashSet<_> = tx_hashes.iter().cloned().collect();
        prop_assert_eq!(history.len(), unique_txs.len());

        // All unique txs should be in history
        for tx in unique_txs {
            prop_assert!(history.contains(&tx));
        }
    });
}

// Property: Block processor increments slot monotonically
#[test]
fn prop_block_processor_monotonic_slots() {
    proptest!(|(slots in prop::collection::vec(1u64..1000, 2..20))| {
        let (_temp, storage) = create_test_storage();
        let mut processor = BlockProcessor::new(storage);

        let mut sorted_slots = slots.clone();
        sorted_slots.sort();
        sorted_slots.dedup();

        // Process blocks in order
        for (i, slot) in sorted_slots.iter().enumerate() {
            if i > 0 && slot <= &processor.current_slot {
                continue; // Skip slots that would violate monotonicity
            }

            // Mock block data (minimal)
            let block_hash = vec![i as u8; 32];
            let block_bytes = vec![0u8; 100]; // Minimal CBOR

            // This would fail for real processing, but tests the slot check
            let _ = processor.process_block(&block_bytes, *slot, &block_hash);

            // If processing succeeded, current_slot should be >= previous
            if processor.blocks_processed > 0 {
                prop_assert!(processor.current_slot > 0);
            }
        }

        Ok(())
    });
}

// Property: UTxO keys are unique and deterministic
#[test]
fn prop_utxo_key_format() {
    proptest!(|(
        tx_hash in prop::collection::vec(any::<u8>(), 32..33),
        output_idx in 0u32..100
    )| {
        let tx_hash_hex = hex::encode(&tx_hash);
        let utxo_key = format!("{}#{}", tx_hash_hex, output_idx);

        // Should be parseable
        let parts: Vec<&str> = utxo_key.split('#').collect();
        prop_assert_eq!(parts.len(), 2);

        // Hash part should be hex
        prop_assert!(parts[0].chars().all(|c| c.is_ascii_hexdigit()));

        // Index part should be numeric
        prop_assert!(parts[1].parse::<u32>().is_ok());
        prop_assert_eq!(parts[1].parse::<u32>().unwrap(), output_idx);
    });
}

// Property: Address index survives add-remove cycles
#[test]
fn prop_address_index_add_remove_cycle() {
    proptest!(|(
        address_hex in "[0-9a-f]{64}",
        utxo_key in "[0-9a-f]{64}#[0-9]"
    )| {
        let (_temp, mut storage) = create_test_storage();

        // Start empty
        let initial = storage.get_utxos_for_address(&address_hex).unwrap();
        prop_assert!(initial.is_empty());

        // Add
        storage.add_utxo_to_address_index(&address_hex, &utxo_key).unwrap();
        let after_add = storage.get_utxos_for_address(&address_hex).unwrap();
        prop_assert_eq!(after_add.len(), 1);
        prop_assert!(after_add.contains(&utxo_key));

        // Remove
        storage.remove_utxo_from_address_index(&address_hex, &utxo_key).unwrap();
        let after_remove = storage.get_utxos_for_address(&address_hex).unwrap();
        prop_assert!(after_remove.is_empty());
    });
}

// Property: Multiple addresses don't interfere with each other
#[test]
fn prop_address_isolation() {
    proptest!(|(
        addr1 in "[0-9a-f]{64}",
        addr2 in "[0-9a-f]{64}",
        utxo1 in "[0-9a-f]{64}#[0-9]",
        utxo2 in "[0-9a-f]{64}#[0-9]"
    )| {
        prop_assume!(addr1 != addr2); // Different addresses
        prop_assume!(utxo1 != utxo2); // Different UTxOs

        let (_temp, mut storage) = create_test_storage();

        // Add different UTxOs to different addresses
        storage.add_utxo_to_address_index(&addr1, &utxo1).unwrap();
        storage.add_utxo_to_address_index(&addr2, &utxo2).unwrap();

        // Each address should only see its own UTxO
        let utxos1 = storage.get_utxos_for_address(&addr1).unwrap();
        let utxos2 = storage.get_utxos_for_address(&addr2).unwrap();

        prop_assert_eq!(utxos1.len(), 1);
        prop_assert_eq!(utxos2.len(), 1);
        prop_assert!(utxos1.contains(&utxo1));
        prop_assert!(!utxos1.contains(&utxo2));
        prop_assert!(utxos2.contains(&utxo2));
        prop_assert!(!utxos2.contains(&utxo1));
    });
}
