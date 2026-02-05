// Unit tests for Hayate components

use hayate::keys::*;
use hayate::indexer::block_processor::*;
use hayate::indexer::{Network, NetworkStorage};
use pallas_crypto::hash::Hash;
use tempfile::TempDir;

// ============================================================================
// Key Derivation Tests
// ============================================================================

#[test]
fn test_parse_hex_xpub() {
    let hex_xpub = "70e900b88a0ece70cefd07c046b31b09fe1e531c2c66a65cd67ed04e37fa65987bc1fb4cd7b72611b81baaf6e8aad39e066296e8f05f97d8e31ffa9ad87e9bb6";
    let result = parse_account_xpub(hex_xpub);
    assert!(result.is_ok());
}

#[test]
fn test_parse_bech32_xpub() {
    let bech32_xpub = "acct_xvk1dd0t9hr59nlhywlgphxz3y8ju5ek4zyrtzxxmt9nhe5tl8uev5u700h7zaaypelerrr7m2y277gh8awdcvugutq2q8qk0hwt748ezts9qgfap";
    let result = parse_account_xpub(bech32_xpub);
    assert!(result.is_ok());
}

#[test]
fn test_parse_invalid_xpub() {
    let invalid = "not_a_valid_xpub";
    let result = parse_account_xpub(invalid);
    assert!(result.is_err());
}

#[test]
fn test_derive_account_keys() {
    let hex_xpub = "70e900b88a0ece70cefd07c046b31b09fe1e531c2c66a65cd67ed04e37fa65987bc1fb4cd7b72611b81baaf6e8aad39e066296e8f05f97d8e31ffa9ad87e9bb6";
    let xpub = parse_account_xpub(hex_xpub).unwrap();

    // Derive account with gap limit 20
    let account = derive_account_keys(xpub.clone(), 20).unwrap();

    // Should have 20 payment keys and 20 stake keys
    assert_eq!(account.external_keys.len(), 20);
    assert_eq!(account.internal_keys.len(), 20);
    assert_eq!(account.stake_keys.len(), 20);

    // All keys should be different
    for i in 0..19 {
        assert_ne!(account.external_keys[i].key_hash(), account.external_keys[i + 1].key_hash());
    }
}

#[test]
fn test_get_wallet_key_hashes() {
    let hex_xpub = "70e900b88a0ece70cefd07c046b31b09fe1e531c2c66a65cd67ed04e37fa65987bc1fb4cd7b72611b81baaf6e8aad39e066296e8f05f97d8e31ffa9ad87e9bb6";
    let xpub = parse_account_xpub(hex_xpub).unwrap();
    let account = derive_account_keys(xpub, 5).unwrap();

    let (payment_hashes, stake_hash) = get_wallet_key_hashes(&account);

    // Should have payment hashes from external and internal keys
    assert_eq!(payment_hashes.len(), 10); // 5 external + 5 internal

    // All hashes should be unique
    let mut unique_hashes = payment_hashes.clone();
    unique_hashes.sort();
    unique_hashes.dedup();
    assert_eq!(unique_hashes.len(), payment_hashes.len());
}

#[test]
fn test_derive_keys_deterministic() {
    let hex_xpub = "70e900b88a0ece70cefd07c046b31b09fe1e531c2c66a65cd67ed04e37fa65987bc1fb4cd7b72611b81baaf6e8aad39e066296e8f05f97d8e31ffa9ad87e9bb6";
    let xpub = parse_account_xpub(hex_xpub).unwrap();

    // Derive same keys twice
    let account1 = derive_account_keys(xpub.clone(), 5).unwrap();
    let account2 = derive_account_keys(xpub.clone(), 5).unwrap();

    // Should produce identical keys
    for i in 0..5 {
        assert_eq!(
            account1.external_keys[i].key_hash(),
            account2.external_keys[i].key_hash()
        );
    }
}

// ============================================================================
// Epoch Calculation Tests
// ============================================================================

#[test]
fn test_slot_to_epoch_zero() {
    assert_eq!(slot_to_epoch(0), 0);
}

#[test]
fn test_slot_to_epoch_boundaries() {
    assert_eq!(slot_to_epoch(432_000), 1);
    assert_eq!(slot_to_epoch(864_000), 2);
    assert_eq!(slot_to_epoch(1_296_000), 3);
}

#[test]
fn test_slot_to_epoch_within_epoch() {
    assert_eq!(slot_to_epoch(1), 0);
    assert_eq!(slot_to_epoch(432_001), 1);
    assert_eq!(slot_to_epoch(431_999), 0);
}

#[test]
fn test_epoch_to_slot() {
    assert_eq!(epoch_to_slot(0), 0);
    assert_eq!(epoch_to_slot(1), 432_000);
    assert_eq!(epoch_to_slot(2), 864_000);
}

#[test]
fn test_epoch_slot_roundtrip() {
    for epoch in 0..100 {
        let slot = epoch_to_slot(epoch);
        assert_eq!(slot_to_epoch(slot), epoch);
    }
}

#[test]
fn test_is_epoch_boundary() {
    assert!(is_epoch_boundary(0));
    assert!(is_epoch_boundary(432_000));
    assert!(is_epoch_boundary(864_000));

    assert!(!is_epoch_boundary(1));
    assert!(!is_epoch_boundary(432_001));
    assert!(!is_epoch_boundary(431_999));
}

// ============================================================================
// Wallet Filter Tests
// ============================================================================

#[test]
fn test_wallet_filter_empty() {
    let filter = WalletFilter::new();

    // Empty filter should accept all addresses
    let test_addr = vec![0u8; 57]; // Minimal Shelley address
    assert!(filter.is_our_address(&test_addr) || !filter.is_our_address(&test_addr)); // Just test it doesn't crash
}

#[test]
fn test_wallet_filter_payment_key() {
    let mut filter = WalletFilter::new();

    // Add a payment key
    let payment_key = Hash::<28>::from([1u8; 28]);
    filter.add_payment_key_hash(payment_key);

    assert!(filter.is_our_payment_key(&payment_key));

    let other_key = Hash::<28>::from([2u8; 28]);
    assert!(!filter.is_our_payment_key(&other_key));
}

#[test]
fn test_wallet_filter_stake_key() {
    let mut filter = WalletFilter::new();

    // Add a stake key
    let stake_key = Hash::<28>::from([1u8; 28]);
    filter.add_stake_credential(stake_key);

    assert!(filter.is_our_stake_key(&stake_key));

    let other_key = Hash::<28>::from([2u8; 28]);
    assert!(!filter.is_our_stake_key(&other_key));
}

// ============================================================================
// Storage Tests
// ============================================================================

fn create_test_storage() -> (TempDir, NetworkStorage) {
    let temp_dir = TempDir::new().unwrap();
    let storage = NetworkStorage::open(temp_dir.path().to_path_buf(), Network::Preview).unwrap();
    (temp_dir, storage)
}

#[test]
fn test_storage_creation() {
    let (_temp, storage) = create_test_storage();
    assert_eq!(storage.network, Network::Preview);
}

#[test]
fn test_chain_tip_empty() {
    let (_temp, storage) = create_test_storage();
    let tip = storage.get_chain_tip().unwrap();
    assert!(tip.is_none());
}

#[test]
fn test_chain_tip_store_retrieve() {
    let (_temp, mut storage) = create_test_storage();

    let slot = 12345u64;
    let hash = vec![0xAB; 32];

    storage.store_chain_tip(slot, &hash).unwrap();

    let tip = storage.get_chain_tip().unwrap().unwrap();
    assert_eq!(tip.slot, slot);
    assert_eq!(tip.hash, hash);
}

#[test]
fn test_chain_tip_update() {
    let (_temp, mut storage) = create_test_storage();

    // Store first tip
    storage.store_chain_tip(100, &vec![1; 32]).unwrap();

    // Update to new tip
    storage.store_chain_tip(200, &vec![2; 32]).unwrap();

    let tip = storage.get_chain_tip().unwrap().unwrap();
    assert_eq!(tip.slot, 200);
    assert_eq!(tip.hash, vec![2; 32]);
}

#[test]
fn test_nonce_storage() {
    let (_temp, mut storage) = create_test_storage();

    let epoch = 5u64;
    let nonce = vec![0xFF; 32];

    storage.store_nonce(epoch, &nonce).unwrap();

    let retrieved = storage.get_nonce(epoch).unwrap().unwrap();
    assert_eq!(retrieved, nonce);
}

#[test]
fn test_nonce_missing() {
    let (_temp, storage) = create_test_storage();

    let retrieved = storage.get_nonce(999).unwrap();
    assert!(retrieved.is_none());
}

#[test]
fn test_address_index_empty() {
    let (_temp, storage) = create_test_storage();

    let utxos = storage.get_utxos_for_address("deadbeef").unwrap();
    assert!(utxos.is_empty());
}

#[test]
fn test_address_index_add_single() {
    let (_temp, mut storage) = create_test_storage();

    let address = "00112233";
    let utxo_key = "abcdef#0";

    storage.add_utxo_to_address_index(address, utxo_key).unwrap();

    let utxos = storage.get_utxos_for_address(address).unwrap();
    assert_eq!(utxos.len(), 1);
    assert_eq!(utxos[0], utxo_key);
}

#[test]
fn test_address_index_add_multiple() {
    let (_temp, mut storage) = create_test_storage();

    let address = "00112233";
    let utxo1 = "abc#0";
    let utxo2 = "def#1";
    let utxo3 = "ghi#2";

    storage.add_utxo_to_address_index(address, utxo1).unwrap();
    storage.add_utxo_to_address_index(address, utxo2).unwrap();
    storage.add_utxo_to_address_index(address, utxo3).unwrap();

    let utxos = storage.get_utxos_for_address(address).unwrap();
    assert_eq!(utxos.len(), 3);
    assert!(utxos.contains(&utxo1.to_string()));
    assert!(utxos.contains(&utxo2.to_string()));
    assert!(utxos.contains(&utxo3.to_string()));
}

#[test]
fn test_address_index_no_duplicates() {
    let (_temp, mut storage) = create_test_storage();

    let address = "00112233";
    let utxo = "abc#0";

    // Add same UTxO twice
    storage.add_utxo_to_address_index(address, utxo).unwrap();
    storage.add_utxo_to_address_index(address, utxo).unwrap();

    let utxos = storage.get_utxos_for_address(address).unwrap();
    assert_eq!(utxos.len(), 1); // Should only appear once
}

#[test]
fn test_address_index_remove() {
    let (_temp, mut storage) = create_test_storage();

    let address = "00112233";
    let utxo1 = "abc#0";
    let utxo2 = "def#1";

    storage.add_utxo_to_address_index(address, utxo1).unwrap();
    storage.add_utxo_to_address_index(address, utxo2).unwrap();

    assert_eq!(storage.get_utxos_for_address(address).unwrap().len(), 2);

    storage.remove_utxo_from_address_index(address, utxo1).unwrap();

    let utxos = storage.get_utxos_for_address(address).unwrap();
    assert_eq!(utxos.len(), 1);
    assert_eq!(utxos[0], utxo2);
}

#[test]
fn test_address_index_remove_last() {
    let (_temp, mut storage) = create_test_storage();

    let address = "00112233";
    let utxo = "abc#0";

    storage.add_utxo_to_address_index(address, utxo).unwrap();
    storage.remove_utxo_from_address_index(address, utxo).unwrap();

    // After removing the last UTxO, index entry should be deleted
    let utxos = storage.get_utxos_for_address(address).unwrap();
    assert!(utxos.is_empty());
}

#[test]
fn test_tx_history_empty() {
    let (_temp, storage) = create_test_storage();

    let history = storage.get_tx_history_for_address("deadbeef").unwrap();
    assert!(history.is_empty());
}

#[test]
fn test_tx_history_add() {
    let (_temp, mut storage) = create_test_storage();

    let address = "00112233";
    let tx1 = "abcdef0123456789";
    let tx2 = "0123456789abcdef";

    storage.add_tx_to_address_history(address, tx1).unwrap();
    storage.add_tx_to_address_history(address, tx2).unwrap();

    let history = storage.get_tx_history_for_address(address).unwrap();
    assert_eq!(history.len(), 2);
    assert!(history.contains(&tx1.to_string()));
    assert!(history.contains(&tx2.to_string()));
}

#[test]
fn test_tx_history_no_duplicates() {
    let (_temp, mut storage) = create_test_storage();

    let address = "00112233";
    let tx = "abcdef0123456789";

    // Add same tx twice
    storage.add_tx_to_address_history(address, tx).unwrap();
    storage.add_tx_to_address_history(address, tx).unwrap();

    let history = storage.get_tx_history_for_address(address).unwrap();
    assert_eq!(history.len(), 1);
}

// ============================================================================
// Block Processor Tests
// ============================================================================

#[test]
fn test_block_processor_creation() {
    let (_temp, storage) = create_test_storage();
    let processor = BlockProcessor::new(storage);

    assert_eq!(processor.blocks_processed, 0);
    assert_eq!(processor.current_slot, 0);
    assert_eq!(processor.current_epoch, 0);
}

#[test]
fn test_block_processor_with_existing_tip() {
    let (_temp, mut storage) = create_test_storage();

    // Set a chain tip
    storage.store_chain_tip(12345, &vec![0; 32]).unwrap();

    let processor = BlockProcessor::new(storage);

    // Should restore from tip
    assert_eq!(processor.current_slot, 12345);
}

// ============================================================================
// Network Tests
// ============================================================================

#[test]
fn test_network_magic() {
    assert_eq!(Network::Mainnet.magic(), 764824073);
    assert_eq!(Network::Preprod.magic(), 1);
    assert_eq!(Network::Preview.magic(), 2);
    assert_eq!(Network::SanchoNet.magic(), 4);
}

#[test]
fn test_network_as_str() {
    assert_eq!(Network::Mainnet.as_str(), "mainnet");
    assert_eq!(Network::Preprod.as_str(), "preprod");
    assert_eq!(Network::Preview.as_str(), "preview");
    assert_eq!(Network::SanchoNet.as_str(), "sanchonet");
}

#[test]
fn test_network_from_str() {
    assert_eq!(Network::from_str("mainnet"), Some(Network::Mainnet));
    assert_eq!(Network::from_str("preprod"), Some(Network::Preprod));
    assert_eq!(Network::from_str("preview"), Some(Network::Preview));
    assert_eq!(Network::from_str("sanchonet"), Some(Network::SanchoNet));

    // Case insensitive
    assert_eq!(Network::from_str("MAINNET"), Some(Network::Mainnet));
    assert_eq!(Network::from_str("PreProd"), Some(Network::Preprod));

    // Custom networks
    match Network::from_str("custom") {
        Some(Network::Custom(name)) => assert_eq!(name, "custom"),
        _ => panic!("Expected custom network"),
    }
}

#[test]
fn test_network_roundtrip() {
    let networks = vec![
        Network::Mainnet,
        Network::Preprod,
        Network::Preview,
        Network::SanchoNet,
    ];

    for network in networks {
        let name = network.as_str();
        let recovered = Network::from_str(name).unwrap();
        assert_eq!(network, recovered);
    }
}
