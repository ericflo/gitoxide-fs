//! Performance benchmark tests.
//! These verify that the filesystem meets basic performance targets.

use gitoxide_fs::Config;
use std::fs;
use std::time::Instant;
use tempfile::TempDir;

fn test_config() -> (Config, TempDir, TempDir) {
    let repo_dir = TempDir::new().unwrap();
    let mount_dir = TempDir::new().unwrap();
    let config = Config::new(repo_dir.path(), mount_dir.path());
    (config, repo_dir, mount_dir)
}

#[test]
fn test_read_throughput_large_file() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();

    // Write a 10MB file
    let data = vec![0xAAu8; 10_000_000];
    let file_path = mount.path().join("read_bench.bin");
    fs::write(&file_path, &data).unwrap();

    // Benchmark reads
    let start = Instant::now();
    for _ in 0..10 {
        let _ = fs::read(&file_path).unwrap();
    }
    let elapsed = start.elapsed();
    let total_bytes = 10_000_000 * 10;
    let throughput_mbps = (total_bytes as f64) / elapsed.as_secs_f64() / 1_000_000.0;

    eprintln!("Read throughput: {throughput_mbps:.1} MB/s");
    assert!(
        throughput_mbps > 10.0,
        "read throughput should be > 10 MB/s, got {throughput_mbps:.1} MB/s"
    );
    handle.unmount().unwrap();
}

#[test]
fn test_write_throughput_large_file() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();

    let data = vec![0xBBu8; 10_000_000];

    let start = Instant::now();
    for i in 0..10 {
        fs::write(mount.path().join(format!("write_bench_{i}.bin")), &data).unwrap();
    }
    let elapsed = start.elapsed();
    let total_bytes = 10_000_000 * 10;
    let throughput_mbps = (total_bytes as f64) / elapsed.as_secs_f64() / 1_000_000.0;

    eprintln!("Write throughput: {throughput_mbps:.1} MB/s");
    assert!(
        throughput_mbps > 5.0,
        "write throughput should be > 5 MB/s, got {throughput_mbps:.1} MB/s"
    );
    handle.unmount().unwrap();
}

#[test]
fn test_small_file_creation_rate() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();

    let count = 500;
    let start = Instant::now();
    for i in 0..count {
        fs::write(
            mount.path().join(format!("small_{i:04}.txt")),
            format!("content {i}"),
        )
        .unwrap();
    }
    let elapsed = start.elapsed();
    let rate = count as f64 / elapsed.as_secs_f64();

    eprintln!("Small file creation rate: {rate:.0} files/sec");
    assert!(
        rate > 50.0,
        "should create > 50 files/sec, got {rate:.0} files/sec"
    );
    handle.unmount().unwrap();
}

#[test]
fn test_directory_listing_speed() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();

    // Create 1000 files
    for i in 0..1000 {
        fs::write(
            mount.path().join(format!("list_{i:04}.txt")),
            format!("{i}"),
        )
        .unwrap();
    }

    let start = Instant::now();
    for _ in 0..10 {
        let entries: Vec<_> = fs::read_dir(mount.path()).unwrap().collect();
        assert_eq!(entries.len(), 1000);
    }
    let elapsed = start.elapsed();
    let avg_ms = elapsed.as_millis() as f64 / 10.0;

    eprintln!("Directory listing (1000 entries): {avg_ms:.1} ms avg");
    assert!(
        avg_ms < 1000.0,
        "listing 1000 entries should take < 1s, took {avg_ms:.1} ms"
    );
    handle.unmount().unwrap();
}

#[test]
fn test_git_commit_overhead() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();

    let start = Instant::now();
    for i in 0..20 {
        fs::write(
            mount.path().join(format!("overhead_{i}.txt")),
            format!("data {i}"),
        )
        .unwrap();
        handle.flush().unwrap(); // Force commit
    }
    let elapsed = start.elapsed();
    let avg_ms = elapsed.as_millis() as f64 / 20.0;

    eprintln!("Average commit overhead: {avg_ms:.1} ms");
    assert!(
        avg_ms < 500.0,
        "commit overhead should be < 500ms, got {avg_ms:.1} ms"
    );
    handle.unmount().unwrap();
}

#[test]
fn test_memory_usage_under_load() {
    let (config, _repo, mount) = test_config();
    let handle = gitoxide_fs::mount(config).unwrap();

    // Write many files and check we don't blow up
    for i in 0..2000 {
        fs::write(
            mount.path().join(format!("mem_{i:04}.txt")),
            format!("data {i}"),
        )
        .unwrap();
    }
    handle.flush().unwrap();

    // Read all files back
    for i in 0..2000 {
        let _ = fs::read_to_string(mount.path().join(format!("mem_{i:04}.txt"))).unwrap();
    }

    // If we got here without OOM, the test passes
    handle.unmount().unwrap();
}
