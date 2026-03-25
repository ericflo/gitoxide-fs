//! Performance benchmarks for gitoxide-fs.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
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

// =============================================================================
// NEW BENCHMARKS: DIRECTORY LISTING SPEED VS ENTRY COUNT
// =============================================================================

fn bench_directory_listing_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("directory_listing_scaling");
    for count in [10, 100, 1000, 5000, 10000].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, &count| {
            b.iter(|| {
                // TODO: Create dir with `count` entries, then list_dir
                black_box(count);
            });
        });
    }
    group.finish();
}

// =============================================================================
// NEW BENCHMARKS: FORK CREATION AND MERGE SPEED
// =============================================================================

fn bench_fork_creation(c: &mut Criterion) {
    c.bench_function("fork_creation_empty_repo", |b| {
        b.iter(|| {
            // TODO: Create a fork from an empty repo
            black_box(());
        });
    });

    c.bench_function("fork_creation_1000_files", |b| {
        b.iter(|| {
            // TODO: Create a fork from a repo with 1000 files
            black_box(());
        });
    });
}

fn bench_merge_speed(c: &mut Criterion) {
    c.bench_function("merge_clean_no_conflicts", |b| {
        b.iter(|| {
            // TODO: Merge a fork with no conflicting changes
            black_box(());
        });
    });

    c.bench_function("merge_with_100_changed_files", |b| {
        b.iter(|| {
            // TODO: Merge a fork where 100 files were modified
            black_box(());
        });
    });
}

// =============================================================================
// NEW BENCHMARKS: RENAME AND TRUNCATE OVERHEAD
// =============================================================================

fn bench_rename_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("rename");
    group.bench_function("rename_same_dir", |b| {
        b.iter(|| {
            // TODO: Rename a file within the same directory
            black_box(());
        });
    });
    group.bench_function("rename_across_dirs", |b| {
        b.iter(|| {
            // TODO: Rename (move) a file between directories
            black_box(());
        });
    });
    group.finish();
}

fn bench_truncate(c: &mut Criterion) {
    let mut group = c.benchmark_group("truncate");
    for size in [0, 1024, 65536, 1048576].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter(|| {
                // TODO: Truncate a file to `size` bytes
                black_box(size);
            });
        });
    }
    group.finish();
}

// =============================================================================
// NEW BENCHMARKS: SYMLINK AND XATTR OVERHEAD
// =============================================================================

fn bench_symlink_operations(c: &mut Criterion) {
    c.bench_function("create_and_read_symlink", |b| {
        b.iter(|| {
            // TODO: Create symlink, then read through it
            black_box(());
        });
    });

    c.bench_function("symlink_chain_resolution", |b| {
        b.iter(|| {
            // TODO: Resolve a chain of 5 symlinks
            black_box(());
        });
    });
}

fn bench_xattr_operations(c: &mut Criterion) {
    c.bench_function("set_xattr", |b| {
        b.iter(|| {
            // TODO: Set an xattr on a file
            black_box(());
        });
    });

    c.bench_function("get_xattr", |b| {
        b.iter(|| {
            // TODO: Get an xattr from a file
            black_box(());
        });
    });

    c.bench_function("list_xattrs_50_entries", |b| {
        b.iter(|| {
            // TODO: List xattrs on a file with 50 xattrs
            black_box(());
        });
    });
}

// =============================================================================
// NEW BENCHMARKS: CONCURRENT THROUGHPUT
// =============================================================================

fn bench_concurrent_write_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_write");
    for thread_count in [1, 2, 4, 8, 16].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(thread_count),
            thread_count,
            |b, &threads| {
                b.iter(|| {
                    // TODO: Spawn `threads` threads each writing 100 files
                    black_box(threads);
                });
            },
        );
    }
    group.finish();
}

fn bench_concurrent_read_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_read");
    for thread_count in [1, 2, 4, 8, 16].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(thread_count),
            thread_count,
            |b, &threads| {
                b.iter(|| {
                    // TODO: Spawn `threads` threads each reading 100 files
                    black_box(threads);
                });
            },
        );
    }
    group.finish();
}

criterion_group!(
    benches,
    // Original benchmarks
    bench_sequential_write,
    bench_random_write,
    bench_sequential_read,
    bench_random_read,
    bench_metadata_ops,
    bench_many_small_files,
    bench_large_file_streaming,
    bench_commit_overhead,
    bench_memory_usage,
    // New benchmarks
    bench_directory_listing_scaling,
    bench_fork_creation,
    bench_merge_speed,
    bench_rename_operations,
    bench_truncate,
    bench_symlink_operations,
    bench_xattr_operations,
    bench_concurrent_write_throughput,
    bench_concurrent_read_throughput,
);
criterion_main!(benches);
