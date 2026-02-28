// Disk I/O Performance Benchmarks
// Direct LSM storage layer benchmarks to measure raw disk performance

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput, BatchSize};
use cardano_lsm::{LsmTree, LsmConfig, Key, Value};
use tempfile::TempDir;

// Helper to generate test data
fn generate_key(index: u32) -> Vec<u8> {
    format!("key_{:010}", index).into_bytes()
}

fn generate_value(size: usize, index: u32) -> Vec<u8> {
    let base = format!("value_{:010}_", index);
    let mut value = base.into_bytes();
    value.resize(size, b'x');
    value
}

// Benchmark: Raw sequential writes
fn bench_sequential_writes(c: &mut Criterion) {
    let mut group = c.benchmark_group("disk_sequential_writes");

    for value_size in [100, 1000, 10_000].iter() {
        group.throughput(Throughput::Bytes(*value_size as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(value_size),
            value_size,
            |b, &size| {
                b.iter_batched(
                    || {
                        let temp = TempDir::new().unwrap();
                        let config = LsmConfig::default();
                        let tree = LsmTree::open(temp.path(), config).unwrap();
                        (temp, tree)
                    },
                    |(temp, mut tree)| {
                        let key_bytes = generate_key(0);
                        let value_bytes = generate_value(size, 0);
                        let key = Key::from(black_box(&key_bytes));
                        let value = Value::from(black_box(&value_bytes));
                        tree.insert(&key, &value).unwrap();
                        drop(tree);
                        drop(temp);
                    },
                    BatchSize::SmallInput
                );
            }
        );
    }

    group.finish();
}

// Benchmark: Raw sequential reads
fn bench_sequential_reads(c: &mut Criterion) {
    let mut group = c.benchmark_group("disk_sequential_reads");

    for value_size in [100, 1000, 10_000].iter() {
        group.throughput(Throughput::Bytes(*value_size as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(value_size),
            value_size,
            |b, &size| {
                b.iter_batched(
                    || {
                        let temp = TempDir::new().unwrap();
                        let config = LsmConfig::default();
                        let mut tree = LsmTree::open(temp.path(), config).unwrap();

                        // Setup: write data first
                        let key_bytes = generate_key(0);
                        let value_bytes = generate_value(size, 0);
                        let key = Key::from(&key_bytes);
                        let value = Value::from(&value_bytes);
                        tree.insert(&key, &value).unwrap();

                        (temp, tree, key)
                    },
                    |(temp, tree, key)| {
                        let _value = tree.get(black_box(&key)).unwrap();
                        drop(tree);
                        drop(temp);
                    },
                    BatchSize::SmallInput
                );
            }
        );
    }

    group.finish();
}

// Benchmark: Bulk sequential writes
fn bench_bulk_writes(c: &mut Criterion) {
    let mut group = c.benchmark_group("disk_bulk_writes");

    for count in [10, 100, 1000].iter() {
        group.throughput(Throughput::Elements(*count as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(count),
            count,
            |b, &count| {
                b.iter_batched(
                    || {
                        let temp = TempDir::new().unwrap();
                        let config = LsmConfig::default();
                        let tree = LsmTree::open(temp.path(), config).unwrap();
                        (temp, tree)
                    },
                    |(temp, mut tree)| {
                        for i in 0..count {
                            let key_bytes = generate_key(i);
                            let value_bytes = generate_value(1000, i);
                            let key = Key::from(&key_bytes);
                            let value = Value::from(&value_bytes);
                            tree.insert(&key, &value).unwrap();
                        }
                        drop(tree);
                        drop(temp);
                    },
                    BatchSize::SmallInput
                );
            }
        );
    }

    group.finish();
}

// Benchmark: Random access pattern
fn bench_random_reads(c: &mut Criterion) {
    let mut group = c.benchmark_group("disk_random_reads");

    for dataset_size in [100, 1000].iter() {
        group.throughput(Throughput::Elements(*dataset_size as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(dataset_size),
            dataset_size,
            |b, &size| {
                b.iter_batched(
                    || {
                        let temp = TempDir::new().unwrap();
                        let config = LsmConfig::default();
                        let mut tree = LsmTree::open(temp.path(), config).unwrap();

                        // Setup: populate with data
                        for i in 0..size {
                            let key_bytes = generate_key(i);
                            let value_bytes = generate_value(1000, i);
                            let key = Key::from(&key_bytes);
                            let value = Value::from(&value_bytes);
                            tree.insert(&key, &value).unwrap();
                        }

                        (temp, tree)
                    },
                    |(temp, tree)| {
                        // Read in random order
                        for i in (0..10).map(|x| (x * 7) % 100) {
                            let key_bytes = generate_key(i);
                            let key = Key::from(&key_bytes);
                            let _value = tree.get(black_box(&key)).unwrap();
                        }
                        drop(tree);
                        drop(temp);
                    },
                    BatchSize::SmallInput
                );
            }
        );
    }

    group.finish();
}

// Benchmark: Write throughput (sustained)
fn bench_sustained_writes(c: &mut Criterion) {
    let mut group = c.benchmark_group("disk_sustained_writes");
    group.sample_size(10);

    for count in [1000, 5000].iter() {
        group.throughput(Throughput::Elements(*count as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(count),
            count,
            |b, &count| {
                b.iter_batched(
                    || {
                        let temp = TempDir::new().unwrap();
                        let config = LsmConfig::default();
                        let tree = LsmTree::open(temp.path(), config).unwrap();
                        (temp, tree)
                    },
                    |(temp, mut tree)| {
                        for i in 0..count {
                            let key_bytes = generate_key(i);
                            let value_bytes = generate_value(1000, i);
                            let key = Key::from(&key_bytes);
                            let value = Value::from(&value_bytes);
                            tree.insert(&key, &value).unwrap();
                        }
                        drop(tree);
                        drop(temp);
                    },
                    BatchSize::SmallInput
                );
            }
        );
    }

    group.finish();
}

// Benchmark: Mixed read/write workload
fn bench_mixed_workload(c: &mut Criterion) {
    c.bench_function("disk_mixed_workload", |b| {
        b.iter_batched(
            || {
                let temp = TempDir::new().unwrap();
                let config = LsmConfig::default();
                let mut tree = LsmTree::open(temp.path(), config).unwrap();

                // Setup: initial data
                for i in 0..100 {
                    let key_bytes = generate_key(i);
                    let value_bytes = generate_value(1000, i);
                    let key = Key::from(&key_bytes);
                    let value = Value::from(&value_bytes);
                    tree.insert(&key, &value).unwrap();
                }

                (temp, tree)
            },
            |(temp, mut tree)| {
                // Mixed workload: 70% reads, 30% writes
                for i in 0..100 {
                    if i % 10 < 7 {
                        // Read
                        let key_bytes = generate_key(i % 100);
                        let key = Key::from(&key_bytes);
                        let _value = tree.get(&key).unwrap();
                    } else {
                        // Write
                        let key_bytes = generate_key(100 + i);
                        let value_bytes = generate_value(1000, 100 + i);
                        let key = Key::from(&key_bytes);
                        let value = Value::from(&value_bytes);
                        tree.insert(&key, &value).unwrap();
                    }
                }
                drop(tree);
                drop(temp);
            },
            BatchSize::SmallInput
        );
    });
}

// Benchmark: Large value writes
fn bench_large_values(c: &mut Criterion) {
    let mut group = c.benchmark_group("disk_large_values");

    for value_size in [10_000, 100_000, 1_000_000].iter() {
        group.throughput(Throughput::Bytes(*value_size as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(value_size),
            value_size,
            |b, &size| {
                b.iter_batched(
                    || {
                        let temp = TempDir::new().unwrap();
                        let config = LsmConfig::default();
                        let tree = LsmTree::open(temp.path(), config).unwrap();
                        (temp, tree)
                    },
                    |(temp, mut tree)| {
                        let key_bytes = generate_key(0);
                        let value_bytes = generate_value(size, 0);
                        let key = Key::from(black_box(&key_bytes));
                        let value = Value::from(black_box(&value_bytes));
                        tree.insert(&key, &value).unwrap();
                        drop(tree);
                        drop(temp);
                    },
                    BatchSize::SmallInput
                );
            }
        );
    }

    group.finish();
}

// Benchmark: Scan performance (iterator)
fn bench_scan_performance(c: &mut Criterion) {
    let mut group = c.benchmark_group("disk_scan_performance");

    for dataset_size in [100, 1000].iter() {
        group.throughput(Throughput::Elements(*dataset_size as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(dataset_size),
            dataset_size,
            |b, &size| {
                b.iter_batched(
                    || {
                        let temp = TempDir::new().unwrap();
                        let config = LsmConfig::default();
                        let mut tree = LsmTree::open(temp.path(), config).unwrap();

                        // Setup: populate with data
                        for i in 0..size {
                            let key_bytes = generate_key(i);
                            let value_bytes = generate_value(1000, i);
                            let key = Key::from(&key_bytes);
                            let value = Value::from(&value_bytes);
                            tree.insert(&key, &value).unwrap();
                        }

                        (temp, tree)
                    },
                    |(temp, tree)| {
                        // Scan all entries
                        let mut count = 0;
                        for (_key, _value) in tree.iter() {
                            count += 1;
                        }
                        black_box(count);
                        drop(tree);
                        drop(temp);
                    },
                    BatchSize::SmallInput
                );
            }
        );
    }

    group.finish();
}

// Benchmark: Delete performance
fn bench_delete_performance(c: &mut Criterion) {
    c.bench_function("disk_delete_performance", |b| {
        b.iter_batched(
            || {
                let temp = TempDir::new().unwrap();
                let config = LsmConfig::default();
                let mut tree = LsmTree::open(temp.path(), config).unwrap();

                // Setup: populate with data
                for i in 0..100 {
                    let key_bytes = generate_key(i);
                    let value_bytes = generate_value(1000, i);
                    let key = Key::from(&key_bytes);
                    let value = Value::from(&value_bytes);
                    tree.insert(&key, &value).unwrap();
                }

                (temp, tree)
            },
            |(temp, mut tree)| {
                // Delete half the entries
                for i in 0..50 {
                    let key_bytes = generate_key(i);
                    let key = Key::from(&key_bytes);
                    tree.delete(&key).unwrap();
                }
                drop(tree);
                drop(temp);
            },
            BatchSize::SmallInput
        );
    });
}

// Benchmark: Write amplification test (overwrites)
fn bench_overwrite_performance(c: &mut Criterion) {
    c.bench_function("disk_overwrite_performance", |b| {
        b.iter_batched(
            || {
                let temp = TempDir::new().unwrap();
                let config = LsmConfig::default();
                let mut tree = LsmTree::open(temp.path(), config).unwrap();

                // Setup: initial data
                for i in 0..100 {
                    let key_bytes = generate_key(i);
                    let value_bytes = generate_value(1000, i);
                    let key = Key::from(&key_bytes);
                    let value = Value::from(&value_bytes);
                    tree.insert(&key, &value).unwrap();
                }

                (temp, tree)
            },
            |(temp, mut tree)| {
                // Overwrite all entries
                for i in 0..100 {
                    let key_bytes = generate_key(i);
                    let value_bytes = generate_value(1000, i + 1000);
                    let key = Key::from(&key_bytes);
                    let value = Value::from(&value_bytes);
                    tree.insert(&key, &value).unwrap();
                }
                drop(tree);
                drop(temp);
            },
            BatchSize::SmallInput
        );
    });
}

criterion_group!(
    benches,
    bench_sequential_writes,
    bench_sequential_reads,
    bench_bulk_writes,
    bench_random_reads,
    bench_sustained_writes,
    bench_mixed_workload,
    bench_large_values,
    bench_scan_performance,
    bench_delete_performance,
    bench_overwrite_performance,
);

criterion_main!(benches);
