//! Fork and merge management for gitoxide-fs.
//!
//! Implements the fork/merge paradigm for parallel agent work.
//! Each fork creates a new git branch, and merging reconciles changes.
//! Fork metadata is persisted to `.gitoxide-fs/forks.json` alongside the repo
//! so that fork state survives across CLI invocations and process restarts.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use crate::config::MergeStrategy;
use crate::error::{Error, Result};
use crate::git::GitBackend;

/// Information about an active fork.
#[derive(Debug, Clone)]
pub struct ForkInfo {
    /// Unique identifier for this fork.
    pub id: String,
    /// The git branch backing this fork.
    pub branch: String,
    /// The parent fork's branch (or "main" for root).
    pub parent_branch: String,
    /// The commit where this fork diverged.
    pub fork_point: String,
    /// Mount point for this fork (if mounted).
    pub mount_point: Option<PathBuf>,
    /// Number of commits since fork point.
    pub commits_ahead: usize,
    /// Whether this fork has been merged.
    pub merged: bool,
}

/// Result of a merge operation.
#[derive(Debug, Clone)]
pub struct MergeResult {
    /// The merge commit ID.
    pub commit_id: String,
    /// Whether there were conflicts.
    pub had_conflicts: bool,
    /// List of conflicting files (empty if no conflicts).
    pub conflicts: Vec<MergeConflict>,
    /// Number of files changed.
    pub files_changed: usize,
}

/// A merge conflict for a specific file.
#[derive(Debug, Clone)]
pub struct MergeConflict {
    pub path: String,
    pub conflict_type: ConflictType,
    pub ours: Option<Vec<u8>>,
    pub theirs: Option<Vec<u8>>,
    pub base: Option<Vec<u8>>,
}

/// Types of merge conflicts.
#[derive(Debug, Clone, PartialEq)]
pub enum ConflictType {
    /// Both sides modified the same file.
    BothModified,
    /// One side modified, other deleted.
    ModifyDelete,
    /// Both sides added a file with same name.
    BothAdded,
    /// Directory vs file conflict.
    DirectoryFile,
}

/// Internal metadata tracked for each fork, persisted to disk as JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ForkMetadata {
    /// The branch name for this fork.
    branch: String,
    /// The parent branch this fork was created from.
    parent_branch: String,
    /// The commit OID at the point of fork creation.
    fork_point: String,
    /// Whether this fork has been merged back.
    merged: bool,
}

/// Manages fork lifecycle — creation, listing, merging, deletion.
///
/// Fork metadata is persisted to `.gitoxide-fs/forks.json` in the repository
/// root directory, ensuring fork state survives across process restarts.
pub struct ForkManager {
    backend: GitBackend,
    forks: RwLock<HashMap<String, ForkMetadata>>,
    /// Path to the JSON file storing fork metadata.
    metadata_path: PathBuf,
}

impl ForkManager {
    /// Create a new ForkManager, loading any persisted fork metadata from disk.
    pub fn new(backend: GitBackend) -> Self {
        let metadata_path = backend.repo_path().join(".gitoxide-fs").join("forks.json");
        let forks = Self::load_from_disk(&metadata_path).unwrap_or_default();
        Self {
            backend,
            forks: RwLock::new(forks),
            metadata_path,
        }
    }

    /// Access the underlying git backend (e.g. to read/write files on branches).
    pub fn backend(&self) -> &GitBackend {
        &self.backend
    }

    /// Create a new fork from the current branch.
    pub fn create_fork(&self, name: &str) -> Result<ForkInfo> {
        let parent_branch = self.backend.current_branch()?;
        let fork_point = self
            .backend
            .head_commit_hex()
            .ok_or_else(|| Error::Fork("no commits yet, cannot fork".into()))?;

        // Check for duplicate fork names in our tracking.
        {
            let forks = self.read_forks()?;
            if forks.contains_key(name) {
                return Err(Error::AlreadyExists(format!("fork '{}'", name)));
            }
        }

        // Create the git branch at the current HEAD.
        self.backend.create_branch(name)?;

        let metadata = ForkMetadata {
            branch: name.to_string(),
            parent_branch: parent_branch.clone(),
            fork_point: fork_point.clone(),
            merged: false,
        };

        {
            let mut forks = self.write_forks()?;
            forks.insert(name.to_string(), metadata);
        }
        self.persist()?;

        Ok(ForkInfo {
            id: name.to_string(),
            branch: name.to_string(),
            parent_branch,
            fork_point,
            mount_point: None,
            commits_ahead: 0,
            merged: false,
        })
    }

    /// Create a fork from a specific commit or tag.
    pub fn create_fork_at(&self, name: &str, commit_id: &str) -> Result<ForkInfo> {
        let parent_branch = self.backend.current_branch()?;

        {
            let forks = self.read_forks()?;
            if forks.contains_key(name) {
                return Err(Error::AlreadyExists(format!("fork '{}'", name)));
            }
        }

        self.backend.create_branch_at(name, commit_id)?;

        let metadata = ForkMetadata {
            branch: name.to_string(),
            parent_branch: parent_branch.clone(),
            fork_point: commit_id.to_string(),
            merged: false,
        };

        {
            let mut forks = self.write_forks()?;
            forks.insert(name.to_string(), metadata);
        }
        self.persist()?;

        Ok(ForkInfo {
            id: name.to_string(),
            branch: name.to_string(),
            parent_branch,
            fork_point: commit_id.to_string(),
            mount_point: None,
            commits_ahead: 0,
            merged: false,
        })
    }

    /// Create a nested fork (fork of a fork).
    pub fn create_nested_fork(&self, parent_fork: &str, name: &str) -> Result<ForkInfo> {
        // Find the parent fork's branch and its current commit.
        let parent_commit = {
            let forks = self.read_forks()?;
            let parent_meta = forks
                .get(parent_fork)
                .ok_or_else(|| Error::NotFound(format!("fork '{}' not found", parent_fork)))?;
            if parent_meta.merged {
                return Err(Error::Fork(format!(
                    "cannot fork from already-merged fork '{}'",
                    parent_fork
                )));
            }
            // The parent fork's branch tip is the fork point for the nested fork.
            self.backend.branch_commit_oid(&parent_meta.branch)?
        };

        {
            let forks = self.read_forks()?;
            if forks.contains_key(name) {
                return Err(Error::AlreadyExists(format!("fork '{}'", name)));
            }
        }

        self.backend.create_branch_at(name, &parent_commit)?;

        let metadata = ForkMetadata {
            branch: name.to_string(),
            parent_branch: parent_fork.to_string(),
            fork_point: parent_commit.clone(),
            merged: false,
        };

        {
            let mut forks = self.write_forks()?;
            forks.insert(name.to_string(), metadata);
        }
        self.persist()?;

        Ok(ForkInfo {
            id: name.to_string(),
            branch: name.to_string(),
            parent_branch: parent_fork.to_string(),
            fork_point: parent_commit,
            mount_point: None,
            commits_ahead: 0,
            merged: false,
        })
    }

    /// List all active forks.
    pub fn list_forks(&self) -> Result<Vec<ForkInfo>> {
        let forks = self.read_forks()?;
        let mut result = Vec::new();
        for (name, meta) in forks.iter() {
            let commits_ahead = self.count_commits_ahead(&meta.branch, &meta.fork_point);
            result.push(ForkInfo {
                id: name.clone(),
                branch: meta.branch.clone(),
                parent_branch: meta.parent_branch.clone(),
                fork_point: meta.fork_point.clone(),
                mount_point: None,
                commits_ahead,
                merged: meta.merged,
            });
        }
        result.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(result)
    }

    /// Get info about a specific fork.
    pub fn get_fork(&self, name: &str) -> Result<ForkInfo> {
        let forks = self.read_forks()?;
        let meta = forks
            .get(name)
            .ok_or_else(|| Error::NotFound(format!("fork '{}' not found", name)))?;
        let commits_ahead = self.count_commits_ahead(&meta.branch, &meta.fork_point);
        Ok(ForkInfo {
            id: name.to_string(),
            branch: meta.branch.clone(),
            parent_branch: meta.parent_branch.clone(),
            fork_point: meta.fork_point.clone(),
            mount_point: None,
            commits_ahead,
            merged: meta.merged,
        })
    }

    /// Merge a fork back into its parent branch using the default three-way strategy.
    pub fn merge_fork(&self, name: &str) -> Result<MergeResult> {
        self.merge_fork_with_strategy(name, MergeStrategy::ThreeWay)
    }

    /// Merge with a specific strategy.
    pub fn merge_fork_with_strategy(
        &self,
        name: &str,
        strategy: MergeStrategy,
    ) -> Result<MergeResult> {
        // Validate the fork exists and isn't already merged.
        let (parent_branch, fork_branch, fork_point) = {
            let forks = self.read_forks()?;
            let meta = forks
                .get(name)
                .ok_or_else(|| Error::NotFound(format!("fork '{}' not found", name)))?;
            if meta.merged {
                return Err(Error::Fork(format!(
                    "fork '{}' has already been merged",
                    name
                )));
            }
            (
                meta.parent_branch.clone(),
                meta.branch.clone(),
                meta.fork_point.clone(),
            )
        };

        // Get the commit OIDs for parent branch tip and fork branch tip.
        let parent_commit = self.resolve_branch_commit(&parent_branch)?;
        let fork_commit = self.backend.branch_commit_oid(&fork_branch)?;

        // Get tree snapshots at the base (fork point), parent tip, and fork tip.
        let base_tree = self.backend.tree_at_commit(&fork_point)?;
        let parent_tree = self.backend.tree_at_commit(&parent_commit)?;
        let fork_tree = self.backend.tree_at_commit(&fork_commit)?;

        // Perform three-way merge to detect conflicts and compute result.
        let (conflicts, merged_files, files_changed) =
            self.three_way_merge(&base_tree, &parent_tree, &fork_tree, &strategy);

        let had_conflicts = !conflicts.is_empty();

        // If strategy is Manual and there are conflicts, report them without committing.
        if had_conflicts && strategy == MergeStrategy::ThreeWay {
            // For ThreeWay with conflicts, report the conflicts.
            // Mark as merged so it can't be merged again.
            {
                let mut forks = self.write_forks()?;
                if let Some(meta) = forks.get_mut(name) {
                    meta.merged = true;
                }
            }
            self.persist()?;
            return Ok(MergeResult {
                commit_id: parent_commit.clone(),
                had_conflicts: true,
                conflicts,
                files_changed,
            });
        }

        // Apply the merged files to the working tree.
        // First, switch to the parent branch.
        self.backend.checkout_branch(&parent_branch)?;

        // Write all merged files to the working tree.
        for (path, content) in &merged_files {
            // Ensure parent directories exist.
            let abs_path = self.backend.repo_path().join(path);
            if let Some(parent) = abs_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&abs_path, content)?;
        }

        // Remove files that were in the parent but are deleted in the merge result.
        for path in parent_tree.keys() {
            if !merged_files.contains_key(path) {
                let abs_path = self.backend.repo_path().join(path);
                if abs_path.exists() {
                    let _ = std::fs::remove_file(&abs_path);
                }
            }
        }

        // Create merge commit.
        let commit_msg = format!("Merge fork '{}' into {}", name, parent_branch);
        let commit_id =
            self.backend
                .create_merge_commit(&parent_commit, &fork_commit, &commit_msg)?;

        // Mark as merged.
        {
            let mut forks = self.write_forks()?;
            if let Some(meta) = forks.get_mut(name) {
                meta.merged = true;
            }
        }
        self.persist()?;

        Ok(MergeResult {
            commit_id,
            had_conflicts: false,
            conflicts: Vec::new(),
            files_changed,
        })
    }

    /// Abandon a fork (delete the branch).
    pub fn abandon_fork(&self, name: &str) -> Result<()> {
        {
            let forks = self.read_forks()?;
            if !forks.contains_key(name) {
                return Err(Error::NotFound(format!("fork '{}' not found", name)));
            }
        }

        // Delete the git branch.
        let branch_name = {
            let forks = self.read_forks()?;
            forks[name].branch.clone()
        };
        self.backend.delete_branch(&branch_name)?;

        // Remove from tracking.
        {
            let mut forks = self.write_forks()?;
            forks.remove(name);
        }
        self.persist()?;

        Ok(())
    }

    /// Check if a fork can be merged cleanly (dry run).
    pub fn can_merge(&self, name: &str) -> Result<bool> {
        let (parent_branch, fork_branch, fork_point) = {
            let forks = self.read_forks()?;
            let meta = forks
                .get(name)
                .ok_or_else(|| Error::NotFound(format!("fork '{}' not found", name)))?;
            if meta.merged {
                return Err(Error::Fork(format!(
                    "fork '{}' has already been merged",
                    name
                )));
            }
            (
                meta.parent_branch.clone(),
                meta.branch.clone(),
                meta.fork_point.clone(),
            )
        };

        let parent_commit = self.resolve_branch_commit(&parent_branch)?;
        let fork_commit = self.backend.branch_commit_oid(&fork_branch)?;

        let base_tree = self.backend.tree_at_commit(&fork_point)?;
        let parent_tree = self.backend.tree_at_commit(&parent_commit)?;
        let fork_tree = self.backend.tree_at_commit(&fork_commit)?;

        let (conflicts, _, _) =
            self.three_way_merge(&base_tree, &parent_tree, &fork_tree, &MergeStrategy::ThreeWay);
        Ok(conflicts.is_empty())
    }

    /// Get the diff between a fork and its parent.
    pub fn fork_diff(&self, name: &str) -> Result<String> {
        let (parent_branch, fork_branch) = {
            let forks = self.read_forks()?;
            let meta = forks
                .get(name)
                .ok_or_else(|| Error::NotFound(format!("fork '{}' not found", name)))?;
            (meta.parent_branch.clone(), meta.branch.clone())
        };

        let parent_commit = self.resolve_branch_commit(&parent_branch)?;
        let fork_commit = self.backend.branch_commit_oid(&fork_branch)?;

        // If both branches point to the same commit, there's no diff.
        if parent_commit == fork_commit {
            return Ok(String::new());
        }

        self.backend.diff(&parent_commit, &fork_commit)
    }

    // ======================== Persistence =====================================

    /// Load fork metadata from a JSON file on disk.
    fn load_from_disk(path: &PathBuf) -> Option<HashMap<String, ForkMetadata>> {
        let data = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&data).ok()
    }

    /// Persist current fork metadata to disk.
    fn persist(&self) -> Result<()> {
        let forks = self.read_forks()?;
        // Ensure the parent directory exists.
        if let Some(parent) = self.metadata_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&*forks)
            .map_err(|e| Error::Fork(format!("failed to serialize fork metadata: {}", e)))?;
        std::fs::write(&self.metadata_path, json)?;
        Ok(())
    }

    // ======================== RwLock helpers ==================================

    /// Acquire read lock on forks, converting poisoned lock to an error.
    fn read_forks(&self) -> Result<std::sync::RwLockReadGuard<'_, HashMap<String, ForkMetadata>>> {
        self.forks
            .read()
            .map_err(|_| Error::Fork("fork metadata lock poisoned".into()))
    }

    /// Acquire write lock on forks, converting poisoned lock to an error.
    fn write_forks(
        &self,
    ) -> Result<std::sync::RwLockWriteGuard<'_, HashMap<String, ForkMetadata>>> {
        self.forks
            .write()
            .map_err(|_| Error::Fork("fork metadata lock poisoned".into()))
    }

    // ======================== Internal helpers ==============================

    /// Resolve a branch name to its commit OID. Handles both tracked forks
    /// (which might be parent forks) and regular branches.
    fn resolve_branch_commit(&self, branch: &str) -> Result<String> {
        // First check if this is a tracked fork — use its branch name.
        let forks = self.read_forks()?;
        if let Some(meta) = forks.get(branch) {
            return self.backend.branch_commit_oid(&meta.branch);
        }
        drop(forks);
        // Otherwise resolve directly as a branch name.
        self.backend.branch_commit_oid(branch)
    }

    /// Count how many commits a branch is ahead of a given fork point.
    fn count_commits_ahead(&self, branch: &str, fork_point: &str) -> usize {
        let tip = match self.backend.branch_commit_oid(branch) {
            Ok(hex) => hex,
            Err(_) => return 0,
        };
        if tip == fork_point {
            return 0;
        }
        // Walk backwards from tip counting commits until we hit the fork point.
        // For simplicity, use the log and count entries.
        let logs = match self.backend.log(Some(100)) {
            Ok(l) => l,
            Err(_) => return 0,
        };
        let mut count = 0;
        for entry in &logs {
            if entry.id == *fork_point {
                break;
            }
            count += 1;
        }
        count
    }

    /// Perform a three-way merge between base, ours (parent), and theirs (fork).
    ///
    /// Returns: (conflicts, merged_files, files_changed_count)
    fn three_way_merge(
        &self,
        base: &HashMap<String, Vec<u8>>,
        ours: &HashMap<String, Vec<u8>>,
        theirs: &HashMap<String, Vec<u8>>,
        strategy: &MergeStrategy,
    ) -> (Vec<MergeConflict>, HashMap<String, Vec<u8>>, usize) {
        let mut conflicts = Vec::new();
        let mut merged = HashMap::new();
        let mut files_changed: usize = 0;

        // Collect all paths from all three trees.
        let mut all_paths: Vec<String> = Vec::new();
        for key in base.keys().chain(ours.keys()).chain(theirs.keys()) {
            if !all_paths.contains(key) {
                all_paths.push(key.clone());
            }
        }
        all_paths.sort();

        for path in &all_paths {
            let in_base = base.get(path);
            let in_ours = ours.get(path);
            let in_theirs = theirs.get(path);

            match (in_base, in_ours, in_theirs) {
                // File unchanged in all three — keep as-is.
                (Some(b), Some(o), Some(t)) if b == o && b == t => {
                    merged.insert(path.clone(), o.clone());
                }
                // File modified only in ours — take ours.
                (Some(b), Some(o), Some(t)) if b == t && b != o => {
                    merged.insert(path.clone(), o.clone());
                    files_changed += 1;
                }
                // File modified only in theirs — take theirs.
                (Some(b), Some(o), Some(t)) if b == o && b != t => {
                    merged.insert(path.clone(), t.clone());
                    files_changed += 1;
                }
                // Both modified differently — conflict!
                (Some(b), Some(o), Some(t)) => {
                    files_changed += 1;
                    match strategy {
                        MergeStrategy::Ours => {
                            merged.insert(path.clone(), o.clone());
                        }
                        MergeStrategy::Theirs => {
                            merged.insert(path.clone(), t.clone());
                        }
                        _ => {
                            // ThreeWay / Rebase: report conflict, keep ours as default.
                            merged.insert(path.clone(), o.clone());
                            conflicts.push(MergeConflict {
                                path: path.clone(),
                                conflict_type: ConflictType::BothModified,
                                ours: Some(o.clone()),
                                theirs: Some(t.clone()),
                                base: Some(b.clone()),
                            });
                        }
                    }
                }
                // File added only in ours (not in base or theirs).
                (None, Some(o), None) => {
                    merged.insert(path.clone(), o.clone());
                    files_changed += 1;
                }
                // File added only in theirs (not in base or ours).
                (None, None, Some(t)) => {
                    merged.insert(path.clone(), t.clone());
                    files_changed += 1;
                }
                // File added in both — conflict if different content.
                (None, Some(o), Some(t)) => {
                    files_changed += 1;
                    if o == t {
                        merged.insert(path.clone(), o.clone());
                    } else {
                        match strategy {
                            MergeStrategy::Ours => {
                                merged.insert(path.clone(), o.clone());
                            }
                            MergeStrategy::Theirs => {
                                merged.insert(path.clone(), t.clone());
                            }
                            _ => {
                                merged.insert(path.clone(), o.clone());
                                conflicts.push(MergeConflict {
                                    path: path.clone(),
                                    conflict_type: ConflictType::BothAdded,
                                    ours: Some(o.clone()),
                                    theirs: Some(t.clone()),
                                    base: None,
                                });
                            }
                        }
                    }
                }
                // File deleted only in ours (still in base and theirs).
                (Some(_b), None, Some(t)) => {
                    files_changed += 1;
                    if _b == t {
                        // Ours deleted, theirs unchanged — honor deletion.
                        // Don't include in merged.
                    } else {
                        // Ours deleted, theirs modified — conflict!
                        match strategy {
                            MergeStrategy::Ours => {
                                // Ours deleted — don't include.
                            }
                            MergeStrategy::Theirs => {
                                merged.insert(path.clone(), t.clone());
                            }
                            _ => {
                                conflicts.push(MergeConflict {
                                    path: path.clone(),
                                    conflict_type: ConflictType::ModifyDelete,
                                    ours: None,
                                    theirs: Some(t.clone()),
                                    base: Some(_b.clone()),
                                });
                            }
                        }
                    }
                }
                // File deleted only in theirs (still in base and ours).
                (Some(_b), Some(o), None) => {
                    files_changed += 1;
                    if _b == o {
                        // Theirs deleted, ours unchanged — honor deletion.
                    } else {
                        // Theirs deleted, ours modified — conflict!
                        match strategy {
                            MergeStrategy::Ours => {
                                merged.insert(path.clone(), o.clone());
                            }
                            MergeStrategy::Theirs => {
                                // Theirs deleted — don't include.
                            }
                            _ => {
                                conflicts.push(MergeConflict {
                                    path: path.clone(),
                                    conflict_type: ConflictType::ModifyDelete,
                                    ours: Some(o.clone()),
                                    theirs: None,
                                    base: Some(_b.clone()),
                                });
                            }
                        }
                    }
                }
                // File deleted in both — fine, remove it.
                (Some(_), None, None) => {
                    files_changed += 1;
                }
                // File exists nowhere — shouldn't happen.
                (None, None, None) => {}
            }
        }

        (conflicts, merged, files_changed)
    }
}
