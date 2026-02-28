// Storage Performance Benchmarks
// Measures performance of core storage operations

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use hayate::indexer::{Network, NetworkStorage, StorageManager};
use tempfile::TempDir;
use tokio::runtime::Runtime;

// Helper to create test storage
async fn create_bench_storage() -> (TempDir, hayate::indexer::StorageHandle) {
    let temp = TempDir::new().unwrap();
    let storage = NetworkStorage::open(temp.path().to_path_buf(), Network::Preview).unwrap();
    let (manager, handle) = StorageManager::new(storage);

    tokio::spawn(async move {
        manager.run().await;
    });

    (temp, handle)
}

// Helper to generate mock data
fn generate_mock_address(index: u32) -> String {
    let mut bytes = vec![0x00];
    for i in 0..28 {
        bytes.push(((index * 7 + i * 13) % 256) as u8);
    }
    hex::encode(bytes)
}

fn generate_mock_tx_hash(index: u32) -> String {
    let mut bytes = vec![0u8; 32];
    let index_bytes = index.to_le_bytes();
    bytes[0..4].copy_from_slice(&index_bytes);
    hex::encode(bytes)
}

fn generate_mock_utxo_json(tx_hash: &str, output_index: u32, address: &str, amount: u64) -> Vec<u8> {
    let json = serde_json::json!({
        "tx_hash": tx_hash,
        "output_index": output_index,
        "address": address,
        "amount": amount,
        "assets": {}
    });
    serde_json::to_vec(&json).unwrap()
}

// Benchmark: Single UTxO insertion
fn bench_utxo_insert_single(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("utxo_insert_single", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (_temp, handle) = create_bench_storage().await;

                let addr = generate_mock_address(1);
                let tx_hash = generate_mock_tx_hash(1);
                let utxo_key = format!("{}#0", tx_hash);
                let utxo_data = generate_mock_utxo_json(&tx_hash, 0, &addr, 50_000_000);

                handle.insert_utxo(black_box(utxo_key), black_box(utxo_data)).await.unwrap();
            })
        });
    });
}

// Benchmark: Batch UTxO insertion
fn bench_utxo_insert_batch(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("utxo_insert_batch");

    for size in [10, 100].iter() {
        group.throughput(Throughput::Elements(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter(|| {
                rt.block_on(async move {
                    let (_temp, handle) = create_bench_storage().await;

                    for i in 0..size {
                        let addr = generate_mock_address(i);
                        let tx_hash = generate_mock_tx_hash(i);
                        let utxo_key = format!("{}#0", tx_hash);
                        let utxo_data = generate_mock_utxo_json(&tx_hash, 0, &addr, 50_000_000);

                        handle.insert_utxo(utxo_key, utxo_data).await.unwrap();
                    }
                })
            });
        });
    }

    group.finish();
}

// Benchmark: Address indexing
fn bench_address_indexing(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("address_indexing");

    for size in [10, 100].iter() {
        group.throughput(Throughput::Elements(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter(|| {
                rt.block_on(async move {
                    let (_temp, handle) = create_bench_storage().await;

                    for i in 0..size {
                        let addr = generate_mock_address(i);
                        let tx_hash = generate_mock_tx_hash(i);
                        let utxo_key = format!("{}#0", tx_hash);

                        handle.add_utxo_to_address_index(black_box(addr), black_box(utxo_key)).await.unwrap();
                    }
                })
            });
        });
    }

    group.finish();
}

// Benchmark: UTxO retrieval by address
fn bench_utxo_retrieval(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("utxo_retrieval", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (_temp, handle) = create_bench_storage().await;

                // Setup: Insert some UTxOs
                let addr = generate_mock_address(1);
                for i in 0..10 {
                    let tx_hash = generate_mock_tx_hash(i);
                    let utxo_key = format!("{}#0", tx_hash);
                    let utxo_data = generate_mock_utxo_json(&tx_hash, 0, &addr, 50_000_000);

                    handle.insert_utxo(utxo_key.clone(), utxo_data).await.unwrap();
                    handle.add_utxo_to_address_index(addr.clone(), utxo_key).await.unwrap();
                }

                // Benchmark: Retrieve UTxOs for address
                let _utxos: Vec<String> = handle.get_utxos_for_address(black_box(addr)).await.unwrap();
            })
        });
    });
}

// Benchmark: Chain tip updates
fn bench_chain_tip_update(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("chain_tip_update", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (_temp, handle) = create_bench_storage().await;

                handle.store_chain_tip(
                    black_box(1000),
                    black_box(vec![1, 2, 3, 4]),
                    black_box(1234567890)
                ).await.unwrap();
            })
        });
    });
}

// Benchmark: Chain tip retrieval
fn bench_chain_tip_retrieval(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("chain_tip_retrieval", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (_temp, handle) = create_bench_storage().await;

                // Setup: Store a tip
                handle.store_chain_tip(1000, vec![1, 2, 3, 4], 1234567890).await.unwrap();

                // Benchmark: Retrieve tip
                let _tip = handle.get_chain_tip().await.unwrap();
            })
        });
    });
}

// Benchmark: Concurrent operations
fn bench_concurrent_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("concurrent_operations", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (_temp, handle) = create_bench_storage().await;

                // Spawn 10 concurrent tasks
                let mut handles_vec = vec![];
                for i in 0..10 {
                    let handle_clone = handle.clone();
                    let task = tokio::spawn(async move {
                        let addr = generate_mock_address(i);
                        let tx_hash = generate_mock_tx_hash(i);
                        let utxo_key = format!("{}#0", tx_hash);
                        let utxo_data = generate_mock_utxo_json(&tx_hash, 0, &addr, 50_000_000);

                        handle_clone.insert_utxo(utxo_key.clone(), utxo_data).await.unwrap();
                        handle_clone.add_utxo_to_address_index(addr, utxo_key).await.unwrap();
                    });
                    handles_vec.push(task);
                }

                // Wait for all tasks
                for h in handles_vec {
                    h.await.unwrap();
                }
            })
        });
    });
}

// Benchmark: Wallet tip tracking
fn bench_wallet_tip_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("wallet_tip_operations");

    // Store wallet tips
    group.bench_function("store_wallet_tip", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (_temp, handle) = create_bench_storage().await;

                handle.store_wallet_tip(
                    black_box("wallet_1".to_string()),
                    black_box(1000),
                    black_box(vec![1, 2, 3]),
                    black_box(1234567890)
                ).await.unwrap();
            })
        });
    });

    // Get minimum wallet tip
    group.bench_function("get_min_wallet_tip", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (_temp, handle) = create_bench_storage().await;

                // Setup: Store tips for multiple wallets
                for i in 0..5 {
                    handle.store_wallet_tip(
                        format!("wallet_{}", i),
                        (i + 1) * 1000,
                        vec![i as u8],
                        1234567890
                    ).await.unwrap();
                }

                // Benchmark: Get minimum tip
                let wallet_ids = (0..5).map(|i| format!("wallet_{}", i)).collect();
                let _min_tip = handle.get_min_wallet_tip(black_box(wallet_ids)).await.unwrap();
            })
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_utxo_insert_single,
    bench_utxo_insert_batch,
    bench_address_indexing,
    bench_utxo_retrieval,
    bench_chain_tip_update,
    bench_chain_tip_retrieval,
    bench_concurrent_operations,
    bench_wallet_tip_operations,
);

criterion_main!(benches);
