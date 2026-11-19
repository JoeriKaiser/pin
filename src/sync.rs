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
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub worktree_preserved_dirty: bool,
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
    query_gh_prs_with_command("gh", repo_path, GH_TIMEOUT)
}

fn query_gh_prs_with_command(
    program: &str,
    repo_path: &Path,
    timeout: Duration,
) -> Option<Vec<GhPrInfo>> {
    let mut child = Command::new(program)
        .current_dir(repo_path)
        .args([
            "pr",
            "list",
            "--state",
            "all",
            "--limit",
            "100",
            "--json",
            "number,headRefName,state,url",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    // Drain stdout on a helper thread so outputs larger than the OS pipe buffer
    // cannot block the child before try_wait sees it exit.
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        stdout.read_to_end(&mut buf).ok().map(|_| buf)
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return None;
            }
        }
    };

    let buf = reader.join().ok()??;
    if !status.success() {
        return None;
    }
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

fn is_on_first_parent_chain(
    repo_path: &Path,
    branch_oid: &str,
    branch: &str,
    target: &str,
) -> bool {
    // When `branch` is a strict ancestor of `target`, walking `--first-parent`
    // from `target` down to `^branch` stops at the child of `branch` if and
    // only if `branch` sits on `target`'s first-parent lineage (e.g. an
    // untouched worktree created before `target` moved ahead).
    let range = format!("{branch}..{target}");
    let Ok(out) = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["rev-list", "--first-parent", "--parents", &range])
        .output()
    else {
        return false;
    };
    if !out.status.success() {
        return false;
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let Some(oldest_line) = stdout.lines().rev().find(|l| !l.trim().is_empty()) else {
        return false;
    };
    oldest_line.split_whitespace().nth(1) == Some(branch_oid)
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
            if out.status.success() && !is_on_first_parent_chain(repo_path, &b_oid, branch, target)
            {
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

fn preload_pin_refs(repo_path: &Path) -> (HashMap<String, String>, HashMap<String, String>) {
    let mut local = HashMap::new();
    let mut remote = HashMap::new();
    let Ok(out) = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args([
            "for-each-ref",
            "--format=%(refname) %(objectname)",
            "refs/heads/pin/",
            "refs/remotes/origin/pin/",
        ])
        .output()
    else {
        return (local, remote);
    };
    if !out.status.success() {
        return (local, remote);
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    for line in stdout.lines() {
        let Some((refname, oid)) = line.trim().split_once(' ') else {
            continue;
        };
        if let Some(id) = refname.strip_prefix("refs/heads/pin/") {
            local.insert(id.to_string(), oid.to_string());
        } else if let Some(id) = refname.strip_prefix("refs/remotes/origin/pin/") {
            remote.insert(id.to_string(), oid.to_string());
        }
    }
    (local, remote)
}

fn current_head_branch(repo_path: &Path) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

pub struct GitStatusResolver {
    pub repo_path: PathBuf,
    pub targets: Vec<String>,
    pub prs: Option<Vec<GhPrInfo>>,
    local_pin_refs: HashMap<String, String>,
    remote_pin_refs: HashMap<String, String>,
    current_branch: Option<String>,
    target_oids: Vec<(String, Option<String>)>,
}

impl GitStatusResolver {
    pub fn new(repo_path: PathBuf) -> Self {
        let prs = query_gh_prs(&repo_path);
        Self::with_prs(repo_path, prs)
    }

    pub fn with_prs(repo_path: PathBuf, prs: Option<Vec<GhPrInfo>>) -> Self {
        let targets = resolve_target_branches(&repo_path);
        let (local_pin_refs, remote_pin_refs) = preload_pin_refs(&repo_path);
        let current_branch = current_head_branch(&repo_path);
        let target_oids = targets
            .iter()
            .map(|t| (t.clone(), get_branch_commit(&repo_path, t)))
            .collect();
        Self {
            repo_path,
            targets,
            prs,
            local_pin_refs,
            remote_pin_refs,
            current_branch,
            target_oids,
        }
    }

    fn is_ref_merged(&self, branch: &str, branch_oid: &str) -> bool {
        for (target, t_oid_opt) in &self.target_oids {
            if t_oid_opt.as_deref() == Some(branch_oid) {
                continue;
            }
            if let Ok(out) = Command::new("git")
                .arg("-C")
                .arg(&self.repo_path)
                .args(["merge-base", "--is-ancestor", branch, target])
                .output()
            {
                if out.status.success()
                    && !is_on_first_parent_chain(&self.repo_path, branch_oid, branch, target)
                {
                    return true;
                }
            }
        }
        false
    }

    fn resolve_item_inner(&self, item_id: &str, check_squash: bool) -> GitItemInfo {
        let branch_name = format!("pin/{item_id}");
        let remote_branch = format!("origin/pin/{item_id}");
        let worktree_dir = self.repo_path.join(".pin_worktrees").join(item_id);

        let local_oid = self.local_pin_refs.get(item_id).cloned();
        let remote_oid = self.remote_pin_refs.get(item_id).cloned();
        let local_exists = local_oid.is_some();
        let remote_exists = remote_oid.is_some();
        let wt_exists = worktree_dir.exists();

        let pr = self
            .prs
            .as_deref()
            .and_then(|list| find_pr_for_item(list, item_id));
        let pr_number = pr.map(|p| p.number);
        let pr_url = pr.map(|p| p.url.clone());
        let pr_state = pr.map(|p| p.state.as_str());

        if pr_state == Some("MERGED") {
            return GitItemInfo {
                id: item_id.to_string(),
                status: GitItemStatus::Merged,
                pr_number,
                pr_url,
                commit_oid: None,
                branch: local_exists.then_some(branch_name),
                worktree_path: wt_exists.then_some(worktree_dir),
            };
        }

        if let Some(ref b_oid) = local_oid {
            if self.is_ref_merged(&branch_name, b_oid) {
                return GitItemInfo {
                    id: item_id.to_string(),
                    status: GitItemStatus::Merged,
                    pr_number,
                    pr_url,
                    commit_oid: Some(b_oid.clone()),
                    branch: Some(branch_name),
                    worktree_path: wt_exists.then_some(worktree_dir),
                };
            }
        }
        if let Some(ref r_oid) = remote_oid {
            if self.is_ref_merged(&remote_branch, r_oid) {
                return GitItemInfo {
                    id: item_id.to_string(),
                    status: GitItemStatus::Merged,
                    pr_number,
                    pr_url,
                    commit_oid: Some(r_oid.clone()),
                    branch: Some(branch_name),
                    worktree_path: wt_exists.then_some(worktree_dir),
                };
            }
        }

        if check_squash {
            if let Some(squash_oid) = find_squash_commit(&self.repo_path, item_id, &self.targets) {
                return GitItemInfo {
                    id: item_id.to_string(),
                    status: GitItemStatus::Merged,
                    pr_number,
                    pr_url,
                    commit_oid: Some(squash_oid),
                    branch: local_exists.then_some(branch_name),
                    worktree_path: wt_exists.then_some(worktree_dir),
                };
            }
        }

        if pr_state == Some("OPEN") {
            return GitItemInfo {
                id: item_id.to_string(),
                status: GitItemStatus::PrOpen,
                pr_number,
                pr_url,
                commit_oid: None,
                branch: local_exists.then_some(branch_name),
                worktree_path: wt_exists.then_some(worktree_dir),
            };
        }

        if wt_exists && has_uncommitted_changes(&worktree_dir) {
            return GitItemInfo {
                id: item_id.to_string(),
                status: GitItemStatus::ChangesPending,
                pr_number,
                pr_url,
                commit_oid: None,
                branch: local_exists.then_some(branch_name),
                worktree_path: Some(worktree_dir),
            };
        }
        if self.current_branch.as_deref() == Some(branch_name.as_str())
            && has_uncommitted_changes(&self.repo_path)
        {
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

        let has_commits = (local_exists
            && has_commits_ahead(&self.repo_path, &branch_name, &self.targets))
            || (remote_exists && has_commits_ahead(&self.repo_path, &remote_branch, &self.targets));

        if has_commits {
            return GitItemInfo {
                id: item_id.to_string(),
                status: GitItemStatus::Committed,
                pr_number,
                pr_url,
                commit_oid: local_oid.or(remote_oid),
                branch: Some(branch_name),
                worktree_path: wt_exists.then_some(worktree_dir),
            };
        }

        GitItemInfo {
            id: item_id.to_string(),
            status: GitItemStatus::None,
            pr_number,
            pr_url,
            commit_oid: None,
            branch: local_exists.then_some(branch_name),
            worktree_path: wt_exists.then_some(worktree_dir),
        }
    }

    pub fn resolve_item(&self, item_id: &str) -> GitItemInfo {
        self.resolve_item_inner(item_id, true)
    }

    pub fn resolve_all(&self, items: &[IdeaMeta]) -> HashMap<String, GitItemInfo> {
        let mut map = HashMap::with_capacity(items.len());
        for item in items {
            let has_local_or_remote = self.local_pin_refs.contains_key(&item.id)
                || self.remote_pin_refs.contains_key(&item.id);
            let has_wt = self
                .repo_path
                .join(".pin_worktrees")
                .join(&item.id)
                .exists();
            let check_squash =
                item.current_status() == Status::Done || has_local_or_remote || has_wt;
            map.insert(
                item.id.clone(),
                self.resolve_item_inner(&item.id, check_squash),
            );
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
    force: bool,
    actor: Option<&str>,
) -> Result<Vec<SyncResult>, String> {
    let resolved_target_file = match target_id {
        Some(target) => {
            Some(crate::vault::resolve_selector(vault_path, target).map_err(|e| e.to_string())?)
        }
        None => None,
    };

    let all_items = collect_ideas(vault_path).map_err(|e| format!("Failed to read vault: {e}"))?;
    let (local_pin_refs, _) = preload_pin_refs(repo_path);

    let candidates: Vec<&IdeaMeta> = all_items
        .iter()
        .filter(|meta| {
            if let Some(proj) = filter_project {
                if !meta.project.eq_ignore_ascii_case(proj) {
                    return false;
                }
            }
            if let Some(ref target_file) = resolved_target_file {
                meta.filename == *target_file
            } else {
                // Completed work, or any work item that still has a stranded worktree or branch in this repo
                meta.current_status() == Status::Done
                    || repo_path.join(".pin_worktrees").join(&meta.id).exists()
                    || local_pin_refs.contains_key(&meta.id)
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
        let is_merged = git_info.status == GitItemStatus::Merged;
        let is_archived = meta.is_archived();
        let is_archived_or_closed = is_archived
            || meta.current_status() == Status::Closed
            || meta.current_status() == Status::Cancelled;

        if !is_merged && !is_archived_or_closed {
            continue;
        }

        let effective_resolution = if is_merged {
            Resolution::Implemented
        } else if let Some(res) = meta.resolution {
            res
        } else if meta.current_status() == Status::Cancelled {
            Resolution::Rejected
        } else {
            Resolution::Implemented
        };

        let resolution_note = if let Some(pr_num) = git_info.pr_number {
            format!("Merged in PR #{pr_num}")
        } else if let Some(oid) = &git_info.commit_oid {
            let short = if oid.len() >= 12 { &oid[..12] } else { oid };
            format!("Merged in commit {short}")
        } else if is_merged {
            "Merged into main".to_string()
        } else {
            meta.resolution_note
                .clone()
                .unwrap_or_else(|| format!("Archived ({})", effective_resolution.as_str()))
        };
        let wt_path = repo_path.join(".pin_worktrees").join(&meta.id);
        let branch_name = format!("pin/{}", meta.id);
        let had_worktree = wt_path.exists();
        let had_branch = branch_exists(repo_path, &branch_name);

        if is_archived && !had_worktree && !had_branch {
            continue;
        }
        if !is_merged && !had_worktree && !had_branch {
            continue;
        }

        let is_dirty = had_worktree && has_uncommitted_changes(&wt_path);
        let should_remove_worktree = had_worktree && (!is_dirty || force);
        let worktree_preserved_dirty = had_worktree && is_dirty && !force;

        if worktree_preserved_dirty {
            results.push(SyncResult {
                id: meta.id.clone(),
                title: meta.title.clone(),
                status: if is_merged {
                    "merged".to_string()
                } else {
                    "archived".to_string()
                },
                action: if dry_run {
                    "dry_run".to_string()
                } else {
                    "skipped_dirty".to_string()
                },
                resolution: effective_resolution.as_str().to_string(),
                resolution_note,
                worktree_removed: false,
                branch_deleted: false,
                pr_number: git_info.pr_number,
                pr_url: git_info.pr_url,
                commit_oid: git_info.commit_oid,
                worktree_preserved_dirty: true,
            });
            continue;
        }

        if !dry_run {
            if !is_archived {
                workflow::archive_item(
                    vault_path,
                    &meta.filename,
                    effective_resolution,
                    actor,
                    Some(&resolution_note),
                    None,
                )
                .map_err(|e| format!("Failed to archive {}: {e}", meta.id))?;
            }
            let mut worktree_removed = false;
            if should_remove_worktree
                && WorktreeManager::remove_worktree(repo_path, &meta.id).is_ok()
            {
                worktree_removed = true;
            }

            let mut branch_deleted = false;
            if had_branch && !worktree_preserved_dirty {
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
                status: if is_merged {
                    "merged".to_string()
                } else {
                    "archived".to_string()
                },
                action: if is_archived {
                    "synced".to_string()
                } else {
                    "archived".to_string()
                },
                resolution: effective_resolution.as_str().to_string(),
                resolution_note: resolution_note.clone(),
                worktree_removed,
                branch_deleted,
                pr_number: git_info.pr_number,
                pr_url: git_info.pr_url,
                commit_oid: git_info.commit_oid,
                worktree_preserved_dirty,
            });
        } else {
            results.push(SyncResult {
                id: meta.id.clone(),
                title: meta.title.clone(),
                status: if is_merged {
                    "merged".to_string()
                } else {
                    "archived".to_string()
                },
                action: "dry_run".to_string(),
                resolution: effective_resolution.as_str().to_string(),
                resolution_note,
                worktree_removed: should_remove_worktree,
                branch_deleted: had_branch && !worktree_preserved_dirty,
                pr_number: git_info.pr_number,
                pr_url: git_info.pr_url,
                commit_oid: git_info.commit_oid,
                worktree_preserved_dirty,
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
        let dry_res = reconcile_and_sync(&vault, &repo, None, None, true, false, None).unwrap();
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
            reconcile_and_sync(&vault, &repo, None, None, false, false, Some("agent:sync"))
                .unwrap();
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

    #[test]
    fn test_reconcile_and_sync_preserves_dirty_worktree() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        let vault = dir.path().join("vault");
        fs::create_dir_all(&repo).unwrap();
        fs::create_dir_all(&vault).unwrap();
        setup_git_repo(&repo);

        let item_id = "dirtywt12345";
        let file_path = vault.join(format!("{item_id}.md"));
        let mut meta = IdeaMeta::new_work_item(
            item_id.to_string(),
            "test-proj".to_string(),
            "Dirty WT Test".to_string(),
            "Body".to_string(),
            crate::model::Kind::Technical,
            crate::model::WorkType::Task,
            Status::Done,
            None,
            None,
            Some("human:tester".to_string()),
        );
        meta.handoff = Some(crate::model::Handoff {
            progress: None,
            next: None,
            blocker: None,
            verification: Some("passes".to_string()),
        });
        let rendered = crate::frontmatter::render_full_document(&meta);
        fs::write(&file_path, rendered).unwrap();

        // Provision worktree and branch
        let wt = WorktreeManager::provision_worktree(&repo, item_id).unwrap();

        // Merge into main via squash commit
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["checkout", "main"])
            .output()
            .unwrap();
        fs::write(repo.join("other.txt"), "merged").unwrap();
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

        // Create an untracked scratch file in the worktree
        let scratch = wt.join("uncommitted_scratch.txt");
        fs::write(&scratch, "important notes").unwrap();
        assert!(has_uncommitted_changes(&wt));

        // Sync without force: must preserve the dirty worktree
        let res_no_force =
            reconcile_and_sync(&vault, &repo, None, None, false, false, Some("agent:sync"))
                .unwrap();
        assert_eq!(res_no_force.len(), 1);
        assert!(res_no_force[0].worktree_preserved_dirty);
        assert!(!res_no_force[0].worktree_removed);
        assert!(wt.exists());
        assert!(scratch.exists());
        assert_eq!(fs::read_to_string(&scratch).unwrap(), "important notes");

        // Sync with force: deletes the worktree
        let res_force =
            reconcile_and_sync(&vault, &repo, None, None, false, true, Some("agent:sync")).unwrap();
        assert_eq!(res_force.len(), 1);
        assert!(!res_force[0].worktree_preserved_dirty);
        assert!(res_force[0].worktree_removed);
        assert!(!wt.exists());
    }

    #[test]
    fn test_reconcile_and_sync_cleans_up_archived_item_worktree() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        let vault = dir.path().join("vault");
        fs::create_dir_all(&repo).unwrap();
        fs::create_dir_all(&vault).unwrap();
        setup_git_repo(&repo);

        let item_id = "archivedwt12";
        let file_path = vault.join(format!("{item_id}.md"));
        let mut meta = IdeaMeta::new_work_item(
            item_id.to_string(),
            "test-proj".to_string(),
            "Archived WT Test".to_string(),
            "Body".to_string(),
            crate::model::Kind::Technical,
            crate::model::WorkType::Task,
            Status::Done,
            None,
            None,
            Some("human:tester".to_string()),
        );
        meta.archived_at = Some(1780000000);
        meta.resolution = Some(Resolution::Implemented);
        let rendered = crate::frontmatter::render_full_document(&meta);
        fs::write(&file_path, rendered).unwrap();

        // Provision worktree and branch
        let wt = WorktreeManager::provision_worktree(&repo, item_id).unwrap();
        assert!(wt.exists());
        assert!(branch_exists(&repo, &format!("pin/{item_id}")));

        // Simulate squash merge on main
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["checkout", "main"])
            .output()
            .unwrap();
        fs::write(repo.join("file.txt"), "merged").unwrap();
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["add", "."])
            .output()
            .unwrap();
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["commit", "-m", &format!("feat: merged ({item_id})")])
            .output()
            .unwrap();

        // Sync must clean up the worktree and branch even though item is already archived
        let res = reconcile_and_sync(&vault, &repo, None, None, false, false, Some("agent:sync"))
            .unwrap();
        assert_eq!(res.len(), 1);
        assert!(res[0].worktree_removed);
        assert!(res[0].branch_deleted);
        assert!(!wt.exists());
        assert!(!branch_exists(&repo, &format!("pin/{item_id}")));
    }

    #[test]
    fn test_reconcile_and_sync_cleans_up_rejected_archived_worktree() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        let vault = dir.path().join("vault");
        fs::create_dir_all(&repo).unwrap();
        fs::create_dir_all(&vault).unwrap();
        setup_git_repo(&repo);

        let item_id = "rejectedwt12";
        let file_path = vault.join(format!("{item_id}.md"));
        let mut meta = IdeaMeta::new_work_item(
            item_id.to_string(),
            "test-proj".to_string(),
            "Rejected WT Test".to_string(),
            "Body".to_string(),
            crate::model::Kind::Technical,
            crate::model::WorkType::Task,
            Status::Closed,
            None,
            None,
            Some("human:tester".to_string()),
        );
        meta.archived_at = Some(1780000000);
        meta.resolution = Some(Resolution::Rejected);
        meta.resolution_note = Some("Abandoning approach".to_string());
        let rendered = crate::frontmatter::render_full_document(&meta);
        fs::write(&file_path, rendered).unwrap();

        // Provision worktree and branch without any merge commit on main
        let wt = WorktreeManager::provision_worktree(&repo, item_id).unwrap();
        assert!(wt.exists());
        assert!(branch_exists(&repo, &format!("pin/{item_id}")));

        // Sync must clean up the unmerged worktree and branch because the item is archived/closed
        let res = reconcile_and_sync(&vault, &repo, None, None, false, false, Some("agent:sync"))
            .unwrap();
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].status, "archived");
        assert_eq!(res[0].resolution, "rejected");
        assert!(res[0].worktree_removed);
        assert!(res[0].branch_deleted);
        assert!(!wt.exists());
        assert!(!branch_exists(&repo, &format!("pin/{item_id}")));
    }

    #[cfg(unix)]
    #[test]
    fn test_query_gh_prs_uses_cwd_and_drains_large_stdout() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        fs::create_dir_all(&repo).unwrap();

        let fake_gh = dir.path().join("fake-gh");
        // Verify we never receive `-C`, verify cwd matches repo, and emit >128 KiB JSON
        // to prove stdout draining does not deadlock the OS pipe buffer.
        let script = r#"#!/bin/sh
for arg in "$@"; do
  if [ "$arg" = "-C" ]; then
    echo "unexpected -C flag" >&2
    exit 2
  fi
done
if [ "$(pwd -P)" != "$(cd "$EXPECTED_CWD" && pwd -P)" ]; then
  echo "wrong cwd" >&2
  exit 3
fi
printf '['
i=0
while [ "$i" -lt 1200 ]; do
  if [ "$i" -gt 0 ]; then printf ','; fi
  printf '{"number":%d,"headRefName":"pin/item%08d","state":"OPEN","url":"https://github.com/org/repo/pull/%d/extra-padding-to-exceed-pipe-buffer-size"}' "$i" "$i" "$i"
  i=$((i + 1))
done
printf ']'
"#;
        fs::write(&fake_gh, script).unwrap();
        let mut perms = fs::metadata(&fake_gh).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&fake_gh, perms).unwrap();

        // Pass EXPECTED_CWD via repo path check inside the script by embedding it
        let script_with_path = script.replace(
            "$EXPECTED_CWD",
            &repo.to_string_lossy().replace('"', "\\\""),
        );
        fs::write(&fake_gh, script_with_path).unwrap();

        let prs =
            query_gh_prs_with_command(fake_gh.to_str().unwrap(), &repo, Duration::from_secs(3))
                .expect("fake gh should return parsed PR list");
        assert_eq!(prs.len(), 1200);
        assert_eq!(prs[0].number, 0);
        assert_eq!(prs[1199].head_ref_name, "pin/item00001199");
    }

    #[test]
    fn test_untouched_worktree_not_marked_merged_when_main_advances() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        let vault = dir.path().join("vault");
        fs::create_dir_all(&repo).unwrap();
        fs::create_dir_all(&vault).unwrap();
        setup_git_repo(&repo);

        let item_id = "untouched123";
        let file_path = vault.join(format!("{item_id}.md"));
        let meta = IdeaMeta::new_work_item(
            item_id.to_string(),
            "test-proj".to_string(),
            "Active task".to_string(),
            "Body".to_string(),
            crate::model::Kind::Technical,
            crate::model::WorkType::Task,
            Status::InProgress,
            None,
            None,
            Some("human:tester".to_string()),
        );
        fs::write(&file_path, crate::frontmatter::render_full_document(&meta)).unwrap();

        // Provision a fresh worktree at commit C1 (no commits on branch yet)
        let wt = WorktreeManager::provision_worktree(&repo, item_id).unwrap();
        assert!(wt.exists());

        // Advance main to C2 while the worktree is untouched
        fs::write(repo.join("unrelated.txt"), "main moved ahead").unwrap();
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["add", "."])
            .output()
            .unwrap();
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["commit", "-m", "unrelated commit on main"])
            .output()
            .unwrap();

        let resolver = GitStatusResolver::with_prs(repo.clone(), None);
        let status = resolver.resolve_item(item_id);
        assert_eq!(status.status, GitItemStatus::None);

        let res = reconcile_and_sync(&vault, &repo, None, None, false, false, Some("agent:sync"))
            .unwrap();
        assert!(
            res.is_empty(),
            "untouched active worktree must not be synced"
        );
        assert!(wt.exists(), "untouched active worktree must not be deleted");
        assert!(branch_exists(&repo, &format!("pin/{item_id}")));
        let disk = fs::read_to_string(&file_path).unwrap();
        assert!(!disk.contains("archived_at:"));
    }

    #[test]
    fn test_cancelled_item_sync_uses_rejected_resolution_and_supports_filename_selector() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        let vault = dir.path().join("vault");
        fs::create_dir_all(&repo).unwrap();
        fs::create_dir_all(&vault).unwrap();
        setup_git_repo(&repo);

        let item_id = "cancelwt1234";
        let file_path = vault.join(format!("{item_id}.md"));
        let meta = IdeaMeta::new_work_item(
            item_id.to_string(),
            "test-proj".to_string(),
            "Cancelled Task".to_string(),
            "Body".to_string(),
            crate::model::Kind::Technical,
            crate::model::WorkType::Task,
            Status::Cancelled,
            None,
            None,
            Some("human:tester".to_string()),
        );
        fs::write(&file_path, crate::frontmatter::render_full_document(&meta)).unwrap();

        let wt = WorktreeManager::provision_worktree(&repo, item_id).unwrap();
        assert!(wt.exists());

        // Unknown selector must error instead of silently returning empty
        assert!(reconcile_and_sync(
            &vault,
            &repo,
            None,
            Some("doesnotexist"),
            false,
            false,
            Some("agent:sync"),
        )
        .is_err());

        // Filename selector works and records rejected resolution on disk and in SyncResult
        let res = reconcile_and_sync(
            &vault,
            &repo,
            None,
            Some(&format!("{item_id}.md")),
            false,
            false,
            Some("agent:sync"),
        )
        .unwrap();
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].resolution, "rejected");
        assert!(res[0].worktree_removed);
        assert!(res[0].branch_deleted);
        let disk = fs::read_to_string(&file_path).unwrap();
        assert!(disk.contains("resolution: \"rejected\""));
    }
}
