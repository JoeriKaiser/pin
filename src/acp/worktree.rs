use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug)]
pub enum WorktreeError {
    Io(io::Error),
    GitCommand(String),
}

impl fmt::Display for WorktreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WorktreeError::Io(err) => write!(f, "I/O error: {err}"),
            WorktreeError::GitCommand(msg) => write!(f, "Git command failed: {msg}"),
        }
    }
}

impl std::error::Error for WorktreeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            WorktreeError::Io(err) => Some(err),
            WorktreeError::GitCommand(_) => None,
        }
    }
}

impl From<io::Error> for WorktreeError {
    fn from(err: io::Error) -> Self {
        WorktreeError::Io(err)
    }
}

pub struct WorktreeManager;

#[derive(Debug, Clone, serde::Serialize)]
pub struct WorktreeInspection {
    pub exists: bool,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    pub changed_files: usize,
    pub ahead_count: usize,
    pub status_lines: Vec<String>,
    pub diff_stat: String,
    pub diff_preview: String,
    pub truncated: bool,
}

impl WorktreeManager {
    /// The standard folder convention where all worktrees are stored.
    pub const WORKTREES_DIR_NAME: &'static str = ".pin_worktrees";

    /// Returns the root directory where all worktrees are stored by convention.
    pub fn worktrees_root(repo_path: &Path) -> PathBuf {
        repo_path.join(Self::WORKTREES_DIR_NAME)
    }

    /// Returns the target directory for a specific item's worktree.
    pub fn worktree_path(repo_path: &Path, item_id: &str) -> PathBuf {
        Self::worktrees_root(repo_path).join(item_id)
    }

    /// Returns true if `repo_path` is inside a git working tree.
    pub fn is_git_repo(repo_path: &Path) -> bool {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo_path)
            .args(["rev-parse", "--is-inside-work-tree"])
            .output();

        match output {
            Ok(out) => out.status.success(),
            Err(_) => false,
        }
    }

    /// Resolves the git repository root from a starting directory.
    pub fn repo_root(start_path: &Path) -> Option<PathBuf> {
        let output = Command::new("git")
            .arg("-C")
            .arg(start_path)
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .ok()?;
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !s.is_empty() {
                return Some(PathBuf::from(s));
            }
        }
        None
    }

    /// Resolves the repository root for a vault, preferring the vault's own git
    /// checkout and otherwise resolving the current working directory's git root.
    pub fn resolve_repo_path(vault_path: &Path) -> PathBuf {
        Self::repo_root(vault_path)
            .or_else(|| {
                std::env::current_dir()
                    .ok()
                    .and_then(|cwd| Self::repo_root(&cwd).or(Some(cwd)))
            })
            .unwrap_or_else(|| vault_path.parent().unwrap_or(vault_path).to_path_buf())
    }

    fn git_exclude_path(repo_path: &Path) -> PathBuf {
        if let Ok(out) = Command::new("git")
            .arg("-C")
            .arg(repo_path)
            .args(["rev-parse", "--git-path", "info/exclude"])
            .output()
        {
            if out.status.success() {
                let raw = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !raw.is_empty() {
                    let path = PathBuf::from(raw);
                    return if path.is_absolute() {
                        path
                    } else {
                        repo_path.join(path)
                    };
                }
            }
        }
        repo_path.join(".git").join("info").join("exclude")
    }

    /// Attempts to fetch the remote origin tracking refs quietly to ensure
    /// remote branches are up to date before resolving default branch.
    #[allow(dead_code)]
    pub fn fetch_origin_quietly(repo_path: &Path) {
        let _ = Command::new("git")
            .arg("-C")
            .arg(repo_path)
            .args(["fetch", "--quiet", "origin"])
            .output();
    }

    fn is_ancestor(repo_path: &Path, ancestor: &str, descendant: &str) -> bool {
        Command::new("git")
            .arg("-C")
            .arg(repo_path)
            .args(["merge-base", "--is-ancestor", ancestor, descendant])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn ref_exists(repo_path: &Path, reference: &str) -> bool {
        Command::new("git")
            .arg("-C")
            .arg(repo_path)
            .args(["rev-parse", "--verify", reference])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Sanitizes an item ID for use as a git branch segment under `pin/`.
    /// Standard hex IDs pass through unchanged.
    pub fn branch_name_for_item(item_id: &str) -> String {
        let mut out = String::with_capacity(item_id.len());
        let mut prev_dot = false;
        for ch in item_id.chars() {
            if ch == '.' && prev_dot {
                continue;
            }
            let valid = ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.');
            if valid {
                out.push(ch);
                prev_dot = ch == '.';
            } else {
                out.push('-');
                prev_dot = false;
            }
        }
        let trimmed = out.trim_matches(['.', '-', '/']);
        let trimmed = trimmed.strip_suffix(".lock").unwrap_or(trimmed);
        let seg = if trimmed.is_empty() { "item" } else { trimmed };
        format!("pin/{seg}")
    }

    /// Resolves the default branch for `repo_path`, prioritizing an up-to-date main branch.
    /// When both `origin/main` and local `main` exist, prefers local `main` if `origin/main`
    /// is an ancestor of `main` (so unpushed local commits on `main` are not silently dropped).
    pub fn resolve_default_branch(repo_path: &Path) -> String {
        let has_origin_main = Self::ref_exists(repo_path, "origin/main");
        let has_local_main = Self::ref_exists(repo_path, "refs/heads/main");
        if has_origin_main && has_local_main {
            if Self::is_ancestor(repo_path, "origin/main", "refs/heads/main") {
                return "main".to_string();
            }
            return "origin/main".to_string();
        }
        if has_origin_main {
            return "origin/main".to_string();
        }
        if has_local_main {
            return "main".to_string();
        }

        // Try origin/HEAD symbolic ref
        if let Ok(out) = Command::new("git")
            .arg("-C")
            .arg(repo_path)
            .args(["symbolic-ref", "--short", "refs/remotes/origin/HEAD"])
            .output()
        {
            if out.status.success() {
                let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !name.is_empty() {
                    return name;
                }
            }
        }

        let has_origin_master = Self::ref_exists(repo_path, "origin/master");
        let has_local_master = Self::ref_exists(repo_path, "refs/heads/master");
        if has_origin_master && has_local_master {
            if Self::is_ancestor(repo_path, "origin/master", "refs/heads/master") {
                return "master".to_string();
            }
            return "origin/master".to_string();
        }
        if has_origin_master {
            return "origin/master".to_string();
        }
        if has_local_master {
            return "master".to_string();
        }

        "HEAD".to_string()
    }
    /// Provisions a git worktree for concurrent execution on `item_id`.
    ///
    /// - Target dir: `repo_path.join(".pin_worktrees").join(item_id)`
    /// - If target dir already exists, returns `Ok(target_dir)`
    /// - Base branch: `resolve_default_branch(repo_path)`
    /// - Branch name: `pin/{item_id}`
    /// - Attempts to create a new branch with `git worktree add -b pin/{item_id} <target_path> <base_branch>`
    /// - If the branch already exists, falls back to checking out existing branch: `git worktree add <target_path> pin/{item_id}`
    pub fn provision_worktree(repo_path: &Path, item_id: &str) -> Result<PathBuf, WorktreeError> {
        let worktrees_dir = Self::worktrees_root(repo_path);
        let target_dir = Self::worktree_path(repo_path, item_id);
        if target_dir.exists() {
            // Verify it is actually a valid git worktree with its own .git pointer;
            // an empty or stale directory inside repo_path would otherwise cause git
            // commands to walk up and mutate the primary checkout.
            if target_dir.join(".git").exists() && Self::is_git_repo(&target_dir) {
                return Ok(target_dir);
            }
            if target_dir.is_dir() {
                let _ = fs::remove_dir_all(&target_dir);
            } else {
                let _ = fs::remove_file(&target_dir);
            }
            let _ = Command::new("git")
                .arg("-C")
                .arg(repo_path)
                .args(["worktree", "prune"])
                .output();
        }

        // Ensure parent directory exists
        if !worktrees_dir.exists() {
            fs::create_dir_all(&worktrees_dir)?;
        }
        // Ensure .pin_worktrees is excluded in git info exclude
        let exclude_path = Self::git_exclude_path(repo_path);
        if let Some(parent) = exclude_path.parent() {
            if parent.exists() || repo_path.join(".git").exists() {
                let _ = fs::create_dir_all(parent);
                let content = fs::read_to_string(&exclude_path).unwrap_or_default();
                if !content.lines().any(|l| l.trim() == ".pin_worktrees/") {
                    let prefix = if content.is_empty() || content.ends_with('\n') {
                        ""
                    } else {
                        "\n"
                    };
                    let _ = fs::write(&exclude_path, format!("{content}{prefix}.pin_worktrees/\n"));
                }
            }
        }

        let base_branch = Self::resolve_default_branch(repo_path);
        let branch_name = Self::branch_name_for_item(item_id);
        let target_str = target_dir
            .to_str()
            .ok_or_else(|| WorktreeError::GitCommand("Invalid non-UTF-8 path".to_string()))?;

        // First attempt: create worktree with new branch (-b)
        let first_add = Command::new("git")
            .arg("-C")
            .arg(repo_path)
            .args([
                "worktree",
                "add",
                "-b",
                &branch_name,
                target_str,
                &base_branch,
            ])
            .output()?;

        if !first_add.status.success() {
            let stderr_msg = String::from_utf8_lossy(&first_add.stderr).to_string();

            // Fallback: branch might already exist, checkout existing branch
            let second_add = Command::new("git")
                .arg("-C")
                .arg(repo_path)
                .args(["worktree", "add", target_str, &branch_name])
                .output()?;

            if !second_add.status.success() {
                let second_err = String::from_utf8_lossy(&second_add.stderr);
                Self::cleanup_failed_provision(repo_path, &worktrees_dir, &target_dir);
                return Err(WorktreeError::GitCommand(format!(
                    "Failed to create worktree: initial attempt failed: {stderr_msg}; fallback attempt failed: {second_err}"
                )));
            }
        }

        if !target_dir.exists() {
            Self::cleanup_failed_provision(repo_path, &worktrees_dir, &target_dir);
            return Err(WorktreeError::GitCommand(format!(
                "Worktree command succeeded but target directory does not exist: {}",
                target_dir.display()
            )));
        }

        Ok(target_dir)
    }

    /// Removes what a failed `git worktree add` left behind: the target
    /// directory, the stale worktree registration, and `.pin_worktrees` itself
    /// when nothing else is in it.
    fn cleanup_failed_provision(repo_path: &Path, worktrees_dir: &Path, target_dir: &Path) {
        if target_dir.exists() {
            let _ = fs::remove_dir_all(target_dir);
        }

        let _ = Command::new("git")
            .arg("-C")
            .arg(repo_path)
            .args(["worktree", "prune"])
            .output();

        let is_empty = fs::read_dir(worktrees_dir)
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(false);
        if is_empty {
            let _ = fs::remove_dir(worktrees_dir);
        }
    }

    /// Checks whether the worktree for `item_id` has uncommitted modifications or untracked files.
    pub fn has_uncommitted_changes(repo_path: &Path, item_id: &str) -> bool {
        let target_dir = Self::worktree_path(repo_path, item_id);
        if !target_dir.exists() {
            return false;
        }
        Command::new("git")
            .arg("-C")
            .arg(&target_dir)
            .args(["status", "--porcelain"])
            .output()
            .map(|o| o.status.success() && !o.stdout.is_empty())
            .unwrap_or(false)
    }

    /// Inspects a worktree for `item_id`, returning branch, dirty status lines,
    /// ahead commit count, diff stat, and a capped diff preview.
    pub fn inspect_worktree(repo_path: &Path, item_id: &str) -> WorktreeInspection {
        let target_dir = Self::worktree_path(repo_path, item_id);
        let rel_path = format!("{}/{item_id}", Self::WORKTREES_DIR_NAME);
        if !target_dir.exists() || !target_dir.join(".git").exists() {
            return WorktreeInspection {
                exists: false,
                path: rel_path,
                branch: None,
                changed_files: 0,
                ahead_count: 0,
                status_lines: Vec::new(),
                diff_stat: String::new(),
                diff_preview: String::new(),
                truncated: false,
            };
        }

        let branch = Command::new("git")
            .arg("-C")
            .arg(&target_dir)
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty());

        let status_raw = Command::new("git")
            .arg("-C")
            .arg(&target_dir)
            .args(["status", "--short"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();

        let all_status_lines: Vec<String> = status_raw
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.to_string())
            .collect();
        let changed_files = all_status_lines.len();
        let status_lines: Vec<String> = all_status_lines.into_iter().take(50).collect();

        let base_branch = Self::resolve_default_branch(repo_path);
        let ahead_count = Command::new("git")
            .arg("-C")
            .arg(&target_dir)
            .args(["rev-list", "--count", &format!("{base_branch}..HEAD")])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .trim()
                    .parse::<usize>()
                    .ok()
            })
            .unwrap_or(0);

        let diff_stat = Command::new("git")
            .arg("-C")
            .arg(&target_dir)
            .args(["diff", "--stat", "HEAD"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();

        const MAX_DIFF_BYTES: usize = 32 * 1024;
        let diff_raw = Command::new("git")
            .arg("-C")
            .arg(&target_dir)
            .args(["diff", "HEAD"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();

        let (diff_preview, truncated) = if diff_raw.len() > MAX_DIFF_BYTES {
            let mut end = MAX_DIFF_BYTES;
            while end > 0 && !diff_raw.is_char_boundary(end) {
                end -= 1;
            }
            (
                format!("{}\n[... diff truncated ...]", &diff_raw[..end]),
                true,
            )
        } else {
            (diff_raw, false)
        };

        WorktreeInspection {
            exists: true,
            path: rel_path,
            branch,
            changed_files,
            ahead_count,
            status_lines,
            diff_stat,
            diff_preview,
            truncated,
        }
    }

    /// Removes a git worktree for `item_id`.
    ///
    /// - Runs `git -C <repo_path> worktree remove --force .pin_worktrees/{item_id}`
    /// - Preserves the branch `pin/{item_id}` (does NOT delete branch)
    /// - Cleans up directory if still present on disk
    /// - Runs `git -C <repo_path> worktree prune`
    pub fn remove_worktree(repo_path: &Path, item_id: &str) -> Result<(), WorktreeError> {
        let target_dir = Self::worktree_path(repo_path, item_id);
        let target_str = target_dir
            .to_str()
            .ok_or_else(|| WorktreeError::GitCommand("Invalid non-UTF-8 path".to_string()))?;

        let remove_output = Command::new("git")
            .arg("-C")
            .arg(repo_path)
            .args(["worktree", "remove", "--force", target_str])
            .output()?;

        if !remove_output.status.success() {
            // Note: even if git worktree remove fails (e.g. worktree already detached or directory removed),
            // we proceed to cleanup disk and prune.
            let _ = remove_output;
        }

        if target_dir.exists() {
            fs::remove_dir_all(&target_dir)?;
        }

        let _ = Command::new("git")
            .arg("-C")
            .arg(repo_path)
            .args(["worktree", "prune"])
            .output();

        let worktrees_dir = Self::worktrees_root(repo_path);
        if worktrees_dir.exists() {
            let is_empty = fs::read_dir(&worktrees_dir)
                .map(|mut entries| entries.next().is_none())
                .unwrap_or(false);
            if is_empty {
                let _ = fs::remove_dir(&worktrees_dir);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_worktree_lifecycle() {
        let dir = tempdir().unwrap();
        let repo = dir.path();

        let init = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["init"])
            .output()
            .unwrap();
        assert!(init.status.success());

        assert!(WorktreeManager::is_git_repo(repo));

        std::fs::write(repo.join("README.md"), "hello").unwrap();
        Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["config", "user.email", "test@test.com"])
            .output()
            .unwrap();
        Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["config", "user.name", "Test"])
            .output()
            .unwrap();
        Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["add", "."])
            .output()
            .unwrap();
        Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["commit", "-m", "init"])
            .output()
            .unwrap();

        let wt = WorktreeManager::provision_worktree(repo, "task123").unwrap();
        assert!(wt.exists());
        assert!(wt.join("README.md").exists());

        WorktreeManager::remove_worktree(repo, "task123").unwrap();
        assert!(!wt.exists());

        let branch_check = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["rev-parse", "--verify", "pin/task123"])
            .output()
            .unwrap();
        assert!(branch_check.status.success());
    }

    #[test]
    fn test_failed_provision_cleans_up() {
        let dir = tempdir().unwrap();
        let repo = dir.path();

        // A directory that is not a git checkout: `git worktree add` fails, and
        // provisioning must not leave an empty `.pin_worktrees` behind.
        let result = WorktreeManager::provision_worktree(repo, "task456");
        assert!(result.is_err());
        assert!(!repo.join(".pin_worktrees").exists());
    }

    #[test]
    fn test_resolve_default_branch_with_main() {
        let dir = tempdir().unwrap();
        let repo = dir.path();

        Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["init", "-b", "main"])
            .output()
            .unwrap();

        std::fs::write(repo.join("README.md"), "hello").unwrap();
        Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["config", "user.email", "test@test.com"])
            .output()
            .unwrap();
        Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["config", "user.name", "Test"])
            .output()
            .unwrap();
        Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["add", "."])
            .output()
            .unwrap();
        Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["commit", "-m", "init main"])
            .output()
            .unwrap();

        // Switch to a feature branch
        Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["checkout", "-b", "feature-xyz"])
            .output()
            .unwrap();

        // Even though current branch is feature-xyz, resolve_default_branch should find "main"
        let resolved = WorktreeManager::resolve_default_branch(repo);
        assert_eq!(resolved, "main");

        // Convention path helper check
        assert_eq!(
            WorktreeManager::worktrees_root(repo),
            repo.join(".pin_worktrees")
        );
        assert_eq!(
            WorktreeManager::worktree_path(repo, "task99"),
            repo.join(".pin_worktrees").join("task99")
        );
    }

    #[test]
    fn test_remove_worktree_prunes_empty_root_and_exclude_works_in_linked_wt() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        fs::create_dir_all(&repo).unwrap();

        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["init", "-b", "main"])
            .output()
            .unwrap();
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["config", "user.name", "Test"])
            .output()
            .unwrap();
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["config", "user.email", "test@test.com"])
            .output()
            .unwrap();
        fs::write(repo.join("README.md"), "# test").unwrap();
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["add", "."])
            .output()
            .unwrap();
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["commit", "-m", "init"])
            .output()
            .unwrap();

        let wt = WorktreeManager::provision_worktree(&repo, "wtprune1").unwrap();
        assert!(wt.exists());
        // Linked worktree has a `.git` file, not directory; git_exclude_path must still find info/exclude
        let exclude = WorktreeManager::git_exclude_path(&wt);
        assert!(
            exclude.exists(),
            "exclude path should resolve from linked worktree"
        );

        WorktreeManager::remove_worktree(&repo, "wtprune1").unwrap();
        assert!(!wt.exists());
        assert!(
            !WorktreeManager::worktrees_root(&repo).exists(),
            "empty .pin_worktrees directory should be removed"
        );
    }

    #[test]
    fn test_provision_recovers_from_empty_broken_worktree_dir_and_sanitizes_branch() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        fs::create_dir_all(&repo).unwrap();

        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["init", "-b", "main"])
            .output()
            .unwrap();
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["config", "user.name", "Test"])
            .output()
            .unwrap();
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["config", "user.email", "test@test.com"])
            .output()
            .unwrap();
        fs::write(repo.join("README.md"), "# test").unwrap();
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["add", "."])
            .output()
            .unwrap();
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["commit", "-m", "init"])
            .output()
            .unwrap();

        // Create a broken empty directory where the worktree should live.
        let broken_dir = WorktreeManager::worktree_path(&repo, "broken1");
        fs::create_dir_all(&broken_dir).unwrap();

        let wt = WorktreeManager::provision_worktree(&repo, "broken1").unwrap();
        assert!(wt.join(".git").exists());
        assert!(wt.join("README.md").exists());

        assert_eq!(
            WorktreeManager::branch_name_for_item("a1b2c3d4e5f6"),
            "pin/a1b2c3d4e5f6"
        );
        assert_eq!(
            WorktreeManager::branch_name_for_item("bad name..test.lock"),
            "pin/bad-name.test"
        );
    }

    #[test]
    fn test_resolve_default_branch_prefers_local_main_ahead_of_origin() {
        let dir = tempdir().unwrap();
        let remote = dir.path().join("remote.git");
        let repo = dir.path().join("repo");

        Command::new("git")
            .args(["init", "--bare", "-b", "main"])
            .arg(&remote)
            .output()
            .unwrap();
        Command::new("git")
            .args(["clone"])
            .arg(&remote)
            .arg(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["config", "user.name", "Test"])
            .output()
            .unwrap();
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["config", "user.email", "test@test.com"])
            .output()
            .unwrap();
        fs::write(repo.join("README.md"), "v1").unwrap();
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["add", "."])
            .output()
            .unwrap();
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["commit", "-m", "v1"])
            .output()
            .unwrap();
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["push", "-u", "origin", "main"])
            .output()
            .unwrap();

        // Add an unpushed local commit on main
        fs::write(repo.join("README.md"), "v2").unwrap();
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["commit", "-am", "v2"])
            .output()
            .unwrap();

        assert_eq!(WorktreeManager::resolve_default_branch(&repo), "main");
    }

    #[test]
    fn test_inspect_worktree_reports_branch_status_and_diff() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        fs::create_dir_all(&repo).unwrap();

        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["init", "-b", "main"])
            .output()
            .unwrap();
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["config", "user.name", "Test"])
            .output()
            .unwrap();
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["config", "user.email", "test@test.com"])
            .output()
            .unwrap();
        fs::write(repo.join("README.md"), "line 1\n").unwrap();
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["add", "."])
            .output()
            .unwrap();
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["commit", "-m", "init"])
            .output()
            .unwrap();

        let absent = WorktreeManager::inspect_worktree(&repo, "missing1");
        assert!(!absent.exists);

        let wt = WorktreeManager::provision_worktree(&repo, "insp123").unwrap();
        fs::write(wt.join("README.md"), "line 1\nline 2\n").unwrap();

        let info = WorktreeManager::inspect_worktree(&repo, "insp123");
        assert!(info.exists);
        assert_eq!(info.branch.as_deref(), Some("pin/insp123"));
        assert_eq!(info.changed_files, 1);
        assert!(info.diff_preview.contains("+line 2"));
    }
}
