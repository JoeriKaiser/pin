use crate::frontmatter::{parse_front_matter_detailed, render_full_document};
use crate::model::{ActivityEvent, IdeaMeta, Resolution, Status, WorkType};
use crate::vault::{atomic_write, collect_ideas};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub enum WorkflowError {
    Io(io::Error),
    LockConflict(String),
    RevisionConflict {
        expected: u64,
        actual: u64,
    },
    #[allow(dead_code)]
    InvalidTransition {
        from: Status,
        to: Status,
        reason: String,
    },
    ClaimConflict {
        claimed_by: String,
        expires_at: i64,
    },
    NotClaimed,
    PermissionDenied(String),
    MissingEvidence,
    DependencyCycle(String),
    ItemNotFound(String),
    ParseError(String),
}

impl fmt::Display for WorkflowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WorkflowError::Io(e) => write!(f, "IO error: {e}"),
            WorkflowError::LockConflict(msg) => write!(f, "Lock conflict: {msg}"),
            WorkflowError::RevisionConflict { expected, actual } => {
                write!(
                    f,
                    "Revision conflict: expected revision {expected}, but item is at revision {actual}"
                )
            }
            WorkflowError::InvalidTransition { from, to, reason } => {
                write!(f, "Invalid transition from '{from}' to '{to}': {reason}")
            }
            WorkflowError::ClaimConflict {
                claimed_by,
                expires_at,
            } => {
                write!(
                    f,
                    "Item is already claimed by '{claimed_by}' (claim expires at {expires_at})"
                )
            }
            WorkflowError::NotClaimed => write!(f, "Item is not currently claimed"),
            WorkflowError::PermissionDenied(msg) => write!(f, "Permission denied: {msg}"),
            WorkflowError::MissingEvidence => {
                write!(f, "Completion requires non-empty verification evidence")
            }
            WorkflowError::DependencyCycle(msg) => write!(f, "Dependency cycle detected: {msg}"),
            WorkflowError::ItemNotFound(id) => write!(f, "Item '{id}' not found"),
            WorkflowError::ParseError(msg) => write!(f, "Failed to parse item: {msg}"),
        }
    }
}

impl std::error::Error for WorkflowError {}

impl From<io::Error> for WorkflowError {
    fn from(e: io::Error) -> Self {
        WorkflowError::Io(e)
    }
}

pub struct FileLock {
    path: PathBuf,
}

impl FileLock {
    pub fn acquire(vault_path: &Path, filename: &str) -> Result<Self, WorkflowError> {
        let lock_filename = format!(".{filename}.lock");
        let lock_path = vault_path.join(lock_filename);
        let start = Instant::now();
        let timeout = Duration::from_secs(3);

        while start.elapsed() < timeout {
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&lock_path)
            {
                Ok(mut file) => {
                    let pid = std::process::id();
                    let now = chrono::Utc::now().timestamp();
                    let _ = writeln!(file, "{pid}:{now}");
                    return Ok(FileLock { path: lock_path });
                }
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                    // Check if stale (older than 30 seconds)
                    if let Ok(metadata) = fs::metadata(&lock_path) {
                        if let Ok(modified) = metadata.modified() {
                            if let Ok(elapsed) = modified.elapsed() {
                                if elapsed > Duration::from_secs(30) {
                                    let _ = fs::remove_file(&lock_path);
                                    continue;
                                }
                            }
                        }
                    }
                    thread::sleep(Duration::from_millis(50));
                }
                Err(err) => return Err(WorkflowError::Io(err)),
            }
        }

        Err(WorkflowError::LockConflict(format!(
            "Could not acquire lock on {filename} within 3 seconds"
        )))
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn load_and_lock(
    vault_path: &Path,
    filename: &str,
    expected_revision: Option<u64>,
) -> Result<(IdeaMeta, FileLock), WorkflowError> {
    let lock = FileLock::acquire(vault_path, filename)?;
    let file_path = vault_path.join(filename);
    let content = fs::read_to_string(&file_path).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            WorkflowError::ItemNotFound(filename.to_string())
        } else {
            WorkflowError::Io(e)
        }
    })?;

    let mut issues = Vec::new();
    let meta = parse_front_matter_detailed(filename, &content, &mut issues)
        .ok_or_else(|| WorkflowError::ParseError(format!("Failed to parse {filename}")))?;

    if let Some(expected) = expected_revision {
        let current = meta.current_revision();
        if current != expected {
            return Err(WorkflowError::RevisionConflict {
                expected,
                actual: current,
            });
        }
    }

    Ok((meta, lock))
}

fn save_item(vault_path: &Path, meta: &mut IdeaMeta) -> Result<(), WorkflowError> {
    let now = chrono::Utc::now().timestamp();
    meta.updated_at = Some(now);
    meta.revision = Some(meta.current_revision() + 1);

    let doc = render_full_document(meta);
    let file_path = vault_path.join(&meta.filename);
    atomic_write(&file_path, &doc).map_err(WorkflowError::Io)
}

pub fn transition_item(
    vault_path: &Path,
    filename: &str,
    target_status: Status,
    actor: Option<&str>,
    note: Option<&str>,
    expect_revision: Option<u64>,
) -> Result<IdeaMeta, WorkflowError> {
    let (mut meta, _lock) = load_and_lock(vault_path, filename, expect_revision)?;
    let current_status = meta.current_status();

    if target_status == Status::Done {
        // Must provide evidence or note when marking done directly
        let has_evidence = note.is_some_and(|n| !n.trim().is_empty())
            || meta
                .handoff
                .as_ref()
                .and_then(|h| h.verification.as_deref())
                .is_some_and(|v| !v.trim().is_empty());
        if !has_evidence {
            return Err(WorkflowError::MissingEvidence);
        }
    }

    let actor_str = actor.unwrap_or("agent:default").to_string();
    let now = chrono::Utc::now().timestamp();

    if target_status == Status::Done || target_status.is_terminal() {
        // Release any active claim upon completion or closure
        meta.claimed_by = None;
        meta.claim_expires_at = None;
    }

    meta.status = Some(target_status);
    meta.activity.push(ActivityEvent {
        at: now,
        actor: actor_str,
        action: "transition".to_string(),
        from: Some(current_status),
        to: Some(target_status),
        note: note.map(|s| s.to_string()),
    });

    save_item(vault_path, &mut meta)?;
    Ok(meta)
}

pub fn claim_item(
    vault_path: &Path,
    filename: &str,
    actor: &str,
    lease_seconds: u64,
    expect_revision: Option<u64>,
) -> Result<IdeaMeta, WorkflowError> {
    let (mut meta, _lock) = load_and_lock(vault_path, filename, expect_revision)?;
    let now = chrono::Utc::now().timestamp();

    if meta.has_active_claim(now) {
        let current_claimer = meta.claimed_by.as_deref().unwrap_or("");
        if current_claimer != actor {
            let expires = meta.claim_expires_at.unwrap_or(0);
            return Err(WorkflowError::ClaimConflict {
                claimed_by: current_claimer.to_string(),
                expires_at: expires,
            });
        }
    }

    let from_status = meta.current_status();
    meta.status = Some(Status::InProgress);
    meta.claimed_by = Some(actor.to_string());
    meta.claim_expires_at = Some(now + (lease_seconds as i64));

    meta.activity.push(ActivityEvent {
        at: now,
        actor: actor.to_string(),
        action: "claimed".to_string(),
        from: Some(from_status),
        to: Some(Status::InProgress),
        note: Some(format!("Lease: {lease_seconds}s")),
    });

    save_item(vault_path, &mut meta)?;
    Ok(meta)
}

pub fn release_item(
    vault_path: &Path,
    filename: &str,
    actor: Option<&str>,
    force: bool,
    expect_revision: Option<u64>,
) -> Result<IdeaMeta, WorkflowError> {
    let (mut meta, _lock) = load_and_lock(vault_path, filename, expect_revision)?;
    let now = chrono::Utc::now().timestamp();

    if let Some(current_claimer) = &meta.claimed_by {
        if let Some(req_actor) = actor {
            if current_claimer != req_actor && !force && meta.has_active_claim(now) {
                return Err(WorkflowError::PermissionDenied(format!(
                    "Item claimed by '{current_claimer}'. Use --force to release."
                )));
            }
        }
    } else {
        return Err(WorkflowError::NotClaimed);
    }

    let from_status = meta.current_status();
    let actor_str = actor.unwrap_or("agent:default").to_string();

    meta.claimed_by = None;
    meta.claim_expires_at = None;
    meta.status = Some(Status::Planned);

    meta.activity.push(ActivityEvent {
        at: now,
        actor: actor_str,
        action: "released".to_string(),
        from: Some(from_status),
        to: Some(Status::Planned),
        note: None,
    });

    save_item(vault_path, &mut meta)?;
    Ok(meta)
}

pub fn handoff_item(
    vault_path: &Path,
    filename: &str,
    actor: Option<&str>,
    progress: Option<&str>,
    next: Option<&str>,
    blocker: Option<&str>,
    verification: Option<&str>,
    expect_revision: Option<u64>,
) -> Result<IdeaMeta, WorkflowError> {
    let (mut meta, _lock) = load_and_lock(vault_path, filename, expect_revision)?;
    let now = chrono::Utc::now().timestamp();
    let actor_str = actor.unwrap_or("agent:default").to_string();

    let mut handoff = meta.handoff.take().unwrap_or_default();
    if let Some(p) = progress {
        handoff.progress = Some(p.to_string());
    }
    if let Some(n) = next {
        handoff.next = Some(n.to_string());
    }
    if let Some(b) = blocker {
        handoff.blocker = Some(b.to_string());
    }
    if let Some(v) = verification {
        handoff.verification = Some(v.to_string());
    }
    meta.handoff = Some(handoff);

    meta.activity.push(ActivityEvent {
        at: now,
        actor: actor_str,
        action: "handoff".to_string(),
        from: Some(meta.current_status()),
        to: Some(meta.current_status()),
        note: progress.map(|s| s.to_string()),
    });

    save_item(vault_path, &mut meta)?;
    Ok(meta)
}

pub fn complete_item(
    vault_path: &Path,
    filename: &str,
    actor: Option<&str>,
    evidence: &str,
    expect_revision: Option<u64>,
) -> Result<IdeaMeta, WorkflowError> {
    let trimmed = evidence.trim();
    if trimmed.is_empty() {
        return Err(WorkflowError::MissingEvidence);
    }

    let (mut meta, _lock) = load_and_lock(vault_path, filename, expect_revision)?;
    let from_status = meta.current_status();
    let now = chrono::Utc::now().timestamp();
    let actor_str = actor.unwrap_or("agent:default").to_string();

    // Release claim upon completion
    meta.claimed_by = None;
    meta.claim_expires_at = None;
    meta.status = Some(Status::Done);

    let mut handoff = meta.handoff.take().unwrap_or_default();
    handoff.verification = Some(trimmed.to_string());
    meta.handoff = Some(handoff);

    meta.activity.push(ActivityEvent {
        at: now,
        actor: actor_str,
        action: "completed".to_string(),
        from: Some(from_status),
        to: Some(Status::Done),
        note: Some(trimmed.to_string()),
    });

    save_item(vault_path, &mut meta)?;
    Ok(meta)
}

pub fn close_item(
    vault_path: &Path,
    filename: &str,
    actor: Option<&str>,
    note: Option<&str>,
    expect_revision: Option<u64>,
) -> Result<IdeaMeta, WorkflowError> {
    let (mut meta, _lock) = load_and_lock(vault_path, filename, expect_revision)?;
    let from_status = meta.current_status();
    let now = chrono::Utc::now().timestamp();
    let actor_str = actor.unwrap_or("agent:default").to_string();

    meta.claimed_by = None;
    meta.claim_expires_at = None;
    meta.status = Some(Status::Closed);

    meta.activity.push(ActivityEvent {
        at: now,
        actor: actor_str,
        action: "closed".to_string(),
        from: Some(from_status),
        to: Some(Status::Closed),
        note: note.map(|s| s.to_string()),
    });

    save_item(vault_path, &mut meta)?;
    Ok(meta)
}

pub fn archive_item(
    vault_path: &Path,
    filename: &str,
    resolution: Resolution,
    actor: Option<&str>,
    note: Option<&str>,
    expect_revision: Option<u64>,
) -> Result<IdeaMeta, WorkflowError> {
    let (mut meta, _lock) = load_and_lock(vault_path, filename, expect_revision)?;
    let from_status = meta.current_status();
    let now = chrono::Utc::now().timestamp();
    let actor_str = actor.unwrap_or("agent:default").to_string();

    meta.archived_at = Some(now);
    meta.resolution = Some(resolution);
    meta.resolution_note = note.map(|s| s.to_string());

    meta.claimed_by = None;
    meta.claim_expires_at = None;

    meta.activity.push(ActivityEvent {
        at: now,
        actor: actor_str,
        action: "archived".to_string(),
        from: Some(from_status),
        to: Some(meta.current_status()),
        note: note.map(|s| s.to_string()),
    });

    save_item(vault_path, &mut meta)?;
    Ok(meta)
}

pub fn unarchive_item(
    vault_path: &Path,
    filename: &str,
    actor: Option<&str>,
    expect_revision: Option<u64>,
) -> Result<IdeaMeta, WorkflowError> {
    let (mut meta, _lock) = load_and_lock(vault_path, filename, expect_revision)?;
    let from_status = meta.current_status();
    let now = chrono::Utc::now().timestamp();
    let actor_str = actor.unwrap_or("agent:default").to_string();

    meta.archived_at = None;
    meta.resolution = None;
    meta.resolution_note = None;

    meta.activity.push(ActivityEvent {
        at: now,
        actor: actor_str,
        action: "unarchived".to_string(),
        from: Some(from_status),
        to: Some(meta.current_status()),
        note: None,
    });

    save_item(vault_path, &mut meta)?;
    Ok(meta)
}

pub fn depend_item(
    vault_path: &Path,
    filename: &str,
    dependency_id: &str,
    actor: Option<&str>,
    expect_revision: Option<u64>,
) -> Result<IdeaMeta, WorkflowError> {
    let (mut meta, _lock) = load_and_lock(vault_path, filename, expect_revision)?;

    if meta.id == dependency_id {
        return Err(WorkflowError::DependencyCycle(
            "An item cannot depend on itself".to_string(),
        ));
    }

    if meta.depends_on.iter().any(|d| d == dependency_id) {
        return Ok(meta); // already depends on it
    }

    // Check for transitive cycles
    let all_items = collect_ideas(vault_path)?;
    let mut dep_map: HashMap<String, Vec<String>> = HashMap::new();
    for item in &all_items {
        dep_map.insert(item.id.clone(), item.depends_on.clone());
    }

    // Temporarily insert the proposed dependency
    let mut current_deps = meta.depends_on.clone();
    current_deps.push(dependency_id.to_string());
    dep_map.insert(meta.id.clone(), current_deps);

    // BFS to detect cycle starting from dependency_id
    let mut visited = HashSet::new();
    let mut queue = vec![dependency_id.to_string()];
    while let Some(curr) = queue.pop() {
        if curr == meta.id {
            return Err(WorkflowError::DependencyCycle(format!(
                "Adding dependency '{dependency_id}' would create a dependency cycle"
            )));
        }
        if visited.insert(curr.clone()) {
            if let Some(neighbors) = dep_map.get(&curr) {
                for next in neighbors {
                    queue.push(next.clone());
                }
            }
        }
    }

    meta.depends_on.push(dependency_id.to_string());
    let now = chrono::Utc::now().timestamp();
    let actor_str = actor.unwrap_or("agent:default").to_string();

    meta.activity.push(ActivityEvent {
        at: now,
        actor: actor_str,
        action: "depend".to_string(),
        from: Some(meta.current_status()),
        to: Some(meta.current_status()),
        note: Some(format!("Added dependency on {dependency_id}")),
    });

    save_item(vault_path, &mut meta)?;
    Ok(meta)
}

pub fn parent_item(
    vault_path: &Path,
    filename: &str,
    parent_id: &str,
    actor: Option<&str>,
    expect_revision: Option<u64>,
) -> Result<IdeaMeta, WorkflowError> {
    let (mut meta, _lock) = load_and_lock(vault_path, filename, expect_revision)?;
    meta.parent_id = Some(parent_id.to_string());
    let now = chrono::Utc::now().timestamp();
    let actor_str = actor.unwrap_or("agent:default").to_string();

    meta.activity.push(ActivityEvent {
        at: now,
        actor: actor_str,
        action: "parent".to_string(),
        from: Some(meta.current_status()),
        to: Some(meta.current_status()),
        note: Some(format!("Set parent to {parent_id}")),
    });

    save_item(vault_path, &mut meta)?;
    Ok(meta)
}

pub fn relate_item(
    vault_path: &Path,
    filename: &str,
    related_id: &str,
    actor: Option<&str>,
    expect_revision: Option<u64>,
) -> Result<IdeaMeta, WorkflowError> {
    let (mut meta, _lock) = load_and_lock(vault_path, filename, expect_revision)?;
    if !meta.related.iter().any(|r| r == related_id) {
        meta.related.push(related_id.to_string());
    }
    let now = chrono::Utc::now().timestamp();
    let actor_str = actor.unwrap_or("agent:default").to_string();

    meta.activity.push(ActivityEvent {
        at: now,
        actor: actor_str,
        action: "relate".to_string(),
        from: Some(meta.current_status()),
        to: Some(meta.current_status()),
        note: Some(format!("Related to {related_id}")),
    });

    save_item(vault_path, &mut meta)?;
    Ok(meta)
}

pub fn is_item_ready(
    item: &IdeaMeta,
    all_items_map: &HashMap<String, &IdeaMeta>,
    now: i64,
) -> bool {
    let status_ready = item.current_status() == Status::Planned
        || (item.current_status() == Status::Created
            && (item.work_type() == WorkType::Task || item.work_type() == WorkType::Bug));
    if !status_ready {
        return false;
    }
    if item.has_active_claim(now) {
        return false;
    }
    if item.is_archived() {
        return false;
    }
    for dep_id in &item.depends_on {
        if let Some(dep) = all_items_map.get(dep_id) {
            let s = dep.current_status();
            if s != Status::Done && s != Status::Closed {
                return false;
            }
        } else {
            return false;
        }
    }
    true
}

pub fn next_ready_items(
    all_items: &[IdeaMeta],
    project: Option<&str>,
    limit: usize,
) -> Vec<IdeaMeta> {
    let now = chrono::Utc::now().timestamp();
    let mut map = HashMap::new();
    for item in all_items {
        map.insert(item.id.clone(), item);
    }

    let mut ready: Vec<IdeaMeta> = all_items
        .iter()
        .filter(|item| {
            if let Some(p) = project {
                if !item.project.eq_ignore_ascii_case(p.trim()) {
                    return false;
                }
            }
            is_item_ready(item, &map, now)
        })
        .cloned()
        .collect();

    ready.sort_by(|a, b| {
        b.priority_rank()
            .cmp(&a.priority_rank())
            .then_with(|| a.timestamp.cmp(&b.timestamp))
    });

    if ready.len() > limit {
        ready.truncate(limit);
    }
    ready
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Kind, Priority, WorkType};
    use std::sync::Arc;
    use tempfile::tempdir;

    fn setup_test_item(dir: &Path, id: &str, status: Status, priority: Option<Priority>) -> String {
        let filename = format!("{id}.md");
        let item = IdeaMeta::new_work_item(
            id.to_string(),
            "test".to_string(),
            format!("Item {id}"),
            "Body".to_string(),
            Kind::Technical,
            WorkType::Task,
            status,
            priority,
            None,
            Some("agent:creator".to_string()),
        );
        let content = render_full_document(&item);
        fs::write(dir.join(&filename), content).unwrap();
        filename
    }

    #[test]
    fn test_claim_and_release_lifecycle() {
        let dir = tempdir().unwrap();
        let filename = setup_test_item(dir.path(), "0123456789aa", Status::Planned, None);

        // Claim
        let claimed = claim_item(dir.path(), &filename, "agent:worker1", 60, None).unwrap();
        assert_eq!(claimed.current_status(), Status::InProgress);
        assert_eq!(claimed.claimed_by.as_deref(), Some("agent:worker1"));
        assert!(claimed.claim_expires_at.is_some());
        assert_eq!(claimed.current_revision(), 2);

        // Conflicting claim from another agent should fail
        let conflict = claim_item(dir.path(), &filename, "agent:worker2", 60, None);
        assert!(matches!(conflict, Err(WorkflowError::ClaimConflict { .. })));

        // Release
        let released =
            release_item(dir.path(), &filename, Some("agent:worker1"), false, None).unwrap();
        assert_eq!(released.current_status(), Status::Planned);
        assert!(released.claimed_by.is_none());
        assert_eq!(released.current_revision(), 3);
    }

    #[test]
    fn test_complete_requires_evidence() {
        let dir = tempdir().unwrap();
        let filename = setup_test_item(dir.path(), "0123456789bb", Status::InProgress, None);

        let fail = complete_item(dir.path(), &filename, Some("agent:worker1"), "   ", None);
        assert!(matches!(fail, Err(WorkflowError::MissingEvidence)));

        let done = complete_item(
            dir.path(),
            &filename,
            Some("agent:worker1"),
            "Unit tests passed 5/5",
            None,
        )
        .unwrap();
        assert_eq!(done.current_status(), Status::Done);
        assert!(done.claimed_by.is_none());
        assert_eq!(
            done.handoff.and_then(|h| h.verification),
            Some("Unit tests passed 5/5".to_string())
        );
    }

    #[test]
    fn test_dependency_cycle_rejection() {
        let dir = tempdir().unwrap();
        let f1 = setup_test_item(dir.path(), "012345678901", Status::Planned, None);
        let f2 = setup_test_item(dir.path(), "012345678902", Status::Planned, None);

        // f2 depends on f1
        depend_item(dir.path(), &f2, "012345678901", None, None).unwrap();

        // f1 depending on f2 must fail with DependencyCycle
        let cycle = depend_item(dir.path(), &f1, "012345678902", None, None);
        assert!(matches!(cycle, Err(WorkflowError::DependencyCycle(_))));
    }

    #[test]
    fn test_revision_conflict() {
        let dir = tempdir().unwrap();
        let filename = setup_test_item(dir.path(), "0123456789cc", Status::Planned, None);

        let res = transition_item(
            dir.path(),
            &filename,
            Status::Blocked,
            None,
            Some("waiting on key"),
            Some(99), // wrong revision
        );
        assert!(matches!(res, Err(WorkflowError::RevisionConflict { .. })));
    }

    #[test]
    fn test_lease_expiry_allows_new_claim() {
        let dir = tempdir().unwrap();
        let filename = setup_test_item(dir.path(), "0123456789dd", Status::Planned, None);

        // Claim with 0-second lease (already expired)
        claim_item(dir.path(), &filename, "agent:worker1", 0, None).unwrap();

        // Second agent can now claim because lease expired
        let c2 = claim_item(dir.path(), &filename, "agent:worker2", 60, None).unwrap();
        assert_eq!(c2.claimed_by.as_deref(), Some("agent:worker2"));
    }

    #[test]
    fn test_handoff_accumulation() {
        let dir = tempdir().unwrap();
        let filename = setup_test_item(dir.path(), "0123456789ee", Status::InProgress, None);

        handoff_item(
            dir.path(),
            &filename,
            Some("agent:worker1"),
            Some("Step 1 done"),
            Some("Do step 2"),
            None,
            None,
            None,
        )
        .unwrap();

        let h2 = handoff_item(
            dir.path(),
            &filename,
            Some("agent:worker2"),
            None,
            Some("Do step 3"),
            Some("Missing API key"),
            None,
            None,
        )
        .unwrap();

        let handoff = h2.handoff.unwrap();
        assert_eq!(handoff.progress.as_deref(), Some("Step 1 done"));
        assert_eq!(handoff.next.as_deref(), Some("Do step 3"));
        assert_eq!(handoff.blocker.as_deref(), Some("Missing API key"));
    }

    #[test]
    fn test_next_ready_items_dependency_ordering() {
        let dir = tempdir().unwrap();
        let f1 = setup_test_item(
            dir.path(),
            "012345678901",
            Status::Planned,
            Some(Priority::Medium),
        );
        let f2 = setup_test_item(
            dir.path(),
            "012345678902",
            Status::Planned,
            Some(Priority::High),
        );

        // f2 (High priority) depends on f1 (Medium priority)
        depend_item(dir.path(), &f2, "012345678901", None, None).unwrap();

        let items = collect_ideas(dir.path()).unwrap();
        let ready = next_ready_items(&items, Some("test"), 10);
        // Only f1 is ready because f2 is blocked by f1
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, "012345678901");

        // Complete f1 with evidence
        complete_item(
            dir.path(),
            &f1,
            Some("agent:test"),
            "Verification proof",
            None,
        )
        .unwrap();

        let items_after = collect_ideas(dir.path()).unwrap();
        let ready_after = next_ready_items(&items_after, Some("test"), 10);
        // Now f2 is ready!
        assert_eq!(ready_after.len(), 1);
        assert_eq!(ready_after[0].id, "012345678902");
    }

    #[test]
    fn test_concurrent_lock_contention() {
        let dir = Arc::new(tempdir().unwrap());
        let filename = setup_test_item(dir.path(), "0123456789ff", Status::Planned, None);

        let dir1 = Arc::clone(&dir);
        let f1 = filename.clone();
        let handle1 = thread::spawn(move || {
            for i in 0..5 {
                let _ = handoff_item(
                    dir1.path(),
                    &f1,
                    Some("agent:thread1"),
                    Some(&format!("Progress from thread 1 iteration {i}")),
                    None,
                    None,
                    None,
                    None,
                );
                thread::sleep(Duration::from_millis(10));
            }
        });

        let dir2 = Arc::clone(&dir);
        let f2 = filename.clone();
        let handle2 = thread::spawn(move || {
            for i in 0..5 {
                let _ = handoff_item(
                    dir2.path(),
                    &f2,
                    Some("agent:thread2"),
                    None,
                    Some(&format!("Next from thread 2 iteration {i}")),
                    None,
                    None,
                    None,
                );
                thread::sleep(Duration::from_millis(10));
            }
        });

        handle1.join().unwrap();
        handle2.join().unwrap();

        let (final_meta, _lock) = load_and_lock(dir.path(), &filename, None).unwrap();
        // Revision should have incremented 10 times (from 1 to 11)
        assert_eq!(final_meta.current_revision(), 11);
        assert_eq!(final_meta.activity.len(), 11); // initial created + 10 handoffs
    }

    #[test]
    fn test_is_item_ready_created_tasks_and_bugs() {
        let dir = tempdir().unwrap();
        let _task = setup_test_item(dir.path(), "task01", Status::Created, None);

        let bug_item = IdeaMeta::new_work_item(
            "bug01".to_string(),
            "test".to_string(),
            "Bug item".to_string(),
            "Body".to_string(),
            Kind::Technical,
            WorkType::Bug,
            Status::Created,
            None,
            None,
            Some("agent:creator".to_string()),
        );
        fs::write(dir.path().join("bug01.md"), render_full_document(&bug_item)).unwrap();

        let idea_item = IdeaMeta::new_work_item(
            "idea01".to_string(),
            "test".to_string(),
            "Idea item".to_string(),
            "Body".to_string(),
            Kind::Technical,
            WorkType::Idea,
            Status::Created,
            None,
            None,
            Some("agent:creator".to_string()),
        );
        fs::write(
            dir.path().join("idea01.md"),
            render_full_document(&idea_item),
        )
        .unwrap();

        let items = collect_ideas(dir.path()).unwrap();
        let ready = next_ready_items(&items, Some("test"), 10);
        let ready_ids: Vec<&str> = ready.iter().map(|i| i.id.as_str()).collect();
        assert!(ready_ids.contains(&"task01"));
        assert!(ready_ids.contains(&"bug01"));
        assert!(!ready_ids.contains(&"idea01"));
    }

    #[test]
    fn test_archive_and_unarchive_lifecycle() {
        let dir = tempdir().unwrap();
        let filename = setup_test_item(dir.path(), "arch01", Status::InProgress, None);

        let archived = archive_item(
            dir.path(),
            &filename,
            Resolution::Implemented,
            Some("agent:test"),
            Some("Shipped"),
            None,
        )
        .unwrap();

        assert!(archived.is_archived());
        assert_eq!(archived.resolution, Some(Resolution::Implemented));
        assert_eq!(archived.resolution_note.as_deref(), Some("Shipped"));
        assert_eq!(archived.claimed_by, None);
        assert_eq!(archived.current_revision(), 2);
        assert_eq!(archived.activity.last().unwrap().action, "archived");

        let unarchived =
            unarchive_item(dir.path(), &filename, Some("agent:test"), Some(2)).unwrap();

        assert!(!unarchived.is_archived());
        assert_eq!(unarchived.resolution, None);
        assert_eq!(unarchived.current_revision(), 3);
        assert_eq!(unarchived.activity.last().unwrap().action, "unarchived");
    }
}
