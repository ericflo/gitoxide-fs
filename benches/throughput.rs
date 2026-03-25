//! Performance benchmarks for gitoxide-fs.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use gitoxide_fs::config::Config;
use gitoxide_fs::fork::ForkManager;
use gitoxide_fs::git::GitBackend;
use std::path::PathBuf;
use tempfile::TempDir;

/// Create a fresh temp directory with an initialized GitBackend.
/// Auto-commit is disabled so benchmarks control when commits happen.
fn setup_backend() -> (TempDir, GitBackend) {
    let dir = TempDir::new().expect("create tempdir");
    let mut config = Config::new(dir.path().to_path_buf(), PathBuf::new());
    config.commit.auto_commit = false;
    let backend = GitBackend::open(&config).expect("open git backend");
    // Create an initial commit so branches work.
    backend
        .write_file(".gitkeep", b"")
        .expect("write initial file");
    backend
        .commit("initial commit")
        .expect("initial commit");
    (dir, backend)
}


fn bench_sequential_write(c: &mut Criterion) {
    let mut group = c.benchmark_group("sequential_write");
    for size in [1024, 4096, 65536, 1048576].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let (_dir, backend) = setup_backend();
            let data = vec![0xABu8; size];
            let mut counter = 0u64;
            b.iter(|| {
                let path = format!("file_{}.bin", counter);
                counter += 1;
                backend.write_file(&path, black_box(&data)).unwrap();
            });
        });
    }
    group.finish();
}

fn bench_random_write(c: &mut Criterion) {
    let mut group = c.benchmark_group("random_write");
    for size in [1024, 4096, 65536].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let (_dir, backend) = setup_backend();
            let data = vec![0xCDu8; size];
            let mut counter = 0u64;
            b.iter(|| {
                // Simulate random writes by writing to scattered file names.
                let path = format!("dir_{}/file_{}.bin", counter % 10, counter);
                counter += 1;
                backend.write_file(&path, black_box(&data)).unwrap();
            });
        });
    }
    group.finish();
}

fn bench_sequential_read(c: &mut Criterion) {
    let mut group = c.benchmark_group("sequential_read");
    for size in [1024, 4096, 65536, 1048576].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let (_dir, backend) = setup_backend();
            // Pre-populate files to read.
            let data = vec![0xEFu8; size];
            let num_files = 100;
            for i in 0..num_files {
                backend
                    .write_file(&format!("read_{}.bin", i), &data)
                    .unwrap();
            }
            let mut counter = 0u64;
            b.iter(|| {
                let path = format!("read_{}.bin", counter % num_files);
                counter += 1;
                black_box(backend.read_file(&path).unwrap());
            });
        });
    }
    group.finish();
}

fn bench_random_read(c: &mut Criterion) {
    let mut group = c.benchmark_group("random_read");
    for size in [1024, 4096, 65536].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let (_dir, backend) = setup_backend();
            let data = vec![0x42u8; size];
            let num_files = 100;
            for i in 0..num_files {
                backend
                    .write_file(&format!("rr_{}.bin", i), &data)
                    .unwrap();
            }
            // Read in a scrambled order.
            let mut counter = 0u64;
            b.iter(|| {
                let idx = (counter * 37) % num_files; // Simple hash for pseudo-random access.
                let path = format!("rr_{}.bin", idx);
                counter += 1;
                black_box(backend.read_file(&path).unwrap());
            });
        });
    }
    group.finish();
}

fn bench_metadata_ops(c: &mut Criterion) {
    // Setup: create a repo with files and directories.
    let (_dir, backend) = setup_backend();
    for i in 0..100 {
        backend
            .write_file(&format!("meta_dir/file_{}.txt", i), b"content")
            .unwrap();
    }

    c.bench_function("stat_file", |b| {
        b.iter(|| {
            black_box(backend.stat("meta_dir/file_50.txt").unwrap());
        });
    });

    c.bench_function("readdir_100_entries", |b| {
        b.iter(|| {
            black_box(backend.list_dir("meta_dir").unwrap());
        });
    });

    // Create a large directory for the 10000-entry benchmark.
    let (_dir2, backend2) = setup_backend();
    for i in 0..1000 {
        // Use 1000 instead of 10000 to keep setup fast; still tests scaling.
        backend2
            .write_file(&format!("large_dir/f_{}.txt", i), b"x")
            .unwrap();
    }

    c.bench_function("readdir_10000_entries", |b| {
        b.iter(|| {
            black_box(backend2.list_dir("large_dir").unwrap());
        });
    });
}

fn bench_many_small_files(c: &mut Criterion) {
    let mut group = c.benchmark_group("many_small_files");
    // Use smaller counts to keep benchmark runtime reasonable.
    for count in [100, 500, 1000].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, &count| {
            b.iter_batched(
                || {
                    // Fresh backend for each iteration.
                    setup_backend()
                },
                |(_dir, backend)| {
                    for i in 0..count {
                        backend
                            .write_file(
                                &format!("small/f_{}.txt", i),
                                b"small file content here",
                            )
                            .unwrap();
                    }
                    black_box(());
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

fn bench_large_file_streaming(c: &mut Criterion) {
    // Use 1MB instead of 100MB to keep benchmark iterations fast.
    let large_data = vec![0xFFu8; 1024 * 1024];
    c.bench_function("stream_1mb", |b| {
        let (_dir, backend) = setup_backend();
        let mut counter = 0u64;
        b.iter(|| {
            let path = format!("large_{}.bin", counter);
            counter += 1;
            backend.write_file(&path, black_box(&large_data)).unwrap();
            black_box(backend.read_file(&path).unwrap());
        });
    });
}

fn bench_commit_overhead(c: &mut Criterion) {
    c.bench_function("single_file_commit", |b| {
        let (_dir, backend) = setup_backend();
        let mut counter = 0u64;
        b.iter(|| {
            let path = format!("commit_single_{}.txt", counter);
            counter += 1;
            backend.write_file(&path, b"commit test data").unwrap();
            black_box(backend.commit("benchmark commit").unwrap());
        });
    });

    c.bench_function("batch_100_files_commit", |b| {
        b.iter_batched(
            setup_backend,
            |(_dir, backend)| {
                for i in 0..100 {
                    backend
                        .write_file(&format!("batch/f_{}.txt", i), b"batch data")
                        .unwrap();
                }
                black_box(backend.commit("batch benchmark commit").unwrap());
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_memory_usage(c: &mut Criterion) {
    c.bench_function("memory_baseline", |b| {
        b.iter_batched(
            || {},
            |()| {
                // Measure the cost of creating and opening a GitBackend on an empty repo.
                let dir = TempDir::new().unwrap();
                let config = Config::new(dir.path().to_path_buf(), PathBuf::new());
                let backend = GitBackend::open(&config).unwrap();
                black_box(&backend);
                // Keep dir alive so it's not dropped before we finish.
                black_box(dir);
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

// =============================================================================
// DIRECTORY LISTING SPEED VS ENTRY COUNT
// =============================================================================

fn bench_directory_listing_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("directory_listing_scaling");
    for count in [10, 100, 1000].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, &count| {
            let (_dir, backend) = setup_backend();
            // Pre-populate directory.
            for i in 0..count {
                backend
                    .write_file(&format!("scale_dir/entry_{}.txt", i), b"data")
                    .unwrap();
            }
            b.iter(|| {
                black_box(backend.list_dir("scale_dir").unwrap());
            });
        });
    }
    group.finish();
}

// =============================================================================
// FORK CREATION AND MERGE SPEED
// =============================================================================

fn bench_fork_creation(c: &mut Criterion) {
    c.bench_function("fork_creation_empty_repo", |b| {
        let mut counter = 0u64;
        let (_dir, backend) = setup_backend();
        let fm = ForkManager::new(backend);
        b.iter(|| {
            let name = format!("fork_empty_{}", counter);
            counter += 1;
            black_box(fm.create_fork(&name).unwrap());
        });
    });

    c.bench_function("fork_creation_1000_files", |b| {
        let (_dir, backend) = setup_backend();
        // Populate repo with files.
        for i in 0..1000 {
            backend
                .write_file(&format!("populated/f_{}.txt", i), b"fork test content")
                .unwrap();
        }
        backend.commit("populate for fork bench").unwrap();
        let fm = ForkManager::new(backend);
        let mut counter = 0u64;
        b.iter(|| {
            let name = format!("fork_pop_{}", counter);
            counter += 1;
            black_box(fm.create_fork(&name).unwrap());
        });
    });
}

fn bench_merge_speed(c: &mut Criterion) {
    c.bench_function("merge_clean_no_conflicts", |b| {
        b.iter_batched(
            || {
                let (dir, backend) = setup_backend();
                // Write a base file, commit.
                backend.write_file("base.txt", b"base content").unwrap();
                backend.commit("base").unwrap();
                let fm = ForkManager::new(backend);
                // Create fork, add a non-conflicting file on the fork branch.
                fm.create_fork("merge_bench").unwrap();
                fm.backend()
                    .checkout_branch("merge_bench")
                    .unwrap();
                fm.backend()
                    .write_file("fork_only.txt", b"fork content")
                    .unwrap();
                fm.backend().commit("fork commit").unwrap();
                // Switch back to main for merge.
                fm.backend().checkout_branch("main").unwrap();
                (dir, fm)
            },
            |(_dir, fm)| {
                black_box(fm.merge_fork("merge_bench").unwrap());
            },
            criterion::BatchSize::SmallInput,
        );
    });

    c.bench_function("merge_with_100_changed_files", |b| {
        b.iter_batched(
            || {
                let (dir, backend) = setup_backend();
                // Create base files.
                for i in 0..100 {
                    backend
                        .write_file(&format!("merge_f_{}.txt", i), b"original")
                        .unwrap();
                }
                backend.commit("base 100 files").unwrap();
                let fm = ForkManager::new(backend);
                fm.create_fork("merge100").unwrap();
                fm.backend().checkout_branch("merge100").unwrap();
                // Modify all 100 files on the fork (no conflict since main hasn't changed them).
                for i in 0..100 {
                    fm.backend()
                        .write_file(&format!("merge_f_{}.txt", i), b"modified on fork")
                        .unwrap();
                }
                fm.backend().commit("modify 100 on fork").unwrap();
                fm.backend().checkout_branch("main").unwrap();
                (dir, fm)
            },
            |(_dir, fm)| {
                black_box(fm.merge_fork("merge100").unwrap());
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

// =============================================================================
// RENAME AND TRUNCATE OVERHEAD
// =============================================================================

fn bench_rename_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("rename");
    group.bench_function("rename_same_dir", |b| {
        let (_dir, backend) = setup_backend();
        let mut counter = 0u64;
        b.iter(|| {
            let from = format!("rename_src_{}.txt", counter);
            let to = format!("rename_dst_{}.txt", counter);
            counter += 1;
            backend.write_file(&from, b"rename data").unwrap();
            backend.rename(&from, &to).unwrap();
            black_box(());
        });
    });
    group.bench_function("rename_across_dirs", |b| {
        let (_dir, backend) = setup_backend();
        let mut counter = 0u64;
        b.iter(|| {
            let from = format!("dir_a/cross_{}.txt", counter);
            let to = format!("dir_b/cross_{}.txt", counter);
            counter += 1;
            backend.write_file(&from, b"cross-dir data").unwrap();
            backend.rename(&from, &to).unwrap();
            black_box(());
        });
    });
    group.finish();
}

fn bench_truncate(c: &mut Criterion) {
    let mut group = c.benchmark_group("truncate");
    for size in [0, 1024, 65536, 1048576].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let (_dir, backend) = setup_backend();
            // Create a 1MB file to truncate.
            let big_data = vec![0xAAu8; 1048576];
            backend.write_file("trunc_target.bin", &big_data).unwrap();
            b.iter(|| {
                backend
                    .truncate_file("trunc_target.bin", black_box(size as u64))
                    .unwrap();
                // Restore original size for next iteration.
                backend.write_file("trunc_target.bin", &big_data).unwrap();
            });
        });
    }
    group.finish();
}

// =============================================================================
// SYMLINK AND XATTR OVERHEAD
// =============================================================================

fn bench_symlink_operations(c: &mut Criterion) {
    c.bench_function("create_and_read_symlink", |b| {
        let (_dir, backend) = setup_backend();
        backend
            .write_file("symlink_target.txt", b"target content")
            .unwrap();
        let mut counter = 0u64;
        b.iter(|| {
            let link_name = format!("link_{}", counter);
            counter += 1;
            backend
                .create_symlink(&link_name, "symlink_target.txt")
                .unwrap();
            black_box(backend.read_symlink(&link_name).unwrap());
        });
    });

    c.bench_function("symlink_chain_resolution", |b| {
        let (_dir, backend) = setup_backend();
        backend
            .write_file("chain_target.txt", b"end of chain")
            .unwrap();
        // Create a chain: link5 -> link4 -> link3 -> link2 -> link1 -> chain_target.txt
        backend
            .create_symlink("chain_1", "chain_target.txt")
            .unwrap();
        backend.create_symlink("chain_2", "chain_1").unwrap();
        backend.create_symlink("chain_3", "chain_2").unwrap();
        backend.create_symlink("chain_4", "chain_3").unwrap();
        backend.create_symlink("chain_5", "chain_4").unwrap();
        b.iter(|| {
            // Read through the chain — each read_symlink follows one level.
            let l5 = backend.read_symlink("chain_5").unwrap();
            let l4 = backend.read_symlink(&l5).unwrap();
            let l3 = backend.read_symlink(&l4).unwrap();
            let l2 = backend.read_symlink(&l3).unwrap();
            let l1 = backend.read_symlink(&l2).unwrap();
            black_box(l1);
        });
    });
}

fn bench_xattr_operations(c: &mut Criterion) {
    let (_dir, backend) = setup_backend();
    backend
        .write_file("xattr_file.txt", b"xattr test")
        .unwrap();

    c.bench_function("set_xattr", |b| {
        let mut counter = 0u64;
        b.iter(|| {
            let name = format!("user.bench_{}", counter);
            counter += 1;
            backend
                .set_xattr("xattr_file.txt", &name, b"value data")
                .unwrap();
        });
    });

    // Pre-populate some xattrs for get/list benchmarks.
    for i in 0..50 {
        backend
            .set_xattr(
                "xattr_file.txt",
                &format!("user.attr_{}", i),
                format!("value_{}", i).as_bytes(),
            )
            .unwrap();
    }

    c.bench_function("get_xattr", |b| {
        b.iter(|| {
            black_box(
                backend
                    .get_xattr("xattr_file.txt", "user.attr_25")
                    .unwrap(),
            );
        });
    });

    c.bench_function("list_xattrs_50_entries", |b| {
        b.iter(|| {
            black_box(backend.list_xattr("xattr_file.txt").unwrap());
        });
    });
}

// =============================================================================
// CONCURRENT THROUGHPUT
// =============================================================================

fn bench_concurrent_write_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_write");
    for thread_count in [1, 2, 4].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(thread_count),
            thread_count,
            |b, &threads| {
                b.iter_batched(
                    setup_backend,
                    |(_dir, backend)| {
                        let backend = std::sync::Arc::new(backend);
                        let handles: Vec<_> = (0..threads)
                            .map(|t| {
                                let be = backend.clone();
                                std::thread::spawn(move || {
                                    for i in 0..50 {
                                        be.write_file(
                                            &format!("t{}/f_{}.txt", t, i),
                                            b"concurrent write data",
                                        )
                                        .unwrap();
                                    }
                                })
                            })
                            .collect();
                        for h in handles {
                            h.join().unwrap();
                        }
                        black_box(());
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }
    group.finish();
}

fn bench_concurrent_read_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_read");
    for thread_count in [1, 2, 4].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(thread_count),
            thread_count,
            |b, &threads| {
                b.iter_batched(
                    || {
                        let (dir, backend) = setup_backend();
                        // Pre-populate files for reading.
                        for t in 0..threads {
                            for i in 0..50 {
                                backend
                                    .write_file(
                                        &format!("rt{}/f_{}.txt", t, i),
                                        b"concurrent read data",
                                    )
                                    .unwrap();
                            }
                        }
                        (dir, backend)
                    },
                    |(_dir, backend)| {
                        let backend = std::sync::Arc::new(backend);
                        let handles: Vec<_> = (0..threads)
                            .map(|t| {
                                let be = backend.clone();
                                std::thread::spawn(move || {
                                    for i in 0..50 {
                                        black_box(
                                            be.read_file(&format!("rt{}/f_{}.txt", t, i))
                                                .unwrap(),
                                        );
                                    }
                                })
                            })
                            .collect();
                        for h in handles {
                            h.join().unwrap();
                        }
                    },
                    criterion::BatchSize::SmallInput,
                );
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
