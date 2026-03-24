//! Performance benchmarks for gitoxide-fs.

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use std::path::PathBuf;
use tempfile::TempDir;

fn bench_sequential_write(c: &mut Criterion) {
    let mut group = c.benchmark_group("sequential_write");
    for size in [1024, 4096, 65536, 1048576].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter(|| {
                // TODO: Create GitBackend, write sequential data
                let _data = vec![0u8; size];
                black_box(());
            });
        });
    }
    group.finish();
}

fn bench_random_write(c: &mut Criterion) {
    let mut group = c.benchmark_group("random_write");
    for size in [1024, 4096, 65536].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter(|| {
                let _data = vec![0u8; size];
                black_box(());
            });
        });
    }
    group.finish();
}

fn bench_sequential_read(c: &mut Criterion) {
    let mut group = c.benchmark_group("sequential_read");
    for size in [1024, 4096, 65536, 1048576].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter(|| {
                let _data = vec![0u8; size];
                black_box(());
            });
        });
    }
    group.finish();
}

fn bench_random_read(c: &mut Criterion) {
    let mut group = c.benchmark_group("random_read");
    for size in [1024, 4096, 65536].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter(|| {
                let _data = vec![0u8; size];
                black_box(());
            });
        });
    }
    group.finish();
}

fn bench_metadata_ops(c: &mut Criterion) {
    c.bench_function("stat_file", |b| {
        b.iter(|| {
            // TODO: stat a file in the git-backed fs
            black_box(());
        });
    });

    c.bench_function("readdir_100_entries", |b| {
        b.iter(|| {
            // TODO: readdir on a directory with 100 entries
            black_box(());
        });
    });

    c.bench_function("readdir_10000_entries", |b| {
        b.iter(|| {
            // TODO: readdir on a directory with 10000 entries
            black_box(());
        });
    });
}

fn bench_many_small_files(c: &mut Criterion) {
    let mut group = c.benchmark_group("many_small_files");
    for count in [100, 1000, 10000].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, &count| {
            b.iter(|| {
                // TODO: Create `count` small files
                black_box(count);
            });
        });
    }
    group.finish();
}

fn bench_large_file_streaming(c: &mut Criterion) {
    c.bench_function("stream_100mb", |b| {
        b.iter(|| {
            // TODO: Stream a 100MB file through the filesystem
            black_box(());
        });
    });
}

fn bench_commit_overhead(c: &mut Criterion) {
    c.bench_function("single_file_commit", |b| {
        b.iter(|| {
            // TODO: Write one file and commit
            black_box(());
        });
    });

    c.bench_function("batch_100_files_commit", |b| {
        b.iter(|| {
            // TODO: Write 100 files and commit in batch
            black_box(());
        });
    });
}

fn bench_memory_usage(c: &mut Criterion) {
    c.bench_function("memory_baseline", |b| {
        b.iter(|| {
            // TODO: Measure memory after mounting an empty repo
            black_box(());
        });
    });
}

criterion_group!(
    benches,
    bench_sequential_write,
    bench_random_write,
    bench_sequential_read,
    bench_random_read,
    bench_metadata_ops,
    bench_many_small_files,
    bench_large_file_streaming,
    bench_commit_overhead,
    bench_memory_usage,
);
criterion_main!(benches);
