use crate::frontmatter::{parse_front_matter_detailed, Severity};
use crate::model::{ActivityEvent, ArchiveFilter, Handoff, IdeaMeta, Resolution, Status, WorkType};
use crate::vault::{
    atomic_write, collect_work_items_filtered, lock_item, resolve_selector, VaultError,
    WorkItemFilter,
};
use chrono::Utc;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::path::Path;

pub const DEFAULT_CLAIM_SECONDS: i64 = 60 * 60;

#[derive(Debug)]
pub enum WorkflowError {
    Vault(VaultError),
    Io(std::io::Error),
    InvalidFrontMatter(String),
    InvalidTransition { from: Status, to: Status },
    AlreadyClaimed(String),
    NotClaimed,
    WrongClaimant(String),
    MissingEvidence,
    RevisionConflict { expected: u64, actual: u64 },
    InvalidDependency(String),
    InvalidRelation(String),
    InvalidAction(String),
}

impl fmt::Display for WorkflowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WorkflowError::Vault(error) => write!(f, "{error}"),
            WorkflowError::Io(error) => write!(f, "IO error: {error}"),
            WorkflowError::InvalidFrontMatter(filename) => {
                write!(f, "Invalid front matter in '{filename}'")
            }
            WorkflowError::InvalidTransition { from, to } => {
                write!(f, "Cannot transition from '{from}' to '{to}'")
            }
            WorkflowError::AlreadyClaimed(actor) => {
                write!(f, "Item is claimed by '{actor}'")
            }
            WorkflowError::NotClaimed => write!(f, "Item is not claimed"),
            WorkflowError::WrongClaimant(actor) => {
                write!(f, "Item is claimed by '{actor}', not the requested actor")
            }
            WorkflowError::MissingEvidence => {
                write!(f, "Completion requires non-empty evidence")
            }
            WorkflowError::RevisionConflict { expected, actual } => {
                write!(f, "Revision conflict: expected {expected}, found {actual}")
            }
            WorkflowError::InvalidDependency(id) => {
                write!(f, "Invalid dependency ID '{id}'")
            }
            WorkflowError::InvalidRelation(id) => write!(f, "Invalid related item ID '{id}'"),
            WorkflowError::InvalidAction(action) => write!(f, "Invalid action '{action}'"),
        }
    }
}

impl std::error::Error for WorkflowError {}

impl From<VaultError> for WorkflowError {
    fn from(error: VaultError) -> Self {
        WorkflowError::Vault(error)
    }
}

impl From<std::io::Error> for WorkflowError {
    fn from(error: std::io::Error) -> Self {
        WorkflowError::Io(error)
    }
}

pub fn can_transition(from: Status, to: Status) -> bool {
    if from == to {
        return true;
    }

    matches!(
        (from, to),
        (Status::Captured, Status::Planned | Status::Cancelled)
            | (
                Status::Planned,
                Status::Captured | Status::InProgress | Status::Cancelled
            )
            | (
                Status::InProgress,
                Status::Planned
                    | Status::Blocked
                    | Status::Review
                    | Status::Done
                    | Status::Cancelled
            )
            | (
                Status::Blocked,
                Status::Planned | Status::InProgress | Status::Cancelled
            )
            | (Status::Review, Status::InProgress | Status::Done)
            | (Status::Done, Status::InProgress | Status::Closed)
            | (Status::Closed, Status::Planned)
            | (Status::Cancelled, Status::Captured)
    )
}

pub fn actor_from_env() -> String {
    std::env::var("PIN_ACTOR")
        .ok()
        .filter(|actor| !actor.trim().is_empty())
        .unwrap_or_else(|| "human:cli".to_string())
}

pub fn mutate_item<F>(
    vault_path: &Path,
    selector: &str,
    expected_revision: Option<u64>,
    operation: F,
) -> Result<IdeaMeta, WorkflowError>
where
    F: FnOnce(&mut IdeaMeta) -> Result<(), WorkflowError>,
{
    let filename = resolve_selector(vault_path, selector)?;
    let path = vault_path.join(&filename);
    let _lock = lock_item(&path)?;
    let content = fs::read_to_string(&path)?;
    let mut issues = Vec::new();
    let mut meta = parse_front_matter_detailed(&filename, &content, &mut issues)
        .ok_or_else(|| WorkflowError::InvalidFrontMatter(filename.clone()))?;
    if issues.iter().any(|issue| issue.severity == Severity::Error) {
        return Err(WorkflowError::InvalidFrontMatter(filename));
    }

    let actual_revision = meta.current_revision();
    if let Some(expected) = expected_revision {
        if expected != actual_revision {
            return Err(WorkflowError::RevisionConflict {
                expected,
                actual: actual_revision,
            });
        }
    }

    operation(&mut meta)?;
    prepare_v2(&mut meta);
    meta.revision = Some(actual_revision.saturating_add(1));
    meta.updated_at = Some(Utc::now().timestamp());

    atomic_write(&path, &crate::frontmatter::render_full_document(&meta))?;
    Ok(meta)
}

pub fn transition(
    meta: &mut IdeaMeta,
    target: Status,
    actor: &str,
    note: Option<String>,
) -> Result<(), WorkflowError> {
    let from = meta.current_status();
    if target == Status::Done && from != Status::Done {
        return Err(WorkflowError::MissingEvidence);
    }
    if !can_transition(from, target) {
        return Err(WorkflowError::InvalidTransition { from, to: target });
    }

    if from != target {
        meta.status = Some(target);
        if target == Status::Blocked {
            if let Some(blocker) = note.as_deref() {
                let mut handoff = meta.handoff.clone().unwrap_or(Handoff {
                    progress: None,
                    next: None,
                    blocker: None,
                    verification: None,
                });
                handoff.blocker = Some(blocker.to_string());
                meta.handoff = Some(handoff);
            }
        }
        append_activity(
            meta,
            actor,
            "status_changed",
            Some(from),
            Some(target),
            note,
        );
    }

    if target == Status::Blocked || target == Status::Done || target.is_terminal() {
        meta.claimed_by = None;
        meta.claim_expires_at = None;
    }
    Ok(())
}

pub fn claim(meta: &mut IdeaMeta, actor: &str, lease_seconds: i64) -> Result<(), WorkflowError> {
    let now = Utc::now().timestamp();
    if !matches!(meta.current_status(), Status::Planned | Status::InProgress) {
        return Err(WorkflowError::InvalidTransition {
            from: meta.current_status(),
            to: Status::InProgress,
        });
    }

    if meta.has_active_claim(now) && meta.claimed_by.as_deref() != Some(actor) {
        return Err(WorkflowError::AlreadyClaimed(
            meta.claimed_by.clone().unwrap_or_default(),
        ));
    }

    let expires_at = now.saturating_add(lease_seconds.max(1));
    let action = if meta.claimed_by.as_deref() == Some(actor) {
        "claim_renewed"
    } else {
        "claimed"
    };
    meta.claimed_by = Some(actor.to_string());
    meta.claim_expires_at = Some(expires_at);
    if meta.current_status() == Status::Planned {
        meta.status = Some(Status::InProgress);
    }
    append_activity(meta, actor, action, None, Some(Status::InProgress), None);
    Ok(())
}

pub fn release(meta: &mut IdeaMeta, actor: &str, force: bool) -> Result<(), WorkflowError> {
    let Some(claimed_by) = meta.claimed_by.clone() else {
        return Err(WorkflowError::NotClaimed);
    };
    if !force && claimed_by != actor && meta.has_active_claim(Utc::now().timestamp()) {
        return Err(WorkflowError::WrongClaimant(claimed_by));
    }
    meta.claimed_by = None;
    meta.claim_expires_at = None;
    if meta.current_status() == Status::InProgress {
        meta.status = Some(Status::Planned);
        append_activity(
            meta,
            actor,
            "status_changed",
            Some(Status::InProgress),
            Some(Status::Planned),
            Some("Claim released".to_string()),
        );
    }
    append_activity(meta, actor, "released", None, None, None);
    Ok(())
}

pub fn set_handoff(
    meta: &mut IdeaMeta,
    actor: &str,
    handoff: Handoff,
) -> Result<(), WorkflowError> {
    if handoff.progress.is_none()
        && handoff.next.is_none()
        && handoff.blocker.is_none()
        && handoff.verification.is_none()
    {
        return Err(WorkflowError::InvalidFrontMatter(
            "handoff must contain at least one field".to_string(),
        ));
    }
    meta.handoff = Some(handoff);
    append_activity(meta, actor, "handoff_updated", None, None, None);
    Ok(())
}

pub fn add_dependency(
    meta: &mut IdeaMeta,
    dependency: &str,
    actor: &str,
) -> Result<(), WorkflowError> {
    if dependency == meta.id || dependency.trim().is_empty() {
        return Err(WorkflowError::InvalidDependency(dependency.to_string()));
    }
    if !meta.depends_on.iter().any(|id| id == dependency) {
        meta.depends_on.push(dependency.to_string());
        append_activity(
            meta,
            actor,
            "dependency_added",
            None,
            None,
            Some(dependency.to_string()),
        );
    }
    Ok(())
}

pub fn dependency_would_cycle(item_id: &str, dependency_id: &str, items: &[IdeaMeta]) -> bool {
    let dependencies: HashMap<&str, &[String]> = items
        .iter()
        .map(|item| (item.id.as_str(), item.depends_on.as_slice()))
        .collect();
    let mut pending = vec![dependency_id];
    let mut visited = HashSet::new();

    while let Some(id) = pending.pop() {
        if id == item_id {
            return true;
        }
        if !visited.insert(id) {
            continue;
        }
        if let Some(next) = dependencies.get(id) {
            pending.extend(next.iter().map(String::as_str));
        }
    }
    false
}

pub fn parent_would_cycle(item_id: &str, parent_id: &str, items: &[IdeaMeta]) -> bool {
    let parents: HashMap<&str, &str> = items
        .iter()
        .filter_map(|item| {
            item.parent_id
                .as_deref()
                .map(|parent| (item.id.as_str(), parent))
        })
        .collect();
    let mut current = Some(parent_id);
    let mut visited = HashSet::new();

    while let Some(id) = current {
        if id == item_id {
            return true;
        }
        if !visited.insert(id) {
            break;
        }
        current = parents.get(id).copied();
    }
    false
}

pub fn set_parent(meta: &mut IdeaMeta, parent_id: &str, actor: &str) -> Result<(), WorkflowError> {
    if parent_id == meta.id || parent_id.trim().is_empty() {
        return Err(WorkflowError::InvalidRelation(parent_id.to_string()));
    }
    if meta.parent_id.as_deref() != Some(parent_id) {
        meta.parent_id = Some(parent_id.to_string());
        append_activity(
            meta,
            actor,
            "parent_set",
            None,
            None,
            Some(parent_id.to_string()),
        );
    }
    Ok(())
}

pub fn add_related(
    meta: &mut IdeaMeta,
    related_id: &str,
    actor: &str,
) -> Result<(), WorkflowError> {
    if related_id == meta.id || related_id.trim().is_empty() {
        return Err(WorkflowError::InvalidRelation(related_id.to_string()));
    }
    if !meta.related.iter().any(|id| id == related_id) {
        meta.related.push(related_id.to_string());
        append_activity(
            meta,
            actor,
            "related_item_added",
            None,
            None,
            Some(related_id.to_string()),
        );
    }
    Ok(())
}

pub fn append_activity(
    meta: &mut IdeaMeta,
    actor: &str,
    action: &str,
    from: Option<Status>,
    to: Option<Status>,
    note: Option<String>,
) {
    meta.activity.push(ActivityEvent {
        at: Utc::now().timestamp(),
        actor: actor.to_string(),
        action: action.to_string(),
        from,
        to,
        note,
    });
}

pub fn prepare_v2(meta: &mut IdeaMeta) {
    meta.schema = Some(2);
    if meta.item_type.is_none() {
        meta.item_type = Some(WorkType::Idea);
    }
    if meta.status.is_none() {
        meta.status = Some(meta.current_status());
    }
    if meta.updated_at.is_none() {
        meta.updated_at = Some(meta.timestamp);
    }
    if meta.revision.is_none() {
        meta.revision = Some(0);
    }
}

pub fn complete(meta: &mut IdeaMeta, actor: &str, evidence: String) -> Result<(), WorkflowError> {
    if evidence.trim().is_empty() {
        return Err(WorkflowError::MissingEvidence);
    }
    let mut handoff = meta.handoff.clone().unwrap_or(Handoff {
        progress: None,
        next: None,
        blocker: None,
        verification: None,
    });
    handoff.verification = Some(evidence.clone());
    meta.handoff = Some(handoff);
    let from = meta.current_status();
    if from == Status::Done {
        append_activity(
            meta,
            actor,
            "completion_verified",
            Some(from),
            Some(from),
            Some(evidence),
        );
        return Ok(());
    }
    if !matches!(from, Status::InProgress | Status::Review) {
        return Err(WorkflowError::InvalidTransition {
            from,
            to: Status::Done,
        });
    }
    meta.status = Some(Status::Done);
    meta.claimed_by = None;
    meta.claim_expires_at = None;
    append_activity(
        meta,
        actor,
        "completed",
        Some(from),
        Some(Status::Done),
        Some(evidence),
    );
    Ok(())
}
#[derive(Debug, Clone)]
pub enum MutationAction {
    Transition {
        target: Status,
        actor: String,
        note: Option<String>,
    },
    Claim {
        actor: String,
        lease: i64,
    },
    Release {
        actor: String,
        force: bool,
    },
    Handoff {
        actor: String,
        handoff: Handoff,
    },
    Complete {
        actor: String,
        evidence: String,
    },
    Close {
        actor: String,
        note: Option<String>,
    },
    Archive {
        actor: String,
        resolution: Resolution,
        note: Option<String>,
    },
    Unarchive {
        actor: String,
    },
    Depend {
        actor: String,
        dependency_selector: String,
    },
    Parent {
        actor: String,
        parent_selector: String,
    },
    Relate {
        actor: String,
        related_selector: String,
    },
}

fn load_all_items(vault_path: &Path) -> Result<Vec<IdeaMeta>, WorkflowError> {
    collect_work_items_filtered(
        vault_path,
        &WorkItemFilter {
            archive_filter: ArchiveFilter::All,
            ..WorkItemFilter::default()
        },
    )
    .map_err(WorkflowError::Io)
}

fn resolve_link_id(vault_path: &Path, selector: &str) -> Option<String> {
    let filename = resolve_selector(vault_path, selector).ok()?;
    let path = vault_path.join(&filename);
    let content = fs::read_to_string(path).ok()?;
    let mut issues = Vec::new();
    let meta = parse_front_matter_detailed(&filename, &content, &mut issues)?;
    Some(meta.id)
}

pub fn execute_mutation(
    vault_path: &Path,
    selector: &str,
    expected_revision: Option<u64>,
    action: MutationAction,
) -> Result<IdeaMeta, WorkflowError> {
    match action {
        MutationAction::Transition {
            target,
            actor,
            note,
        } => mutate_item(vault_path, selector, expected_revision, |meta| {
            transition(meta, target, &actor, note)
        }),
        MutationAction::Claim { actor, lease } => {
            mutate_item(vault_path, selector, expected_revision, |meta| {
                claim(meta, &actor, lease)
            })
        }
        MutationAction::Release { actor, force } => {
            mutate_item(vault_path, selector, expected_revision, |meta| {
                release(meta, &actor, force)
            })
        }
        MutationAction::Handoff { actor, handoff } => {
            mutate_item(vault_path, selector, expected_revision, |meta| {
                set_handoff(meta, &actor, handoff)
            })
        }
        MutationAction::Complete { actor, evidence } => {
            mutate_item(vault_path, selector, expected_revision, |meta| {
                complete(meta, &actor, evidence)
            })
        }
        MutationAction::Close { actor, note } => {
            mutate_item(vault_path, selector, expected_revision, |meta| {
                transition(meta, Status::Closed, &actor, note)
            })
        }
        MutationAction::Archive {
            actor,
            resolution,
            note,
        } => mutate_item(vault_path, selector, expected_revision, |meta| {
            let previous = meta.current_status();
            meta.archived_at = Some(Utc::now().timestamp());
            meta.resolution = Some(resolution);
            meta.resolution_note = note.clone();
            meta.claimed_by = None;
            meta.claim_expires_at = None;
            append_activity(meta, &actor, "archived", Some(previous), None, note);
            Ok(())
        }),
        MutationAction::Unarchive { actor } => {
            mutate_item(vault_path, selector, expected_revision, |meta| {
                meta.archived_at = None;
                meta.resolution = None;
                meta.resolution_note = None;
                append_activity(meta, &actor, "unarchived", None, None, None);
                Ok(())
            })
        }
        MutationAction::Depend {
            actor,
            dependency_selector,
        } => {
            let dep_id = resolve_link_id(vault_path, &dependency_selector)
                .ok_or(WorkflowError::InvalidDependency(dependency_selector))?;
            mutate_item(vault_path, selector, expected_revision, |meta| {
                let all = load_all_items(vault_path)?;
                if dependency_would_cycle(&meta.id, &dep_id, &all) {
                    return Err(WorkflowError::InvalidDependency(dep_id));
                }
                add_dependency(meta, &dep_id, &actor)
            })
        }
        MutationAction::Parent {
            actor,
            parent_selector,
        } => {
            let parent_id = resolve_link_id(vault_path, &parent_selector)
                .ok_or(WorkflowError::InvalidRelation(parent_selector))?;
            mutate_item(vault_path, selector, expected_revision, |meta| {
                let all = load_all_items(vault_path)?;
                if parent_would_cycle(&meta.id, &parent_id, &all) {
                    return Err(WorkflowError::InvalidRelation(parent_id));
                }
                set_parent(meta, &parent_id, &actor)
            })
        }
        MutationAction::Relate {
            actor,
            related_selector,
        } => {
            let rel_id = resolve_link_id(vault_path, &related_selector)
                .ok_or(WorkflowError::InvalidRelation(related_selector))?;
            mutate_item(vault_path, selector, expected_revision, |meta| {
                add_related(meta, &rel_id, &actor)
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Kind, Priority};

    fn meta(status: Status) -> IdeaMeta {
        IdeaMeta {
            schema: Some(2),
            id: "0123456789ab".to_string(),
            project: "test".to_string(),
            kind: Kind::Technical,
            item_type: Some(WorkType::Task),
            status: Some(status),
            timestamp: 1,
            created_at_ns: None,
            title: "Test".to_string(),
            tags: None,
            priority: Some(Priority::Medium),
            updated_at: Some(1),
            revision: Some(0),
            created_by: None,
            claimed_by: None,
            claim_expires_at: None,
            parent_id: None,
            depends_on: Vec::new(),
            related: Vec::new(),
            handoff: None,
            activity: Vec::new(),
            archived_at: None,
            resolution: None,
            resolution_note: None,
            filename: "0123456789ab.md".to_string(),
            body: String::new(),
            score: None,
            raw_frontmatter_map: serde_yaml::Mapping::new(),
        }
    }

    #[test]
    fn transition_rules_allow_the_workflow_and_reopen_terminal_items() {
        assert!(can_transition(Status::Captured, Status::Planned));
        assert!(can_transition(Status::Planned, Status::InProgress));
        assert!(can_transition(Status::InProgress, Status::Blocked));
        assert!(can_transition(Status::Review, Status::Done));
        assert!(can_transition(Status::Done, Status::Closed));
        assert!(can_transition(Status::Closed, Status::Planned));
        assert!(!can_transition(Status::Captured, Status::Done));
        let mut item = meta(Status::Review);
        assert!(matches!(
            transition(&mut item, Status::Done, "agent:test", None),
            Err(WorkflowError::MissingEvidence)
        ));
    }

    #[test]
    fn claim_moves_planned_work_into_progress() {
        let mut item = meta(Status::Planned);
        claim(&mut item, "agent:test", 60).unwrap();
        assert_eq!(item.current_status(), Status::InProgress);
        assert_eq!(item.claimed_by.as_deref(), Some("agent:test"));
        assert_eq!(item.activity.len(), 1);
    }

    #[test]
    fn completion_requires_evidence_and_clears_claim() {
        let mut item = meta(Status::Review);
        item.claimed_by = Some("agent:test".to_string());
        item.claim_expires_at = Some(i64::MAX);
        assert!(matches!(
            complete(&mut item, "agent:test", " ".to_string()),
            Err(WorkflowError::MissingEvidence)
        ));
        complete(&mut item, "agent:test", "Tests pass".to_string()).unwrap();
        assert_eq!(item.current_status(), Status::Done);
        assert!(item.claimed_by.is_none());
        assert_eq!(
            item.handoff.unwrap().verification.as_deref(),
            Some("Tests pass")
        );
    }

    #[test]
    fn dependency_cycle_detection_follows_existing_links() {
        let mut first = meta(Status::Planned);
        let mut second = meta(Status::Planned);
        first.id = "first".to_string();
        second.id = "second".to_string();
        second.depends_on.push("first".to_string());
        assert!(dependency_would_cycle("first", "second", &[first, second]));
    }

    #[test]
    fn parent_and_related_links_are_idempotent() {
        let mut item = meta(Status::Planned);
        set_parent(&mut item, "parent", "agent:test").unwrap();
        add_related(&mut item, "related", "agent:test").unwrap();
        add_related(&mut item, "related", "agent:test").unwrap();
        assert_eq!(item.parent_id.as_deref(), Some("parent"));
        assert_eq!(item.related, vec!["related"]);
        assert_eq!(item.activity.len(), 2);
    }

    #[test]
    fn parent_cycle_detection_follows_existing_links() {
        let mut first = meta(Status::Planned);
        let mut second = meta(Status::Planned);
        first.id = "first".to_string();
        second.id = "second".to_string();
        second.parent_id = Some("first".to_string());
        assert!(parent_would_cycle("first", "second", &[first, second]));
    }

    #[test]
    fn expired_claim_can_be_released_by_a_new_actor() {
        let mut item = meta(Status::InProgress);
        item.claimed_by = Some("agent:dead".to_string());
        item.claim_expires_at = Some(1);
        release(&mut item, "agent:new", false).unwrap();
        assert_eq!(item.current_status(), Status::Planned);
        assert!(item.claimed_by.is_none());
    }

    #[test]
    fn concurrent_mutations_serialize_without_lost_updates() {
        let temp_dir = tempfile::tempdir().unwrap();
        let vault_path = temp_dir.path().to_path_buf();
        let mut item = meta(Status::Planned);
        item.id = "concurrent01".to_string();
        item.filename = "concurrent01.md".to_string();
        let doc = crate::frontmatter::render_full_document(&item);
        atomic_write(&vault_path.join(&item.filename), &doc).unwrap();

        let vault_a = vault_path.clone();
        let vault_b = vault_path.clone();

        let t1 = std::thread::spawn(move || {
            mutate_item(&vault_a, "concurrent01", None, |meta| {
                set_handoff(
                    meta,
                    "agent:worker1",
                    Handoff {
                        progress: Some("Progress 1".to_string()),
                        next: None,
                        blocker: None,
                        verification: None,
                    },
                )
            })
            .unwrap();
        });

        let t2 = std::thread::spawn(move || {
            mutate_item(&vault_b, "concurrent01", None, |meta| {
                add_related(meta, "rel123456789", "agent:worker2")
            })
            .unwrap();
        });

        t1.join().unwrap();
        t2.join().unwrap();

        let updated = mutate_item(&vault_path, "concurrent01", None, |_| Ok(())).unwrap();
        assert_eq!(updated.current_revision(), 3);
        assert_eq!(
            updated.handoff.as_ref().and_then(|h| h.progress.as_deref()),
            Some("Progress 1")
        );
        assert_eq!(updated.related, vec!["rel123456789"]);
        assert!(updated
            .activity
            .iter()
            .any(|a| a.action == "handoff_updated"));
        assert!(updated
            .activity
            .iter()
            .any(|a| a.action == "related_item_added"));
    }

    #[test]
    fn mutation_waits_for_held_lock_without_conflict() {
        let temp_dir = tempfile::tempdir().unwrap();
        let vault_path = temp_dir.path().to_path_buf();
        let mut item = meta(Status::Planned);
        item.id = "locktest0123".to_string();
        item.filename = "locktest0123.md".to_string();
        let doc = crate::frontmatter::render_full_document(&item);
        let file_path = vault_path.join(&item.filename);
        atomic_write(&file_path, &doc).unwrap();

        let lock_file_path = file_path.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let t1 = std::thread::spawn(move || {
            let _lock = lock_item(&lock_file_path).unwrap();
            tx.send(()).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(50));
        });

        rx.recv().unwrap();
        let vault_clone = vault_path.clone();
        let t2 = std::thread::spawn(move || {
            mutate_item(&vault_clone, "locktest0123", None, |meta| {
                transition(meta, Status::InProgress, "agent:test", None)
            })
            .unwrap()
        });

        t1.join().unwrap();
        let result = t2.join().unwrap();
        assert_eq!(result.current_status(), Status::InProgress);
        assert_eq!(result.current_revision(), 1);
    }

    #[test]
    fn expected_revision_conflict_on_stale_update() {
        let temp_dir = tempfile::tempdir().unwrap();
        let vault_path = temp_dir.path().to_path_buf();
        let mut item = meta(Status::Planned);
        item.id = "revtest01234".to_string();
        item.filename = "revtest01234.md".to_string();
        let doc = crate::frontmatter::render_full_document(&item);
        atomic_write(&vault_path.join(&item.filename), &doc).unwrap();

        mutate_item(&vault_path, "revtest01234", Some(0), |meta| {
            claim(meta, "agent:test", 60)
        })
        .unwrap();

        let conflict_err = mutate_item(&vault_path, "revtest01234", Some(0), |meta| {
            release(meta, "agent:test", false)
        })
        .unwrap_err();

        assert!(matches!(
            conflict_err,
            WorkflowError::RevisionConflict {
                expected: 0,
                actual: 1
            }
        ));
    }
}
