// API Performance Benchmarks
// Measures performance of gRPC API queries

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use hayate::api::query::QueryServiceImpl;
use hayate::api::query::query::query_service_server::QueryService;
use hayate::api::query::query::{ReadUtxosRequest, GetChainTipRequest, GetTxHistoryRequest};
use hayate::indexer::{Network, NetworkStorage, StorageManager};
use tempfile::TempDir;
use tokio::runtime::Runtime;
use tonic::Request;

// Helper to create test storage with API service
async fn create_bench_api() -> (TempDir, QueryServiceImpl) {
    let temp = TempDir::new().unwrap();
    let storage = NetworkStorage::open(temp.path().to_path_buf(), Network::Preview).unwrap();
    let (manager, handle) = StorageManager::new(storage);

    tokio::spawn(async move {
        manager.run().await;
    });

    let service = QueryServiceImpl::new(handle);
    (temp, service)
}

// Helper to populate storage with test data
async fn populate_storage(service: &QueryServiceImpl, num_addresses: u32) {
    for i in 0..num_addresses {
        let addr = generate_mock_address(i);
        let tx_hash = generate_mock_tx_hash(i);
        let utxo_key = format!("{}#0", tx_hash);
        let utxo_data = generate_mock_utxo_json(&tx_hash, 0, &addr, 50_000_000);

        service.storage.insert_utxo(utxo_key.clone(), utxo_data).await.unwrap();
        service.storage.add_utxo_to_address_index(addr, utxo_key).await.unwrap();
    }

    service.storage.store_chain_tip(1000, vec![1, 2, 3, 4], 1234567890).await.unwrap();
}

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

// Benchmark: GetChainTip query
fn bench_get_chain_tip(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("api_get_chain_tip", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (_temp, service) = create_bench_api().await;
                populate_storage(&service, 100).await;

                let request = Request::new(GetChainTipRequest {});
                let _response = service.get_chain_tip(black_box(request)).await.unwrap();
            })
        });
    });
}

// Benchmark: ReadUtxos - single address
fn bench_read_utxos_single(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("api_read_utxos_single", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (_temp, service) = create_bench_api().await;
                populate_storage(&service, 100).await;

                let addr = generate_mock_address(50);
                let addr_bytes = hex::decode(&addr).unwrap();

                let request = Request::new(ReadUtxosRequest {
                    addresses: vec![addr_bytes],
                });

                let _response = service.read_utxos(black_box(request)).await.unwrap();
            })
        });
    });
}

// Benchmark: ReadUtxos - multiple addresses
fn bench_read_utxos_batch(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("api_read_utxos_batch");

    for num_addrs in [1, 10, 50].iter() {
        group.throughput(Throughput::Elements(*num_addrs as u64));
        group.bench_with_input(BenchmarkId::from_parameter(num_addrs), num_addrs, |b, &num_addrs| {
            b.iter(|| {
                rt.block_on(async move {
                    let (_temp, service) = create_bench_api().await;
                    populate_storage(&service, 100).await;

                    let addresses: Vec<Vec<u8>> = (0..num_addrs)
                        .map(|i| hex::decode(&generate_mock_address(i)).unwrap())
                        .collect();

                    let request = Request::new(ReadUtxosRequest {
                        addresses,
                    });

                    let _response = service.read_utxos(black_box(request)).await.unwrap();
                })
            });
        });
    }

    group.finish();
}

// Benchmark: GetTxHistory query
fn bench_get_tx_history(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("api_get_tx_history", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (_temp, service) = create_bench_api().await;
                populate_storage(&service, 100).await;

                // Add some transaction history
                let addr = generate_mock_address(50);
                for i in 0..10 {
                    let tx_hash = generate_mock_tx_hash(i);
                    service.storage.add_tx_to_address_history(addr.clone(), tx_hash).await.unwrap();
                }

                let addr_bytes = hex::decode(&addr).unwrap();
                let request = Request::new(GetTxHistoryRequest {
                    address: addr_bytes,
                    max_txs: 10,
                });

                let _response = service.get_tx_history(black_box(request)).await.unwrap();
            })
        });
    });
}

// Benchmark: Empty database queries
fn bench_empty_queries(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("api_empty_query", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (_temp, service) = create_bench_api().await;

                let addr = generate_mock_address(999);
                let addr_bytes = hex::decode(&addr).unwrap();

                let request = Request::new(ReadUtxosRequest {
                    addresses: vec![addr_bytes],
                });

                let _response = service.read_utxos(black_box(request)).await.unwrap();
            })
        });
    });
}

// Benchmark: Sequential API calls
fn bench_sequential_queries(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("api_sequential_queries", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (_temp, service) = create_bench_api().await;
                populate_storage(&service, 100).await;

                // Query 1: ReadUtxos
                let addr = generate_mock_address(50);
                let addr_bytes = hex::decode(&addr).unwrap();
                let req1 = Request::new(ReadUtxosRequest {
                    addresses: vec![addr_bytes.clone()],
                });
                let _resp1 = service.read_utxos(req1).await.unwrap();

                // Query 2: GetTxHistory
                let req2 = Request::new(GetTxHistoryRequest {
                    address: addr_bytes,
                    max_txs: 10,
                });
                let _resp2 = service.get_tx_history(req2).await.unwrap();

                // Query 3: GetChainTip
                let req3 = Request::new(GetChainTipRequest {});
                let _resp3 = service.get_chain_tip(req3).await.unwrap();
            })
        });
    });
}

// Benchmark: Concurrent API queries
fn bench_concurrent_queries(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("api_concurrent_queries", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (_temp, service) = create_bench_api().await;
                populate_storage(&service, 100).await;

                // Spawn 10 concurrent queries
                let mut handles = vec![];
                for i in 0..10 {
                    let service_clone = service.clone();
                    let task = tokio::spawn(async move {
                        let addr = generate_mock_address(i * 10);
                        let addr_bytes = hex::decode(&addr).unwrap();

                        let request = Request::new(ReadUtxosRequest {
                            addresses: vec![addr_bytes],
                        });

                        service_clone.read_utxos(request).await.unwrap();
                    });
                    handles.push(task);
                }

                // Wait for all queries
                for h in handles {
                    h.await.unwrap();
                }
            })
        });
    });
}

criterion_group!(
    benches,
    bench_get_chain_tip,
    bench_read_utxos_single,
    bench_read_utxos_batch,
    bench_get_tx_history,
    bench_empty_queries,
    bench_sequential_queries,
    bench_concurrent_queries,
);

criterion_main!(benches);
