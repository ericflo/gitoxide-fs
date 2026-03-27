//! Content-addressed blob storage for large files.

use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::{Error, Result};

const POINTER_MAGIC: &str = "gitoxide-fs-pointer v1";

/// Parsed metadata for a pointer file stored in git.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PointerFile {
    /// SHA-256 hash of the blob content.
    pub sha256: String,
    /// Original content size in bytes.
    pub size: u64,
    /// Original filename at pointer creation time.
    pub original: String,
}

impl PointerFile {
    /// Serialize this pointer file into the on-disk/git format.
    pub fn to_bytes(&self) -> Vec<u8> {
        format!(
            "{POINTER_MAGIC}\nsha256:{}\nsize:{}\noriginal:{}\n",
            self.sha256, self.size, self.original
        )
        .into_bytes()
    }
}

/// Blob storage rooted at a filesystem directory.
#[derive(Debug, Clone)]
pub struct BlobStore {
    root: PathBuf,
}

impl BlobStore {
    /// Create a blob store rooted at `root`.
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Return the configured root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Parse a pointer file from raw bytes.
    pub fn parse_pointer(content: &[u8]) -> Option<PointerFile> {
        let text = std::str::from_utf8(content).ok()?;
        let mut lines = text.lines();
        if lines.next()? != POINTER_MAGIC {
            return None;
        }

        let sha256 = lines.next()?.strip_prefix("sha256:")?.to_string();
        let size = lines.next()?.strip_prefix("size:")?.parse().ok()?;
        let original = lines.next()?.strip_prefix("original:")?.to_string();

        if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }

        Some(PointerFile {
            sha256,
            size,
            original,
        })
    }

    /// Compute a SHA-256 hex digest for content.
    pub fn hash_bytes(content: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content);
        let digest = hasher.finalize();
        let mut out = String::with_capacity(digest.len() * 2);
        for byte in digest {
            use std::fmt::Write as _;
            let _ = write!(&mut out, "{byte:02x}");
        }
        out
    }

    /// Store content in the blob store and return its pointer metadata.
    pub fn store_bytes(&self, original_name: &str, content: &[u8]) -> Result<PointerFile> {
        let sha256 = Self::hash_bytes(content);
        let blob_path = self.blob_path(&sha256);

        if let Some(parent) = blob_path.parent() {
            fs::create_dir_all(parent)?;
        }
        if !blob_path.exists() {
            fs::write(&blob_path, content)?;
        }

        Ok(PointerFile {
            sha256,
            size: content.len() as u64,
            original: original_name.to_string(),
        })
    }

    /// Read a blob by hash.
    pub fn read_blob(&self, sha256: &str) -> Result<Vec<u8>> {
        let blob_path = self.blob_path(sha256);
        fs::read(&blob_path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => Error::NotFound(format!("blob '{}' not found", sha256)),
            _ => Error::Io(e),
        })
    }

    /// Delete a blob if it exists.
    pub fn delete_blob(&self, sha256: &str) -> Result<()> {
        let blob_path = self.blob_path(sha256);
        match fs::remove_file(&blob_path) {
            Ok(()) => self.prune_empty_parents(blob_path.parent()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(Error::Io(e)),
        }
    }

    fn blob_path(&self, sha256: &str) -> PathBuf {
        self.root
            .join(&sha256[..2])
            .join(&sha256[2..4])
            .join(sha256)
    }

    fn prune_empty_parents(&self, start: Option<&Path>) -> Result<()> {
        let mut current = start.map(Path::to_path_buf);
        while let Some(path) = current {
            if path == self.root {
                break;
            }
            match fs::remove_dir(&path) {
                Ok(()) => current = path.parent().map(Path::to_path_buf),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    current = path.parent().map(Path::to_path_buf)
                }
                Err(e) if e.kind() == std::io::ErrorKind::DirectoryNotEmpty => break,
                Err(e) => return Err(Error::Io(e)),
            }
        }
        Ok(())
    }
}
