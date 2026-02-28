// Property-based tests using proptest
// Tests invariants and properties that should hold for all inputs
// Target: 12 property tests covering key data structures and operations

mod common;

use common::*;
use hayate::snapshot_manager::SnapshotManager;
use hayate::indexer::Network;
use proptest::prelude::*;

// Property 1: Snapshot naming and parsing roundtrip
proptest! {
    #[test]
    fn prop_snapshot_name_roundtrip(slot in 0u64..1_000_000_000u64) {
        let name = SnapshotManager::snapshot_name(slot);
        let parsed = SnapshotManager::parse_snapshot_slot(&name);
        prop_assert_eq!(parsed, Some(slot));
    }
}

// Property 2: Snapshot names are lexicographically sortable
proptest! {
    #[test]
    fn prop_snapshot_names_sortable(slot1 in 0u64..1_000_000_000u64, slot2 in 0u64..1_000_000_000u64) {
        let name1 = SnapshotManager::snapshot_name(slot1);
        let name2 = SnapshotManager::snapshot_name(slot2);

        // String comparison should match numeric comparison
        prop_assert_eq!(name1.cmp(&name2), slot1.cmp(&slot2));
    }
}

// Property 3: Mock address generation is deterministic
proptest! {
    #[test]
    fn prop_mock_address_deterministic(index in 0u32..1000u32) {
        let addr1 = generate_mock_address(Network::Preview, index);
        let addr2 = generate_mock_address(Network::Preview, index);
        prop_assert_eq!(addr1, addr2);
    }
}

// Property 4: Mock address has correct length (29 bytes = 58 hex chars)
proptest! {
    #[test]
    fn prop_mock_address_length(index in 0u32..1000u32) {
        let addr = generate_mock_address(Network::Preview, index);
        prop_assert_eq!(addr.len(), 58); // 29 bytes * 2 hex chars
    }
}

// Property 5: Mock tx hash generation is deterministic
proptest! {
    #[test]
    fn prop_mock_tx_hash_deterministic(index in 0u32..1000u32) {
        let hash1 = generate_mock_tx_hash(index);
        let hash2 = generate_mock_tx_hash(index);
        prop_assert_eq!(hash1, hash2);
    }
}

// Property 6: Mock tx hash has correct length (32 bytes = 64 hex chars)
proptest! {
    #[test]
    fn prop_mock_tx_hash_length(index in 0u32..1000u32) {
        let hash = generate_mock_tx_hash(index);
        prop_assert_eq!(hash.len(), 64); // 32 bytes * 2 hex chars
    }
}

// Property 7: UTxO key format is consistent (tx_hash#output_index)
proptest! {
    #[test]
    fn prop_utxo_key_format(tx_index in 0u32..1000u32, output_index in 0u32..100u32) {
        let key = generate_mock_utxo_key(tx_index, output_index);
        prop_assert!(key.contains("#"));

        let parts: Vec<&str> = key.split('#').collect();
        prop_assert_eq!(parts.len(), 2);
        prop_assert_eq!(parts[0].len(), 64); // tx hash is 64 hex chars
        prop_assert_eq!(parts[1], output_index.to_string());
    }
}

// Property 8: Same index always produces same address (idempotence)
proptest! {
    #[test]
    fn prop_address_generation_idempotent(index in 0u32..1000u32, iterations in 2usize..5usize) {
        let first_addr = generate_mock_address(Network::Preview, index);

        // Generate multiple times and verify they're all the same
        for _ in 0..iterations {
            let addr = generate_mock_address(Network::Preview, index);
            prop_assert_eq!(&addr, &first_addr);
        }
    }
}

// Property 9: Different indices produce different tx hashes
proptest! {
    #[test]
    fn prop_different_indices_different_hashes(index1 in 0u32..1000u32, index2 in 0u32..1000u32) {
        prop_assume!(index1 != index2);

        let hash1 = generate_mock_tx_hash(index1);
        let hash2 = generate_mock_tx_hash(index2);
        prop_assert_ne!(hash1, hash2);
    }
}

// Property 10: Mock UTxO data encodes amount correctly
proptest! {
    #[test]
    fn prop_utxo_data_encoding(amount in 1u64..1_000_000_000_000u64) {
        let addr = generate_mock_address(Network::Preview, 1);
        let data = generate_mock_utxo_data(&addr, amount);

        // Data should contain both address and amount
        prop_assert!(data.len() >= 29 + 8); // 29 bytes address + 8 bytes u64

        // Last 8 bytes should be the amount in little-endian
        let amount_bytes = &data[data.len()-8..];
        let decoded_amount = u64::from_le_bytes(amount_bytes.try_into().unwrap());
        prop_assert_eq!(decoded_amount, amount);
    }
}

// Property 11: Hex encoding is reversible for valid hex strings
proptest! {
    #[test]
    fn prop_hex_roundtrip(bytes in prop::collection::vec(any::<u8>(), 0..100)) {
        let encoded = hex::encode(&bytes);
        let decoded = hex::decode(&encoded).unwrap();
        prop_assert_eq!(decoded, bytes);
    }
}

// Property 12: Snapshot manager state consistency
proptest! {
    #[test]
    fn prop_snapshot_manager_state(
        tip_threshold in 50u64..200u64,
        tip_interval in 60u64..600u64,
        bulk_interval in 300u64..1200u64,
        max_snapshots in 5usize..20usize,
    ) {
        let manager = SnapshotManager::new(tip_threshold, tip_interval, bulk_interval, max_snapshots);

        // Should not snapshot immediately after creation
        prop_assert!(!manager.should_snapshot(1000, 10, 2000));
    }
}

// Property 13: Network addresses have correct network byte
proptest! {
    #[test]
    fn prop_network_byte_correct(index in 0u32..1000u32) {
        let mainnet_addr = generate_mock_address(Network::Mainnet, index);
        let preview_addr = generate_mock_address(Network::Preview, index);
        let preprod_addr = generate_mock_address(Network::Preprod, index);

        // Mainnet addresses start with 01 (network byte 0x01)
        prop_assert!(mainnet_addr.starts_with("01"));

        // Testnet addresses start with 00 (network byte 0x00)
        prop_assert!(preview_addr.starts_with("00"));
        prop_assert!(preprod_addr.starts_with("00"));
    }
}

// Property 14: Slot numbers in snapshot names are zero-padded to 20 digits
proptest! {
    #[test]
    fn prop_snapshot_name_padding(slot in 0u64..u64::MAX) {
        let name = SnapshotManager::snapshot_name(slot);
        prop_assert_eq!(name.len(), 25); // "slot-" (5) + 20 digits
        prop_assert!(name.starts_with("slot-"));

        // All characters after "slot-" should be digits
        let digits = &name[5..];
        prop_assert!(digits.chars().all(|c| c.is_ascii_digit()));
    }
}

// Property 15: UTxO keys from same tx_index have consistent tx_hash prefix
proptest! {
    #[test]
    fn prop_utxo_keys_same_tx_consistent(tx_index in 0u32..1000u32, out1 in 0u32..10u32, out2 in 0u32..10u32) {
        let key1 = generate_mock_utxo_key(tx_index, out1);
        let key2 = generate_mock_utxo_key(tx_index, out2);

        let tx_hash1 = key1.split('#').next().unwrap();
        let tx_hash2 = key2.split('#').next().unwrap();

        // Same tx_index should produce same tx_hash
        prop_assert_eq!(tx_hash1, tx_hash2);
    }
}

// Property 16: Mock addresses are valid hex strings
proptest! {
    #[test]
    fn prop_addresses_valid_hex(index in 0u32..1000u32) {
        let addr = generate_mock_address(Network::Preview, index);

        // Should be valid hex (decodable)
        let result = hex::decode(&addr);
        prop_assert!(result.is_ok());

        // Decoded length should be 29 bytes
        prop_assert_eq!(result.unwrap().len(), 29);
    }
}

// Property 17: Mock tx hashes are valid hex strings
proptest! {
    #[test]
    fn prop_tx_hashes_valid_hex(index in 0u32..1000u32) {
        let hash = generate_mock_tx_hash(index);

        // Should be valid hex (decodable)
        let result = hex::decode(&hash);
        prop_assert!(result.is_ok());

        // Decoded length should be 32 bytes
        prop_assert_eq!(result.unwrap().len(), 32);
    }
}

// Property 18: Snapshot parsing rejects invalid formats
proptest! {
    #[test]
    fn prop_snapshot_parsing_rejects_invalid(invalid_suffix in "[a-z]{1,10}") {
        let invalid_name = format!("slot-{}", invalid_suffix);
        let result = SnapshotManager::parse_snapshot_slot(&invalid_name);
        prop_assert_eq!(result, None);
    }
}
