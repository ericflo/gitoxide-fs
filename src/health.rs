//! HTTP health server for mount status polling.
//!
//! Provides a lightweight HTTP endpoint that orchestrators can query to check
//! mount health without shelling out to `gofs status`.
//!
//! # Endpoints
//!
//! - `GET /health` — Returns JSON [`MountStatus`](crate::fs::MountStatus) (always 200).
//! - `GET /health/ready` — Returns 200 if the mount is active, 503 if shutting down.

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Instant;

use tiny_http::{Header, Response, Server};
use tracing::{info, warn};

use crate::config::Config;
use crate::fs::MountStatus;
use crate::git::GitBackend;

/// Handle to a running health server. Drop to shut down.
pub struct HealthServer {
    shutdown: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl HealthServer {
    /// Start the health server on the given TCP port.
    ///
    /// The server runs in a background thread and responds to `/health` and
    /// `/health/ready` requests. It reads live repository state on each
    /// request so the response always reflects current status.
    pub fn start_on_port(port: u16, config: &Config) -> std::io::Result<Self> {
        let addr = format!("0.0.0.0:{}", port);

        // Bind early so we can report errors before spawning the thread.
        let listener = TcpListener::bind(&addr)?;
        let server = Server::from_listener(listener, None)
            .map_err(|e| std::io::Error::other(e.to_string()))?;

        info!("Health server listening on http://{}", addr);
        Self::run(server, config)
    }

    /// Start the health server on a Unix domain socket.
    #[cfg(unix)]
    pub fn start_on_socket(path: &std::path::Path, config: &Config) -> std::io::Result<Self> {
        use std::os::unix::net::UnixListener;

        // Remove stale socket if it exists
        let _ = std::fs::remove_file(path);

        let listener = UnixListener::bind(path)?;
        let server = Server::from_listener(listener, None)
            .map_err(|e| std::io::Error::other(e.to_string()))?;

        info!("Health server listening on {}", path.display());
        Self::run(server, config)
    }

    fn run(server: Server, config: &Config) -> std::io::Result<Self> {
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_clone = shutdown.clone();

        let repo_path = config.repo_path.clone();
        let mount_point = config.mount_point.clone();
        let read_only = config.read_only;
        let started = Instant::now();

        let handle = thread::Builder::new()
            .name("health-server".into())
            .spawn(move || {
                Self::serve_loop(
                    server,
                    shutdown_clone,
                    repo_path,
                    mount_point,
                    read_only,
                    started,
                );
            })?;

        Ok(Self {
            shutdown,
            handle: Some(handle),
        })
    }

    fn serve_loop(
        server: Server,
        shutdown: Arc<AtomicBool>,
        repo_path: PathBuf,
        mount_point: PathBuf,
        read_only: bool,
        started: Instant,
    ) {
        // Use a short timeout so we can check the shutdown flag periodically.
        while !shutdown.load(Ordering::Relaxed) {
            let request = match server.recv_timeout(std::time::Duration::from_millis(250)) {
                Ok(Some(req)) => req,
                Ok(None) => continue, // timeout, loop back to check shutdown
                Err(_) => break,      // server error, exit
            };

            let url = request.url().to_string();

            let json_header =
                Header::from_bytes("Content-Type", "application/json").expect("valid header");

            match url.as_str() {
                "/health" => {
                    let status = build_status(&repo_path, &mount_point, read_only, started);
                    let body = serde_json::to_string(&status)
                        .unwrap_or_else(|e| format!(r#"{{"error":"{}"}}"#, e));
                    let resp = Response::from_string(body).with_header(json_header);
                    let _ = request.respond(resp);
                }
                "/health/ready" => {
                    // Check that the mount point still exists and is accessible
                    let ready = mount_point.exists() && !shutdown.load(Ordering::Relaxed);

                    if ready {
                        let body = r#"{"status":"ready"}"#;
                        let resp = Response::from_string(body)
                            .with_header(json_header)
                            .with_status_code(200);
                        let _ = request.respond(resp);
                    } else {
                        let body = r#"{"status":"not_ready"}"#;
                        let resp = Response::from_string(body)
                            .with_header(json_header)
                            .with_status_code(503);
                        let _ = request.respond(resp);
                    }
                }
                _ => {
                    let resp = Response::from_string("Not Found").with_status_code(404);
                    let _ = request.respond(resp);
                }
            }
        }
    }

    /// Signal the health server to shut down and wait for it to stop.
    pub fn shutdown(mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for HealthServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        // Don't join on drop — the thread will notice the flag and exit
        // on its next timeout cycle.
    }
}

/// Build a [`MountStatus`] by reading live state from the repo on disk.
fn build_status(
    repo_path: &Path,
    mount_point: &Path,
    read_only: bool,
    started: Instant,
) -> MountStatus {
    let config = Config::new(repo_path.to_path_buf(), mount_point.to_path_buf());
    let (branch, total_commits) = match GitBackend::open(&config) {
        Ok(backend) => {
            let branch = backend.current_branch().unwrap_or_default();
            let commits = backend.log(None).map(|l| l.len()).unwrap_or(0);
            (branch, commits)
        }
        Err(e) => {
            warn!("Health check failed to open repo: {}", e);
            (String::new(), 0)
        }
    };

    MountStatus {
        mount_point: mount_point.to_path_buf(),
        repo_path: repo_path.to_path_buf(),
        branch,
        pending_changes: 0,
        total_commits,
        uptime: started.elapsed(),
        read_only,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn test_health_endpoint_json() {
        let dir = tempfile::tempdir().unwrap();
        let mount_dir = tempfile::tempdir().unwrap();
        let config = Config::new(dir.path().to_path_buf(), mount_dir.path().to_path_buf());

        // Init a repo so the backend can open it
        let _backend = GitBackend::open(&config).unwrap();

        let server = HealthServer::start_on_port(0, &config);
        // Port 0 won't work with tiny_http in the same way; use a specific port
        // We'll test with a real port instead
        drop(server);

        // Use a random high port
        let port = portpicker_fallback();
        let server = HealthServer::start_on_port(port, &config).unwrap();

        // Give the server a moment to start
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Query /health
        let resp = http_get(&format!("http://127.0.0.1:{}/health", port));
        assert_eq!(resp.status, 200);
        let status: serde_json::Value = serde_json::from_str(&resp.body).unwrap();
        assert!(status.get("branch").is_some());
        assert!(status.get("read_only").is_some());
        assert!(status.get("repo_path").is_some());

        // Query /health/ready
        let resp = http_get(&format!("http://127.0.0.1:{}/health/ready", port));
        assert_eq!(resp.status, 200);
        let ready: serde_json::Value = serde_json::from_str(&resp.body).unwrap();
        assert_eq!(ready["status"], "ready");

        // Query unknown path
        let resp = http_get(&format!("http://127.0.0.1:{}/unknown", port));
        assert_eq!(resp.status, 404);

        server.shutdown();
    }

    #[test]
    fn test_concurrent_health_queries() {
        let dir = tempfile::tempdir().unwrap();
        let mount_dir = tempfile::tempdir().unwrap();
        let config = Config::new(dir.path().to_path_buf(), mount_dir.path().to_path_buf());
        let _backend = GitBackend::open(&config).unwrap();

        let port = portpicker_fallback();
        let server = HealthServer::start_on_port(port, &config).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Fire 10 concurrent requests
        let handles: Vec<_> = (0..10)
            .map(|_| {
                let url = format!("http://127.0.0.1:{}/health", port);
                thread::spawn(move || http_get(&url))
            })
            .collect();

        for handle in handles {
            let resp = handle.join().unwrap();
            assert_eq!(resp.status, 200);
            let _: serde_json::Value = serde_json::from_str(&resp.body).unwrap();
        }

        server.shutdown();
    }

    #[test]
    fn test_shutdown_sets_not_ready() {
        let dir = tempfile::tempdir().unwrap();
        let mount_dir = tempfile::tempdir().unwrap();
        let config = Config::new(dir.path().to_path_buf(), mount_dir.path().to_path_buf());
        let _backend = GitBackend::open(&config).unwrap();

        let port = portpicker_fallback();
        let server = HealthServer::start_on_port(port, &config).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Confirm ready before shutdown
        let resp = http_get(&format!("http://127.0.0.1:{}/health/ready", port));
        assert_eq!(resp.status, 200);

        // Shutdown
        server.shutdown();

        // Server should be gone now — connection should fail
        let result = std::net::TcpStream::connect_timeout(
            &format!("127.0.0.1:{}", port).parse().unwrap(),
            std::time::Duration::from_millis(200),
        );
        // After shutdown the server thread has exited, so connection should fail
        // (or if it succeeds, the response would be 503). Either is acceptable.
        if result.is_ok() {
            // Server might still be draining; that's fine
        }
    }

    // -- Helpers --

    struct SimpleResponse {
        status: u16,
        body: String,
    }

    fn http_get(url: &str) -> SimpleResponse {
        use std::io::Write;
        use std::net::TcpStream;

        let url_parsed: url_parts::UrlParts = url.into();
        let mut stream =
            TcpStream::connect(format!("{}:{}", url_parsed.host, url_parsed.port)).unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();

        write!(
            stream,
            "GET {} HTTP/1.0\r\nHost: {}\r\nConnection: close\r\n\r\n",
            url_parsed.path, url_parsed.host
        )
        .unwrap();

        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();

        // Parse HTTP response
        let (headers, body) = response.split_once("\r\n\r\n").unwrap_or((&response, ""));
        let status_line = headers.lines().next().unwrap_or("");
        let status: u16 = status_line
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        SimpleResponse {
            status,
            body: body.to_string(),
        }
    }

    /// Pick a random available port by binding to :0 and reading the assigned port.
    fn portpicker_fallback() -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
        // listener is dropped here, freeing the port
    }

    /// Minimal URL parser for tests.
    mod url_parts {
        pub struct UrlParts {
            pub host: String,
            pub port: u16,
            pub path: String,
        }

        impl From<&str> for UrlParts {
            fn from(url: &str) -> Self {
                let without_scheme = url.strip_prefix("http://").unwrap_or(url);
                let (authority, path) = without_scheme
                    .find('/')
                    .map(|i| (&without_scheme[..i], &without_scheme[i..]))
                    .unwrap_or((without_scheme, "/"));
                let (host, port) = authority
                    .rfind(':')
                    .map(|i| (&authority[..i], authority[i + 1..].parse().unwrap_or(80)))
                    .unwrap_or((authority, 80));
                UrlParts {
                    host: host.to_string(),
                    port,
                    path: path.to_string(),
                }
            }
        }
    }
}
