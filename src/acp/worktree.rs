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

impl WorktreeManager {
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

    /// Resolves the default branch for `repo_path`.
    ///
    /// Tries:
    /// 1. `symbolic-ref --short refs/remotes/origin/HEAD` (e.g. "origin/main")
    /// 2. `rev-parse --verify origin/main`
    /// 3. `rev-parse --verify origin/master`
    /// 4. Fallback to `"HEAD"`
    pub fn resolve_default_branch(repo_path: &Path) -> String {
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

        // Try origin/main
        if let Ok(out) = Command::new("git")
            .arg("-C")
            .arg(repo_path)
            .args(["rev-parse", "--verify", "origin/main"])
            .output()
        {
            if out.status.success() {
                return "origin/main".to_string();
            }
        }

        // Try origin/master
        if let Ok(out) = Command::new("git")
            .arg("-C")
            .arg(repo_path)
            .args(["rev-parse", "--verify", "origin/master"])
            .output()
        {
            if out.status.success() {
                return "origin/master".to_string();
            }
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
        let worktrees_dir = repo_path.join(".pin_worktrees");
        let target_dir = worktrees_dir.join(item_id);

        if target_dir.exists() {
            return Ok(target_dir);
        }

        // Ensure parent directory exists
        if !worktrees_dir.exists() {
            fs::create_dir_all(&worktrees_dir)?;
        }
        // Ensure .pin_worktrees is excluded in git info exclude if present
        let exclude_path = repo_path.join(".git").join("info").join("exclude");
        if exclude_path.exists() {
            if let Ok(content) = fs::read_to_string(&exclude_path) {
                if !content.contains(".pin_worktrees") {
                    let _ = fs::write(&exclude_path, format!("{content}\n.pin_worktrees/\n"));
                }
            }
        }

        let base_branch = Self::resolve_default_branch(repo_path);
        let branch_name = format!("pin/{item_id}");
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
                return Err(WorktreeError::GitCommand(format!(
                    "Failed to create worktree: initial attempt failed: {stderr_msg}; fallback attempt failed: {second_err}"
                )));
            }
        }

        if !target_dir.exists() {
            return Err(WorktreeError::GitCommand(format!(
                "Worktree command succeeded but target directory does not exist: {}",
                target_dir.display()
            )));
        }

        Ok(target_dir)
    }

    /// Removes a git worktree for `item_id`.
    ///
    /// - Runs `git -C <repo_path> worktree remove --force .pin_worktrees/{item_id}`
    /// - Preserves the branch `pin/{item_id}` (does NOT delete branch)
    /// - Cleans up directory if still present on disk
    /// - Runs `git -C <repo_path> worktree prune`
    pub fn remove_worktree(repo_path: &Path, item_id: &str) -> Result<(), WorktreeError> {
        let target_dir = repo_path.join(".pin_worktrees").join(item_id);
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
}
