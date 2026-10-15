use crate::acp::WorktreeManager;
use crate::model::{IdeaMeta, Resolution, Status};
use crate::vault::collect_ideas;
use crate::workflow;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitItemStatus {
    Merged,
    PrOpen,
    Committed,
    ChangesPending,
    None,
}

impl GitItemStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            GitItemStatus::Merged => "merged",
            GitItemStatus::PrOpen => "pr_open",
            GitItemStatus::Committed => "committed",
            GitItemStatus::ChangesPending => "changes_pending",
            GitItemStatus::None => "none",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitItemInfo {
    pub id: String,
    pub status: GitItemStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr_number: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_oid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GhPrInfo {
    pub number: u64,
    #[serde(rename = "headRefName")]
    pub head_ref_name: String,
    pub state: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResult {
    pub id: String,
    pub title: String,
    pub status: String,
    pub action: String,
    pub resolution: String,
    pub resolution_note: String,
    pub worktree_removed: bool,
    pub branch_deleted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr_number: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_oid: Option<String>,
}

pub fn query_gh_prs(repo_path: &Path) -> Option<Vec<GhPrInfo>> {
    // gh talks to the GitHub API. PR data is a convenience, never a
    // requirement, so it must not stall its callers: CI runs skip the lookup
    // entirely (deterministic tests, no network), and anywhere else gh gets a
    // few seconds before the process is killed and the status resolves
    // without PR data. This call sits in the view startup path, so an
    // offline or slow gh used to hold the server back before it printed its
    // URL for as long as gh took to give up.
    if std::env::var("CI").is_ok() {
        return None;
    }
    const GH_TIMEOUT: Duration = Duration::from_secs(3);

    let mut child = Command::new("gh")
        .arg("-C")
        .arg(repo_path)
        .args([
            "pr",
            "list",
            "--state",
            "all",
            "--json",
            "number,headRefName,state,url",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    let deadline = Instant::now() + GH_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(_) => return None,
        }
    };

    if !status.success() {
        return None;
    }
    let mut stdout = child.stdout.take()?;
    let mut buf = Vec::new();
    stdout.read_to_end(&mut buf).ok()?;
    serde_json::from_slice(&buf).ok()
}

pub fn resolve_target_branches(repo_path: &Path) -> Vec<String> {
    let mut targets = Vec::new();
    let default_branch = WorktreeManager::resolve_default_branch(repo_path);
    if default_branch != "HEAD" {
        targets.push(default_branch.clone());
    }
    if default_branch.starts_with("origin/") {
        let local = default_branch.trim_start_matches("origin/");
        if Command::new("git")
            .arg("-C")
            .arg(repo_path)
            .args(["rev-parse", "--verify", local])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            targets.push(local.to_string());
        }
    }
    if targets.is_empty() {
        targets.push("HEAD".to_string());
    }
    targets
}

fn branch_exists(repo_path: &Path, branch_name: &str) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["rev-parse", "--verify", branch_name])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn is_ancestor_of_any(repo_path: &Path, branch: &str, targets: &[String]) -> bool {
    let branch_oid = get_branch_commit(repo_path, branch);
    let Some(b_oid) = branch_oid else {
        return false;
    };

    for target in targets {
        if let Some(t_oid) = get_branch_commit(repo_path, target) {
            if b_oid == t_oid {
                continue;
            }
        }
        if let Ok(out) = Command::new("git")
            .arg("-C")
            .arg(repo_path)
            .args(["merge-base", "--is-ancestor", branch, target])
            .output()
        {
            if out.status.success() {
                return true;
            }
        }
    }
    false
}

fn get_branch_commit(repo_path: &Path, branch: &str) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["rev-parse", branch])
        .output()
        .ok()?;
    if out.status.success() {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !s.is_empty() {
            return Some(s);
        }
    }
    None
}

fn find_squash_commit(repo_path: &Path, item_id: &str, targets: &[String]) -> Option<String> {
    let p1 = format!("({item_id})");
    let p2 = format!("Item ID: {item_id}");
    for target in targets {
        if let Ok(out) = Command::new("git")
            .arg("-C")
            .arg(repo_path)
            .args([
                "log",
                "-n",
                "1",
                "--format=%H",
                "-F",
                "--grep",
                &p1,
                "--grep",
                &p2,
                target,
            ])
            .output()
        {
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !s.is_empty() {
                    return Some(s);
                }
            }
        }
    }
    None
}

fn has_uncommitted_changes(dir_path: &Path) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(dir_path)
        .args(["status", "--porcelain"])
        .output()
        .map(|o| o.status.success() && !o.stdout.is_empty())
        .unwrap_or(false)
}

fn is_current_branch(repo_path: &Path, branch: &str) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .map(|o| {
            if o.status.success() {
                String::from_utf8_lossy(&o.stdout).trim() == branch
            } else {
                false
            }
        })
        .unwrap_or(false)
}

fn has_commits_ahead(repo_path: &Path, branch: &str, targets: &[String]) -> bool {
    let target = targets.first().map(|s| s.as_str()).unwrap_or("HEAD");
    let range = format!("{target}..{branch}");
    Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["rev-list", "--count", &range])
        .output()
        .map(|o| {
            if o.status.success() {
                String::from_utf8_lossy(&o.stdout)
                    .trim()
                    .parse::<usize>()
                    .map(|c| c > 0)
                    .unwrap_or(false)
            } else {
                false
            }
        })
        .unwrap_or(false)
}

pub fn find_pr_for_item<'a>(prs: &'a [GhPrInfo], item_id: &str) -> Option<&'a GhPrInfo> {
    let expected_branch = format!("pin/{item_id}");
    prs.iter().find(|pr| {
        pr.head_ref_name == expected_branch
            || pr.head_ref_name == item_id
            || pr.head_ref_name.ends_with(&format!("/{item_id}"))
    })
}

pub fn resolve_git_status(
    repo_path: &Path,
    item_id: &str,
    prs: Option<&[GhPrInfo]>,
    targets: &[String],
) -> GitItemInfo {
    let branch_name = format!("pin/{item_id}");
    let remote_branch = format!("origin/pin/{item_id}");
    let worktree_dir = repo_path.join(".pin_worktrees").join(item_id);

    let local_exists = branch_exists(repo_path, &branch_name);
    let remote_exists = branch_exists(repo_path, &remote_branch);
    let wt_exists = worktree_dir.exists();

    // 1. Check gh PR if available
    let pr = prs.and_then(|list| find_pr_for_item(list, item_id));
    let pr_number = pr.map(|p| p.number);
    let pr_url = pr.map(|p| p.url.clone());
    let pr_state = pr.map(|p| p.state.as_str());

    // State: Merged via PR
    if pr_state == Some("MERGED") {
        return GitItemInfo {
            id: item_id.to_string(),
            status: GitItemStatus::Merged,
            pr_number,
            pr_url,
            commit_oid: None,
            branch: if local_exists {
                Some(branch_name)
            } else {
                None
            },
            worktree_path: if wt_exists { Some(worktree_dir) } else { None },
        };
    }

    // State: Merged via Git Ancestry
    if local_exists && is_ancestor_of_any(repo_path, &branch_name, targets) {
        let commit_oid = get_branch_commit(repo_path, &branch_name);
        return GitItemInfo {
            id: item_id.to_string(),
            status: GitItemStatus::Merged,
            pr_number,
            pr_url,
            commit_oid,
            branch: Some(branch_name),
            worktree_path: if wt_exists { Some(worktree_dir) } else { None },
        };
    }
    if remote_exists && is_ancestor_of_any(repo_path, &remote_branch, targets) {
        let commit_oid = get_branch_commit(repo_path, &remote_branch);
        return GitItemInfo {
            id: item_id.to_string(),
            status: GitItemStatus::Merged,
            pr_number,
            pr_url,
            commit_oid,
            branch: Some(branch_name),
            worktree_path: if wt_exists { Some(worktree_dir) } else { None },
        };
    }

    // State: Merged via Squash Commit on default branch
    if let Some(squash_oid) = find_squash_commit(repo_path, item_id, targets) {
        return GitItemInfo {
            id: item_id.to_string(),
            status: GitItemStatus::Merged,
            pr_number,
            pr_url,
            commit_oid: Some(squash_oid),
            branch: if local_exists {
                Some(branch_name)
            } else {
                None
            },
            worktree_path: if wt_exists { Some(worktree_dir) } else { None },
        };
    }

    // 2. Check PrOpen
    if pr_state == Some("OPEN") {
        return GitItemInfo {
            id: item_id.to_string(),
            status: GitItemStatus::PrOpen,
            pr_number,
            pr_url,
            commit_oid: None,
            branch: if local_exists {
                Some(branch_name)
            } else {
                None
            },
            worktree_path: if wt_exists { Some(worktree_dir) } else { None },
        };
    }

    // 3. Check ChangesPending: worktree or repo contains uncommitted edits
    if wt_exists && has_uncommitted_changes(&worktree_dir) {
        return GitItemInfo {
            id: item_id.to_string(),
            status: GitItemStatus::ChangesPending,
            pr_number,
            pr_url,
            commit_oid: None,
            branch: if local_exists {
                Some(branch_name)
            } else {
                None
            },
            worktree_path: Some(worktree_dir),
        };
    }
    if is_current_branch(repo_path, &branch_name) && has_uncommitted_changes(repo_path) {
        return GitItemInfo {
            id: item_id.to_string(),
            status: GitItemStatus::ChangesPending,
            pr_number,
            pr_url,
            commit_oid: None,
            branch: Some(branch_name),
            worktree_path: None,
        };
    }

    // 4. Check Committed: commits exist on pin/<id>, but no PR is open
    let has_commits = (local_exists && has_commits_ahead(repo_path, &branch_name, targets))
        || (remote_exists && has_commits_ahead(repo_path, &remote_branch, targets));

    if has_commits {
        let commit_oid = if local_exists {
            get_branch_commit(repo_path, &branch_name)
        } else {
            get_branch_commit(repo_path, &remote_branch)
        };
        return GitItemInfo {
            id: item_id.to_string(),
            status: GitItemStatus::Committed,
            pr_number,
            pr_url,
            commit_oid,
            branch: Some(branch_name),
            worktree_path: if wt_exists { Some(worktree_dir) } else { None },
        };
    }

    // 5. None
    GitItemInfo {
        id: item_id.to_string(),
        status: GitItemStatus::None,
        pr_number,
        pr_url,
        commit_oid: None,
        branch: if local_exists {
            Some(branch_name)
        } else {
            None
        },
        worktree_path: if wt_exists { Some(worktree_dir) } else { None },
    }
}

pub struct GitStatusResolver {
    pub repo_path: PathBuf,
    pub targets: Vec<String>,
    pub prs: Option<Vec<GhPrInfo>>,
}

impl GitStatusResolver {
    pub fn new(repo_path: PathBuf) -> Self {
        let targets = resolve_target_branches(&repo_path);
        let prs = query_gh_prs(&repo_path);
        Self {
            repo_path,
            targets,
            prs,
        }
    }

    pub fn with_prs(repo_path: PathBuf, prs: Option<Vec<GhPrInfo>>) -> Self {
        let targets = resolve_target_branches(&repo_path);
        Self {
            repo_path,
            targets,
            prs,
        }
    }

    pub fn resolve_item(&self, item_id: &str) -> GitItemInfo {
        resolve_git_status(&self.repo_path, item_id, self.prs.as_deref(), &self.targets)
    }

    pub fn resolve_all(&self, items: &[IdeaMeta]) -> HashMap<String, GitItemInfo> {
        let mut map = HashMap::new();
        for item in items {
            map.insert(item.id.clone(), self.resolve_item(&item.id));
        }
        map
    }
}

pub fn reconcile_and_sync(
    vault_path: &Path,
    repo_path: &Path,
    filter_project: Option<&str>,
    target_id: Option<&str>,
    dry_run: bool,
    actor: Option<&str>,
) -> Result<Vec<SyncResult>, String> {
    let all_items = collect_ideas(vault_path).map_err(|e| format!("Failed to read vault: {e}"))?;

    let candidates: Vec<&IdeaMeta> = all_items
        .iter()
        .filter(|meta| {
            if meta.archived_at.is_some() {
                return false;
            }
            if let Some(proj) = filter_project {
                if !meta.project.eq_ignore_ascii_case(proj) {
                    return false;
                }
            }
            if let Some(target) = target_id {
                meta.id == target || meta.id.starts_with(target)
            } else {
                // Only completed work can have a merged PR to reconcile.
                meta.current_status() == Status::Done
            }
        })
        .collect();

    if candidates.is_empty() {
        return Ok(Vec::new());
    }

    let resolver = GitStatusResolver::new(repo_path.to_path_buf());
    let mut results = Vec::new();

    for meta in candidates {
        let git_info = resolver.resolve_item(&meta.id);
        if git_info.status != GitItemStatus::Merged {
            continue;
        }

        let resolution_note = if let Some(pr_num) = git_info.pr_number {
            format!("Merged in PR #{pr_num}")
        } else if let Some(oid) = &git_info.commit_oid {
            let short = if oid.len() >= 12 { &oid[..12] } else { oid };
            format!("Merged in commit {short}")
        } else {
            "Merged into main".to_string()
        };

        let wt_path = repo_path.join(".pin_worktrees").join(&meta.id);
        let branch_name = format!("pin/{}", meta.id);
        let had_worktree = wt_path.exists();
        let had_branch = branch_exists(repo_path, &branch_name);

        if !dry_run {
            workflow::archive_item(
                vault_path,
                &meta.filename,
                Resolution::Implemented,
                actor,
                Some(&resolution_note),
                None,
            )
            .map_err(|e| format!("Failed to archive {}: {e}", meta.id))?;

            let mut worktree_removed = false;
            if had_worktree && WorktreeManager::remove_worktree(repo_path, &meta.id).is_ok() {
                worktree_removed = true;
            }

            let mut branch_deleted = false;
            if had_branch {
                let del = Command::new("git")
                    .arg("-C")
                    .arg(repo_path)
                    .args(["branch", "-D", &branch_name])
                    .output();
                if let Ok(out) = del {
                    if out.status.success() {
                        branch_deleted = true;
                    }
                }
            }

            // Cleanup .pin_worktrees dir if empty
            let worktrees_dir = repo_path.join(".pin_worktrees");
            if worktrees_dir.exists() {
                let is_empty = fs::read_dir(&worktrees_dir)
                    .map(|mut entries| entries.next().is_none())
                    .unwrap_or(false);
                if is_empty {
                    let _ = fs::remove_dir(&worktrees_dir);
                }
            }

            results.push(SyncResult {
                id: meta.id.clone(),
                title: meta.title.clone(),
                status: "merged".to_string(),
                action: "archived".to_string(),
                resolution: "implemented".to_string(),
                resolution_note,
                worktree_removed,
                branch_deleted,
                pr_number: git_info.pr_number,
                pr_url: git_info.pr_url,
                commit_oid: git_info.commit_oid,
            });
        } else {
            results.push(SyncResult {
                id: meta.id.clone(),
                title: meta.title.clone(),
                status: "merged".to_string(),
                action: "dry_run".to_string(),
                resolution: "implemented".to_string(),
                resolution_note,
                worktree_removed: had_worktree,
                branch_deleted: had_branch,
                pr_number: git_info.pr_number,
                pr_url: git_info.pr_url,
                commit_oid: git_info.commit_oid,
            });
        }
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn setup_git_repo(path: &Path) {
        Command::new("git")
            .arg("-C")
            .arg(path)
            .args(["init", "-b", "main"])
            .output()
            .unwrap();
        Command::new("git")
            .arg("-C")
            .arg(path)
            .args(["config", "user.name", "Test"])
            .output()
            .unwrap();
        Command::new("git")
            .arg("-C")
            .arg(path)
            .args(["config", "user.email", "test@test.com"])
            .output()
            .unwrap();
        fs::write(path.join("README.md"), "# Initial").unwrap();
        Command::new("git")
            .arg("-C")
            .arg(path)
            .args(["add", "."])
            .output()
            .unwrap();
        Command::new("git")
            .arg("-C")
            .arg(path)
            .args(["commit", "-m", "initial commit"])
            .output()
            .unwrap();
    }

    #[test]
    fn test_resolve_git_status_regular_merge() {
        let dir = tempdir().unwrap();
        let repo = dir.path();
        setup_git_repo(repo);

        let item_id = "reg123456789";
        let branch = format!("pin/{item_id}");

        // Create feature branch
        Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["checkout", "-b", &branch])
            .output()
            .unwrap();
        fs::write(repo.join("feature.txt"), "feat").unwrap();
        Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["add", "."])
            .output()
            .unwrap();
        Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["commit", "-m", "feature commit"])
            .output()
            .unwrap();

        // Switch to main and merge with merge commit
        Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["checkout", "main"])
            .output()
            .unwrap();
        Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["merge", "--no-ff", &branch, "-m", "Merge feature branch"])
            .output()
            .unwrap();

        let resolver = GitStatusResolver::with_prs(repo.to_path_buf(), None);
        let status = resolver.resolve_item(item_id);
        assert_eq!(status.status, GitItemStatus::Merged);
        assert_eq!(status.id, item_id);
        assert!(status.commit_oid.is_some());
    }

    #[test]
    fn test_resolve_git_status_squash_merge() {
        let dir = tempdir().unwrap();
        let repo = dir.path();
        setup_git_repo(repo);

        let item_id = "sqsh12345678";
        let branch = format!("pin/{item_id}");

        // Create feature branch
        Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["checkout", "-b", &branch])
            .output()
            .unwrap();
        fs::write(repo.join("squash.txt"), "squash").unwrap();
        Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["add", "."])
            .output()
            .unwrap();
        Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["commit", "-m", "squash commit"])
            .output()
            .unwrap();

        // Switch to main and create a commit mentioning Item ID
        Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["checkout", "main"])
            .output()
            .unwrap();
        fs::write(repo.join("squash.txt"), "squash").unwrap();
        Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["add", "."])
            .output()
            .unwrap();
        Command::new("git")
            .arg("-C")
            .arg(repo)
            .args([
                "commit",
                "-m",
                &format!("Add squash feature\n\nItem ID: {item_id}"),
            ])
            .output()
            .unwrap();

        // Verify that ancestry check alone would fail
        assert!(!is_ancestor_of_any(repo, &branch, &["main".to_string()]));

        // But resolver detects it via commit message inspection!
        let resolver = GitStatusResolver::with_prs(repo.to_path_buf(), None);
        let status = resolver.resolve_item(item_id);
        assert_eq!(status.status, GitItemStatus::Merged);
        assert_eq!(status.id, item_id);
        assert!(status.commit_oid.is_some());
    }

    #[test]
    fn test_resolve_git_status_open_pr() {
        let dir = tempdir().unwrap();
        let repo = dir.path();
        setup_git_repo(repo);

        let item_id = "openpr123456";
        let prs = vec![GhPrInfo {
            number: 42,
            head_ref_name: format!("pin/{item_id}"),
            state: "OPEN".to_string(),
            url: "https://github.com/org/repo/pull/42".to_string(),
        }];

        let resolver = GitStatusResolver::with_prs(repo.to_path_buf(), Some(prs));
        let status = resolver.resolve_item(item_id);
        assert_eq!(status.status, GitItemStatus::PrOpen);
        assert_eq!(status.pr_number, Some(42));
        assert_eq!(
            status.pr_url.as_deref(),
            Some("https://github.com/org/repo/pull/42")
        );
    }

    #[test]
    fn test_resolve_git_status_committed_and_pending() {
        let dir = tempdir().unwrap();
        let repo = dir.path();
        setup_git_repo(repo);

        let item_id = "comm12345678";
        let wt = WorktreeManager::provision_worktree(repo, item_id).unwrap();

        // 1. Initially provisioned with 0 commits and 0 edits -> None
        let resolver = GitStatusResolver::with_prs(repo.to_path_buf(), None);
        let s1 = resolver.resolve_item(item_id);
        assert_eq!(s1.status, GitItemStatus::None);

        // 2. Uncommitted edits in worktree -> ChangesPending
        fs::write(wt.join("pending.txt"), "uncommitted").unwrap();
        let s2 = resolver.resolve_item(item_id);
        assert_eq!(s2.status, GitItemStatus::ChangesPending);

        // 3. Commit edits -> Committed
        Command::new("git")
            .arg("-C")
            .arg(&wt)
            .args(["add", "."])
            .output()
            .unwrap();
        Command::new("git")
            .arg("-C")
            .arg(&wt)
            .args(["commit", "-m", "worktree commit"])
            .output()
            .unwrap();

        let s3 = resolver.resolve_item(item_id);
        assert_eq!(s3.status, GitItemStatus::Committed);
    }

    #[test]
    fn test_reconcile_and_sync_cleanup() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        let vault = dir.path().join("vault");
        fs::create_dir_all(&repo).unwrap();
        fs::create_dir_all(&vault).unwrap();
        setup_git_repo(&repo);

        let item_id = "synctest1234";

        // Create a completed item in the vault
        let item = IdeaMeta::new_work_item(
            item_id.to_string(),
            "pin".to_string(),
            "Sync test item".to_string(),
            "Body".to_string(),
            crate::model::Kind::Technical,
            crate::model::WorkType::Task,
            Status::Done,
            None,
            None,
            Some("agent:test".to_string()),
        );
        let file_path = vault.join(format!("{item_id}.md"));
        fs::write(&file_path, crate::frontmatter::render_full_document(&item)).unwrap();

        // Provision a worktree
        let wt = WorktreeManager::provision_worktree(&repo, item_id).unwrap();
        assert!(wt.exists());
        assert!(branch_exists(&repo, &format!("pin/{item_id}")));

        // Simulate squash merge on main
        fs::write(repo.join("synced.txt"), "content").unwrap();
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["add", "."])
            .output()
            .unwrap();
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["commit", "-m", &format!("Squash merge item ({item_id})")])
            .output()
            .unwrap();

        // 1. Dry run first
        let dry_res = reconcile_and_sync(&vault, &repo, None, None, true, None).unwrap();
        assert_eq!(dry_res.len(), 1);
        assert_eq!(dry_res[0].action, "dry_run");
        assert_eq!(dry_res[0].status, "merged");
        // Dry run must NOT delete worktree or branch or archive item
        assert!(wt.exists());
        assert!(branch_exists(&repo, &format!("pin/{item_id}")));
        let content_dry = fs::read_to_string(&file_path).unwrap();
        let meta_dry = crate::frontmatter::parse_front_matter_detailed(
            &format!("{item_id}.md"),
            &content_dry,
            &mut Vec::new(),
        )
        .unwrap();
        assert!(meta_dry.archived_at.is_none());

        // 2. Real sync
        let real_res =
            reconcile_and_sync(&vault, &repo, None, None, false, Some("agent:sync")).unwrap();
        assert_eq!(real_res.len(), 1);
        assert_eq!(real_res[0].action, "archived");
        assert_eq!(real_res[0].resolution, "implemented");
        assert!(real_res[0].worktree_removed);
        assert!(real_res[0].branch_deleted);

        // Verification: Worktree removed, branch deleted, item archived!
        assert!(!wt.exists());
        assert!(!branch_exists(&repo, &format!("pin/{item_id}")));
        let content_synced = fs::read_to_string(&file_path).unwrap();
        let meta_synced = crate::frontmatter::parse_front_matter_detailed(
            &format!("{item_id}.md"),
            &content_synced,
            &mut Vec::new(),
        )
        .unwrap();
        assert!(meta_synced.archived_at.is_some());
        assert_eq!(meta_synced.resolution, Some(Resolution::Implemented));
        assert!(meta_synced
            .resolution_note
            .unwrap()
            .starts_with("Merged in commit "));
    }

    #[test]
    fn test_resolve_git_status_merged_pr() {
        let dir = tempdir().unwrap();
        let repo = dir.path();
        setup_git_repo(repo);

        let item_id = "mergedpr1234";
        let prs = vec![GhPrInfo {
            number: 99,
            head_ref_name: format!("pin/{item_id}"),
            state: "MERGED".to_string(),
            url: "https://github.com/org/repo/pull/99".to_string(),
        }];

        let resolver = GitStatusResolver::with_prs(repo.to_path_buf(), Some(prs));
        let status = resolver.resolve_item(item_id);
        assert_eq!(status.status, GitItemStatus::Merged);
        assert_eq!(status.pr_number, Some(99));
        assert_eq!(
            status.pr_url.as_deref(),
            Some("https://github.com/org/repo/pull/99")
        );
    }

    #[test]
    fn test_resolve_git_status_squash_merge_parens() {
        let dir = tempdir().unwrap();
        let repo = dir.path();
        setup_git_repo(repo);

        let item_id = "sqshparens12";
        let branch = format!("pin/{item_id}");

        // Create feature branch
        Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["checkout", "-b", &branch])
            .output()
            .unwrap();
        fs::write(repo.join("file.txt"), "feat").unwrap();
        Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["add", "."])
            .output()
            .unwrap();
        Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["commit", "-m", "feature commit"])
            .output()
            .unwrap();

        // Switch to main and squash merge with commit subject containing (item_id)
        Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["checkout", "main"])
            .output()
            .unwrap();
        fs::write(repo.join("file.txt"), "feat").unwrap();
        Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["add", "."])
            .output()
            .unwrap();
        Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["commit", "-m", &format!("feat: something new ({item_id})")])
            .output()
            .unwrap();

        let resolver = GitStatusResolver::with_prs(repo.to_path_buf(), None);
        let status = resolver.resolve_item(item_id);
        assert_eq!(status.status, GitItemStatus::Merged);
        assert!(status.commit_oid.is_some());
    }
}
