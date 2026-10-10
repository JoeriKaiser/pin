use crate::acp::client::{AcpClient, AcpError};
use crate::acp::worktree::{WorktreeError, WorktreeManager};
use crate::workflow::WorkflowError;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Debug)]
pub enum AcpManagerError {
    AlreadyRunning(String),
    PrimaryBusy(String),
    ItemNotFound(String),
    Workflow(String),
    Client(String),
    Worktree(String),
    Io(String),
}

impl std::fmt::Display for AcpManagerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AcpManagerError::AlreadyRunning(id) => write!(f, "Task '{id}' is already running"),
            AcpManagerError::PrimaryBusy(id) => write!(
                f,
                "Primary repository checkout is currently busy running task '{id}'"
            ),
            AcpManagerError::ItemNotFound(id) => write!(f, "Item '{id}' not found"),
            AcpManagerError::Workflow(msg) => write!(f, "Workflow error: {msg}"),
            AcpManagerError::Client(msg) => write!(f, "ACP client error: {msg}"),
            AcpManagerError::Worktree(msg) => write!(f, "Worktree error: {msg}"),
            AcpManagerError::Io(msg) => write!(f, "IO error: {msg}"),
        }
    }
}

impl std::error::Error for AcpManagerError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMode {
    Implement { use_worktree: bool },
    CommitPr,
}

impl From<std::io::Error> for AcpManagerError {
    fn from(e: std::io::Error) -> Self {
        AcpManagerError::Io(e.to_string())
    }
}

impl From<WorkflowError> for AcpManagerError {
    fn from(e: WorkflowError) -> Self {
        AcpManagerError::Workflow(e.to_string())
    }
}

impl From<WorktreeError> for AcpManagerError {
    fn from(e: WorktreeError) -> Self {
        AcpManagerError::Worktree(e.to_string())
    }
}

impl From<AcpError> for AcpManagerError {
    fn from(e: AcpError) -> Self {
        AcpManagerError::Client(e.to_string())
    }
}

#[derive(Clone)]
pub struct ActiveRun {
    pub item_id: String,
    pub client: Arc<AcpClient>,
    pub worktree_path: Option<PathBuf>,
    pub is_primary_worktree: bool,
    pub event_buffer: Arc<Mutex<Vec<serde_json::Value>>>,
    pub subscribers: Arc<Mutex<Vec<Sender<serde_json::Value>>>>,
    pub started_at: i64,
    pub cancel_requested: Arc<AtomicBool>,
}
#[derive(Clone)]
pub struct CompletedRun {
    pub item_id: String,
    pub events: Vec<serde_json::Value>,
    pub started_at: i64,
    pub finished_at: i64,
    pub stop_reason: Option<String>,
}

#[derive(Clone)]
pub struct AcpManager {
    vault_path: PathBuf,
    repo_path: PathBuf,
    acp_command: String,
    active_runs: Arc<Mutex<HashMap<String, ActiveRun>>>,
    completed_runs: Arc<Mutex<Vec<CompletedRun>>>,
}

impl AcpManager {
    pub fn new(vault_path: PathBuf, repo_path: PathBuf, acp_command: Option<String>) -> Self {
        let acp_command = acp_command
            .filter(|cmd| !cmd.trim().is_empty())
            .unwrap_or_else(|| {
                std::env::var("PIN_ACP_COMMAND").unwrap_or_else(|_| "omp acp".to_string())
            });

        Self {
            vault_path,
            repo_path,
            acp_command,
            active_runs: Arc::new(Mutex::new(HashMap::new())),
            completed_runs: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn vault_path(&self) -> &Path {
        &self.vault_path
    }

    pub fn repo_path(&self) -> &Path {
        &self.repo_path
    }

    pub fn acp_command(&self) -> &str {
        &self.acp_command
    }

    pub fn is_primary_busy(&self) -> Option<String> {
        let runs = self.active_runs.lock().ok()?;
        runs.values()
            .find(|run| run.is_primary_worktree)
            .map(|run| run.item_id.clone())
    }

    pub fn is_item_running(&self, item_id: &str) -> bool {
        self.active_runs
            .lock()
            .map(|runs| resolve_run_key(&runs, item_id).is_some())
            .unwrap_or(false)
    }

    pub fn running_items(&self) -> Vec<String> {
        if let Ok(runs) = self.active_runs.lock() {
            let mut items: Vec<String> = runs.keys().cloned().collect();
            items.sort();
            items
        } else {
            Vec::new()
        }
    }
    pub fn running_items_with_timing(
        &self,
    ) -> (Vec<String>, std::collections::HashMap<String, i64>) {
        if let Ok(runs) = self.active_runs.lock() {
            let mut items: Vec<String> = runs.keys().cloned().collect();
            items.sort();
            let mut timings = std::collections::HashMap::new();
            for (k, v) in runs.iter() {
                timings.insert(k.clone(), v.started_at);
            }
            (items, timings)
        } else {
            (Vec::new(), std::collections::HashMap::new())
        }
    }

    pub fn get_run_timing(&self, item_id: &str) -> Option<(i64, Option<i64>)> {
        if let Ok(runs) = self.active_runs.lock() {
            if let Some(key) = resolve_run_key(&runs, item_id) {
                if let Some(run) = runs.get(&key) {
                    return Some((run.started_at, None));
                }
            }
        }
        if let Ok(comp) = self.completed_runs.lock() {
            for run in comp.iter().rev() {
                if run.item_id == item_id
                    || run.item_id.starts_with(item_id)
                    || item_id.starts_with(&run.item_id)
                {
                    return Some((run.started_at, Some(run.finished_at)));
                }
            }
        }
        let canonical_id = match crate::vault::resolve_selector(&self.vault_path, item_id) {
            Ok(f) => f.strip_suffix(".md").unwrap_or(&f).to_string(),
            Err(_) => item_id.to_string(),
        };
        let disk_path = self
            .vault_path
            .join("runs")
            .join(format!("{canonical_id}.events.jsonl"));
        if disk_path.is_file() {
            if let Ok(file) = std::fs::File::open(&disk_path) {
                use std::io::BufRead;
                let reader = std::io::BufReader::new(file);
                let mut first_started: Option<i64> = None;
                let mut last_finished: Option<i64> = None;
                for line in reader.lines().map_while(Result::ok) {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) {
                        if first_started.is_none() {
                            if let Some(st) = val.get("startedAt").and_then(|v| v.as_i64()) {
                                first_started = Some(st);
                            }
                        }
                        if let Some(ft) = val.get("finishedAt").and_then(|v| v.as_i64()) {
                            last_finished = Some(ft);
                        }
                    }
                }
                if let Some(st) = first_started {
                    return Some((st, last_finished));
                }
            }
        }
        None
    }

    pub fn get_buffered_events(&self, item_id: &str) -> Vec<serde_json::Value> {
        if let Ok(runs) = self.active_runs.lock() {
            if let Some(key) = resolve_run_key(&runs, item_id) {
                if let Some(run) = runs.get(&key) {
                    return run
                        .event_buffer
                        .lock()
                        .map(|buf| buf.clone())
                        .unwrap_or_default();
                }
            }
        }
        if let Ok(comp) = self.completed_runs.lock() {
            for run in comp.iter().rev() {
                if run.item_id == item_id
                    || run.item_id.starts_with(item_id)
                    || item_id.starts_with(&run.item_id)
                {
                    return run.events.clone();
                }
            }
        }
        let canonical_id = match crate::vault::resolve_selector(&self.vault_path, item_id) {
            Ok(f) => f.strip_suffix(".md").unwrap_or(&f).to_string(),
            Err(_) => item_id.to_string(),
        };
        let disk_path = self
            .vault_path
            .join("runs")
            .join(format!("{canonical_id}.events.jsonl"));
        if disk_path.is_file() {
            if let Ok(file) = std::fs::File::open(&disk_path) {
                use std::io::BufRead;
                let reader = std::io::BufReader::new(file);
                let mut events = Vec::new();
                for line in reader.lines().map_while(Result::ok) {
                    if let Ok(val) = serde_json::from_str(&line) {
                        events.push(val);
                    }
                }
                if !events.is_empty() {
                    return events;
                }
            }
        }
        Vec::new()
    }

    pub fn subscribe(&self, item_id: &str) -> Option<Receiver<serde_json::Value>> {
        let runs = self.active_runs.lock().ok()?;
        let key = resolve_run_key(&runs, item_id)?;
        if let Some(run) = runs.get(&key) {
            let (tx, rx) = mpsc::channel();
            if let Ok(mut subs) = run.subscribers.lock() {
                subs.push(tx);
            }
            Some(rx)
        } else {
            None
        }
    }

    pub fn start_run(&self, item_id: &str, use_worktree: bool) -> Result<(), AcpManagerError> {
        self.start_run_mode(item_id, RunMode::Implement { use_worktree })
    }

    pub fn start_commit_pr_run(&self, item_id: &str) -> Result<(), AcpManagerError> {
        self.start_run_mode(item_id, RunMode::CommitPr)
    }

    pub fn start_run_mode(&self, item_id: &str, mode: RunMode) -> Result<(), AcpManagerError> {
        // Resolve filename and canonical ID first
        let filename = crate::vault::resolve_selector(&self.vault_path, item_id)
            .map_err(|e| AcpManagerError::ItemNotFound(format!("{item_id}: {e}")))?;
        let canonical_id = filename
            .strip_suffix(".md")
            .unwrap_or(&filename)
            .to_string();

        // Determine working directory
        let (work_dir, worktree_path, is_primary) = match mode {
            RunMode::Implement { use_worktree } => {
                if use_worktree {
                    let wt = WorktreeManager::provision_worktree(&self.repo_path, item_id)
                        .map_err(|e| AcpManagerError::Worktree(e.to_string()))?;
                    (wt.clone(), Some(wt), false)
                } else {
                    (self.repo_path.clone(), None, true)
                }
            }
            RunMode::CommitPr => {
                let expected_wt = WorktreeManager::worktree_path(&self.repo_path, item_id);
                if expected_wt.exists() {
                    (expected_wt.clone(), Some(expected_wt), false)
                } else if WorktreeManager::is_git_repo(&self.repo_path) {
                    match WorktreeManager::provision_worktree(&self.repo_path, item_id) {
                        Ok(wt) => (wt.clone(), Some(wt), false),
                        Err(_) => (self.repo_path.clone(), None, true),
                    }
                } else {
                    (self.repo_path.clone(), None, true)
                }
            }
        };
        let is_new_worktree = matches!(mode, RunMode::Implement { use_worktree: true });

        // 1. Reject if canonical_id or item_id is already running
        {
            let runs = self
                .active_runs
                .lock()
                .map_err(|_| AcpManagerError::Io("Lock poisoned on active_runs".to_string()))?;
            if resolve_run_key(&runs, &canonical_id).is_some() {
                return Err(AcpManagerError::AlreadyRunning(canonical_id));
            }

            // 2. If primary checkout is used and busy, return PrimaryBusy
            if is_primary {
                if let Some(busy_run) = runs.values().find(|r| r.is_primary_worktree) {
                    return Err(AcpManagerError::PrimaryBusy(busy_run.item_id.clone()));
                }
            }
        }

        // Attribute the item in the vault to this run
        match mode {
            RunMode::Implement { .. } => {
                if let Err(e) =
                    crate::workflow::begin_agent_run(&self.vault_path, &filename, "agent:omp")
                {
                    return Err(AcpManagerError::Workflow(e.to_string()));
                }
            }
            RunMode::CommitPr => {
                if let Err(e) =
                    crate::workflow::begin_commit_pr_run(&self.vault_path, &filename, "agent:omp")
                {
                    return Err(AcpManagerError::Workflow(e.to_string()));
                }
            }
        }

        // 6. Read item content from vault to build prompt
        let file_path = self.vault_path.join(&filename);
        let content = match fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(e) => {
                let _ = crate::workflow::release_item(
                    &self.vault_path,
                    &filename,
                    Some("agent:omp"),
                    true,
                    None,
                );
                if is_new_worktree {
                    let _ = WorktreeManager::remove_worktree(&self.repo_path, item_id);
                }
                return Err(AcpManagerError::Io(format!(
                    "Failed to read item file {}: {e}",
                    file_path.display()
                )));
            }
        };

        let mut issues = Vec::new();
        let meta =
            crate::frontmatter::parse_front_matter_detailed(&filename, &content, &mut issues);

        let title = meta.as_ref().map(|m| m.title.as_str()).unwrap_or(item_id);
        let body = meta.as_ref().map(|m| m.body.as_str()).unwrap_or(&content);
        let kind = meta.as_ref().map(|m| m.kind.as_str()).unwrap_or("task");
        let priority = meta
            .as_ref()
            .and_then(|m| m.priority.as_ref().map(|p| p.as_str()));

        // Extract acceptance criteria from frontmatter or body
        let mut acceptance_criteria = None;
        if let Some(m) = &meta {
            if let Some(val) = m.raw_frontmatter_map.get("acceptance_criteria") {
                match val {
                    serde_yaml::Value::String(s) => acceptance_criteria = Some(s.clone()),
                    serde_yaml::Value::Sequence(seq) => {
                        let items: Vec<String> = seq
                            .iter()
                            .filter_map(|item| match item {
                                serde_yaml::Value::String(s) => Some(format!("- {s}")),
                                _ => None,
                            })
                            .collect();
                        if !items.is_empty() {
                            acceptance_criteria = Some(items.join("\n"));
                        }
                    }
                    _ => {}
                }
            }
        }
        if acceptance_criteria.is_none() {
            acceptance_criteria = extract_acceptance_criteria_from_body(body);
        }

        let prompt_text = match mode {
            RunMode::Implement { .. } => build_session_prompt(
                title,
                &item_id,
                &kind.to_string(),
                priority,
                acceptance_criteria.as_deref(),
                body,
            ),
            RunMode::CommitPr => build_commit_pr_prompt(
                title,
                &item_id,
                &kind.to_string(),
                priority,
                acceptance_criteria.as_deref(),
                body,
            ),
        };

        // 7. Prepare log path: self.vault_path.join("runs").join(format!("{item_id}.log"))
        let log_dir = self.vault_path.join("runs");
        if let Err(e) = fs::create_dir_all(&log_dir) {
            let _ = crate::workflow::release_item(
                &self.vault_path,
                &filename,
                Some("agent:omp"),
                true,
                None,
            );
            if is_new_worktree {
                let _ = WorktreeManager::remove_worktree(&self.repo_path, item_id);
            }
            return Err(AcpManagerError::Io(e.to_string()));
        }
        let log_path = log_dir.join(format!("{item_id}.log"));

        // Spawn ACP client
        let client = match AcpClient::spawn(&self.acp_command, &work_dir, &log_path) {
            Ok(c) => Arc::new(c),
            Err(e) => {
                let _ = crate::workflow::release_item(
                    &self.vault_path,
                    &filename,
                    Some("agent:omp"),
                    true,
                    None,
                );
                if is_new_worktree {
                    let _ = WorktreeManager::remove_worktree(&self.repo_path, item_id);
                }
                return Err(AcpManagerError::Client(e.to_string()));
            }
        };

        let event_buffer = Arc::new(Mutex::new(Vec::new()));
        let subscribers = Arc::new(Mutex::new(Vec::new()));
        let cancel_requested = Arc::new(AtomicBool::new(false));
        let started_at = chrono::Utc::now().timestamp();

        // 8. Insert ActiveRun into active_runs
        let active_run = ActiveRun {
            item_id: canonical_id.clone(),
            client: Arc::clone(&client),
            worktree_path: worktree_path.clone(),
            is_primary_worktree: is_primary,
            event_buffer: Arc::clone(&event_buffer),
            subscribers: Arc::clone(&subscribers),
            started_at,
            cancel_requested: Arc::clone(&cancel_requested),
        };

        {
            if let Ok(mut runs) = self.active_runs.lock() {
                runs.insert(canonical_id.clone(), active_run);
            }
        }

        // 9. Spawn worker thread
        let worker_client = Arc::clone(&client);
        let worker_event_buffer = Arc::clone(&event_buffer);
        let worker_subscribers = Arc::clone(&subscribers);
        let worker_cancel = Arc::clone(&cancel_requested);
        let worker_vault_path = self.vault_path.clone();
        let worker_repo_path = self.repo_path.clone();
        let worker_item_id = canonical_id.clone();
        let worker_filename = filename.clone();
        let worker_active_runs = Arc::clone(&self.active_runs);
        let worker_completed_runs = Arc::clone(&self.completed_runs);
        let worker_work_dir = work_dir;
        let worker_is_new_worktree = is_new_worktree;

        thread::spawn(move || {
            // Forward client update events (client.subscribe()) to event_buffer and subscribers
            let client_event_rx = worker_client.subscribe();
            let fwd_buffer = Arc::clone(&worker_event_buffer);
            let fwd_subs = Arc::clone(&worker_subscribers);
            let disk_events_path = worker_vault_path
                .join("runs")
                .join(format!("{worker_item_id}.events.jsonl"));
            let fwd_disk_path = disk_events_path.clone();
            thread::spawn(move || {
                while let Ok(event) = client_event_rx.recv() {
                    push_event(&fwd_buffer, &fwd_subs, Some(&fwd_disk_path), event);
                }
            });

            let mut session_id_opt: Option<String> = None;

            let run_result: Result<String, String> = (|| {
                // Initialize
                if worker_cancel.load(Ordering::SeqCst) {
                    return Err("Run cancelled before initialize".to_string());
                }
                worker_client
                    .initialize()
                    .map_err(|e| format!("Initialize failed: {e}"))?;

                // New session
                if worker_cancel.load(Ordering::SeqCst) {
                    return Err("Run cancelled before new_session".to_string());
                }
                let session_id = worker_client
                    .new_session(&worker_work_dir)
                    .map_err(|e| format!("New session failed: {e}"))?;
                session_id_opt = Some(session_id.clone());

                // Prompt
                if worker_cancel.load(Ordering::SeqCst) {
                    return Err("Run cancelled before prompt".to_string());
                }
                let prompt_res = worker_client
                    .prompt(&session_id, &prompt_text)
                    .map_err(|e| format!("Prompt failed: {e}"))?;

                if worker_cancel.load(Ordering::SeqCst) {
                    return Err("Run cancelled during prompt".to_string());
                }

                Ok(prompt_res
                    .stop_reason
                    .unwrap_or_else(|| "end_turn".to_string()))
            })();

            match run_result {
                Ok(stop_reason) => {
                    // Give the event forwarding thread a moment to flush queued events
                    thread::sleep(Duration::from_millis(50));

                    // Extract summary and test evidence from events/text
                    let events = worker_event_buffer
                        .lock()
                        .map(|buf| buf.clone())
                        .unwrap_or_default();
                    let (summary, evidence) = extract_summary_and_evidence(&events);

                    // Record the handoff, then move the item to review. A user
                    // who moved the item elsewhere while the run was active
                    // keeps that status.
                    let handoff_meta = crate::workflow::handoff_item(
                        &worker_vault_path,
                        &worker_filename,
                        Some("agent:omp"),
                        Some(&summary),
                        None,
                        None,
                        Some(&evidence),
                        None,
                    )
                    .ok();

                    let still_running = handoff_meta
                        .as_ref()
                        .map(|meta| meta.current_status() == crate::model::Status::InProgress)
                        .unwrap_or(true);
                    if still_running {
                        let _ = crate::workflow::transition_item(
                            &worker_vault_path,
                            &worker_filename,
                            crate::model::Status::Review,
                            Some("agent:omp"),
                            Some("Completed by agent"),
                            None,
                        );
                    }

                    // End the run attribution without touching the status.
                    let _ = crate::workflow::release_item(
                        &worker_vault_path,
                        &worker_filename,
                        Some("agent:omp"),
                        true,
                        None,
                    );

                    // Sends {"type": "finished", "stopReason": stop_reason} to subscribers
                    let finished_at = chrono::Utc::now().timestamp();
                    let finish_event = serde_json::json!({
                        "type": "finished",
                        "stopReason": stop_reason.clone(),
                        "startedAt": started_at,
                        "finishedAt": finished_at
                    });
                    push_event(
                        &worker_event_buffer,
                        &worker_subscribers,
                        Some(&disk_events_path),
                        finish_event,
                    );
                    let final_events = worker_event_buffer
                        .lock()
                        .map(|buf| buf.clone())
                        .unwrap_or_default();
                    if let Ok(mut comp) = worker_completed_runs.lock() {
                        comp.push(CompletedRun {
                            item_id: worker_item_id.clone(),
                            events: final_events,
                            started_at,
                            finished_at,
                            stop_reason: Some(stop_reason),
                        });
                        if comp.len() > 50 {
                            comp.remove(0);
                        }
                    }

                    // Removes item from active_runs
                    if let Ok(mut runs) = worker_active_runs.lock() {
                        runs.remove(&worker_item_id);
                    }
                }
                Err(err_msg) => {
                    // On error or cancellation:
                    if worker_cancel.load(Ordering::SeqCst) {
                        if let Some(session_id) = session_id_opt.as_deref() {
                            let _ = worker_client.cancel(session_id);
                        }
                        let _ = worker_client.kill();
                    }

                    // Calls release_item. It returns an item that is still
                    // in_progress to planned and leaves any status the user set
                    // while the run was active untouched.
                    let _ = crate::workflow::release_item(
                        &worker_vault_path,
                        &worker_filename,
                        Some("agent:omp"),
                        true,
                        None,
                    );

                    if worker_is_new_worktree {
                        let _ =
                            WorktreeManager::remove_worktree(&worker_repo_path, &worker_item_id);
                    }

                    // Sends {"type": "error", "message": err.to_string()} to subscribers
                    let finished_at = chrono::Utc::now().timestamp();
                    let error_event = serde_json::json!({
                        "type": "error",
                        "message": err_msg.clone(),
                        "startedAt": started_at,
                        "finishedAt": finished_at
                    });
                    push_event(
                        &worker_event_buffer,
                        &worker_subscribers,
                        Some(&disk_events_path),
                        error_event,
                    );
                    let final_events = worker_event_buffer
                        .lock()
                        .map(|buf| buf.clone())
                        .unwrap_or_default();
                    if let Ok(mut comp) = worker_completed_runs.lock() {
                        comp.push(CompletedRun {
                            item_id: worker_item_id.clone(),
                            events: final_events,
                            started_at,
                            finished_at,
                            stop_reason: Some(err_msg),
                        });
                        if comp.len() > 50 {
                            comp.remove(0);
                        }
                    }

                    // Removes item from active_runs
                    if let Ok(mut runs) = worker_active_runs.lock() {
                        runs.remove(&worker_item_id);
                    }
                }
            }
        });

        Ok(())
    }

    pub fn cancel_run(&self, item_id: &str) -> Result<(), AcpManagerError> {
        let run = {
            if let Ok(mut runs) = self.active_runs.lock() {
                let key = resolve_run_key(&runs, item_id);
                if let Some(k) = &key {
                    if let Some(r) = runs.get(k) {
                        let cancel_event = serde_json::json!({
                            "type": "error",
                            "message": "Cancelled by user"
                        });
                        let cancel_disk_path = self
                            .vault_path
                            .join("runs")
                            .join(format!("{}.events.jsonl", r.item_id));
                        push_event(
                            &r.event_buffer,
                            &r.subscribers,
                            Some(&cancel_disk_path),
                            cancel_event,
                        );
                    }
                }
                key.and_then(|k| runs.remove(&k))
            } else {
                None
            }
        };
        if let Some(run) = run {
            run.cancel_requested.store(true, Ordering::SeqCst);
            let _ = run.client.kill();

            let final_events = run
                .event_buffer
                .lock()
                .map(|buf| buf.clone())
                .unwrap_or_default();
            if let Ok(mut comp) = self.completed_runs.lock() {
                comp.push(CompletedRun {
                    item_id: run.item_id.clone(),
                    events: final_events,
                    started_at: run.started_at,
                    finished_at: chrono::Utc::now().timestamp(),
                    stop_reason: Some("Cancelled by user".to_string()),
                });
                if comp.len() > 50 {
                    comp.remove(0);
                }
            }

            let canonical_id = &run.item_id;
            if let Ok(filename) = crate::vault::resolve_selector(&self.vault_path, canonical_id) {
                let _ = crate::workflow::release_item(
                    &self.vault_path,
                    &filename,
                    Some("agent:omp"),
                    true,
                    None,
                );
            }

            if let Some(_wt) = run.worktree_path {
                let _ = WorktreeManager::remove_worktree(&self.repo_path, canonical_id);
            }

            Ok(())
        } else {
            Err(AcpManagerError::ItemNotFound(format!(
                "Item '{item_id}' is not currently running"
            )))
        }
    }
}
pub fn build_session_prompt(
    title: &str,
    item_id: &str,
    kind: &str,
    priority: Option<&str>,
    acceptance_criteria: Option<&str>,
    body: &str,
) -> String {
    let mut prompt_parts = Vec::new();
    prompt_parts.push(format!("# Task: {title}"));
    prompt_parts.push(format!("- Item ID: {item_id}"));
    prompt_parts.push(format!("- Kind: {kind}"));
    if let Some(p) = priority {
        prompt_parts.push(format!("- Priority: {p}"));
    }
    if let Some(ac) = acceptance_criteria {
        prompt_parts.push(format!("\n## Acceptance Criteria\n{ac}"));
    }
    if !body.trim().is_empty() {
        prompt_parts.push(format!("\n## Description\n{}", body.trim()));
    }
    prompt_parts.push("\n## Instructions\nImplement this item. Verify your changes by running tests. Summarize what changed and include test verification evidence. Do not archive this item; completion and handoff recording are handled automatically.".to_string());

    prompt_parts.join("\n")
}

pub fn build_commit_pr_prompt(
    title: &str,
    item_id: &str,
    kind: &str,
    priority: Option<&str>,
    acceptance_criteria: Option<&str>,
    body: &str,
) -> String {
    let mut prompt_parts = Vec::new();
    prompt_parts.push(format!("# Task: Commit and Create PR for '{title}'"));
    prompt_parts.push(format!("- Item ID: {item_id}"));
    prompt_parts.push(format!("- Kind: {kind}"));
    if let Some(p) = priority {
        prompt_parts.push(format!("- Priority: {p}"));
    }
    if let Some(ac) = acceptance_criteria {
        prompt_parts.push(format!("\n## Acceptance Criteria\n{ac}"));
    }
    if !body.trim().is_empty() {
        prompt_parts.push(format!("\n## Description\n{}", body.trim()));
    }
    prompt_parts.push("\n## Instructions\n1. Inspect git status and changed files in this worktree/repository.\n2. Stage and commit all relevant changes with a clear, conventional commit message referencing the task title and ID.\n3. If a git remote is configured, push the branch to origin.\n4. Create a pull request targeting the main branch (e.g. via `gh pr create` or git) with a concise title and summary of changes.\n5. If `gh` is unavailable or no remote is configured, report the commit details and status.\nSummarize what was committed and the pull request status.".to_string());

    prompt_parts.join("\n")
}

fn resolve_run_key(runs: &HashMap<String, ActiveRun>, id: &str) -> Option<String> {
    if runs.contains_key(id) {
        return Some(id.to_string());
    }
    for key in runs.keys() {
        if key.starts_with(id) || id.starts_with(key.as_str()) {
            return Some(key.clone());
        }
    }
    None
}
fn push_event(
    event_buffer: &Arc<Mutex<Vec<serde_json::Value>>>,
    subscribers: &Arc<Mutex<Vec<Sender<serde_json::Value>>>>,
    disk_path: Option<&Path>,
    event: serde_json::Value,
) {
    if let Ok(mut buf) = event_buffer.lock() {
        if buf.len() >= 500 {
            buf.remove(0);
        }
        buf.push(event.clone());
    }
    if let Ok(mut subs) = subscribers.lock() {
        subs.retain(|sub| sub.send(event.clone()).is_ok());
    }
    if let Some(path) = disk_path {
        if let Ok(line) = serde_json::to_string(&event) {
            use std::io::Write;
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
            {
                let _ = writeln!(file, "{line}");
            }
        }
    }
}

/// True when a session update carries assistant prose rather than tool
/// activity, a plan, or a file payload.
fn is_assistant_message(update: &serde_json::Value) -> bool {
    let kind = update
        .get("sessionUpdate")
        .or_else(|| update.get("type"))
        .and_then(|value| value.as_str());
    matches!(kind, Some("agent_message_chunk") | Some("agent_message"))
}

/// Collects the text of an assistant message.
///
/// Callers filter updates first, so this only ever sees conversational prose.
/// Tool results, file reads, and shell output never reach it.
fn extract_text_from_value(val: &serde_json::Value, out: &mut Vec<String>) {
    match val {
        serde_json::Value::String(s) => {
            if !s.trim().is_empty() {
                out.push(s.clone());
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                extract_text_from_value(item, out);
            }
        }
        serde_json::Value::Object(map) => {
            for key in &["content", "text", "message"] {
                if let Some(v) = map.get(*key) {
                    extract_text_from_value(v, out);
                }
            }
        }
        _ => {}
    }
}

fn clean_header_prefix(text: &str, prefixes: &[&str]) -> String {
    let mut trimmed = text.trim();
    let lower = trimmed.to_lowercase();
    for prefix in prefixes {
        if lower.starts_with(prefix) {
            trimmed = trimmed[prefix.len()..].trim();
            break;
        }
    }
    trimmed.to_string()
}

const DEFAULT_SUMMARY: &str = "Task completed by agent.";
const DEFAULT_EVIDENCE: &str = "Run completed without explicit verification output.";

fn extract_summary_and_evidence(events: &[serde_json::Value]) -> (String, String) {
    // Only assistant prose counts. Tool output, file bodies, and shell logs are
    // not the agent's summary, and picking them up is how skill frontmatter and
    // repository file contents ended up in handoffs.
    let mut text_chunks = Vec::new();
    for event in events {
        let Some(params) = event.get("params") else {
            continue;
        };
        let update = params.get("update").unwrap_or(params);
        if !is_assistant_message(update) {
            continue;
        }
        extract_text_from_value(update, &mut text_chunks);
    }

    let full_text = text_chunks.join("\n");
    let full_text_trimmed = full_text.trim();

    if full_text_trimmed.is_empty() {
        return (DEFAULT_SUMMARY.to_string(), DEFAULT_EVIDENCE.to_string());
    }

    let lower = full_text.to_lowercase();

    let summary_start = lower
        .find("## summary")
        .or_else(|| lower.find("### summary"))
        .or_else(|| lower.find("summary:"));

    let evidence_start = lower
        .find("## verification")
        .or_else(|| lower.find("### verification"))
        .or_else(|| lower.find("## tests"))
        .or_else(|| lower.find("### tests"))
        .or_else(|| lower.find("## evidence"))
        .or_else(|| lower.find("### evidence"))
        .or_else(|| lower.find("verification evidence:"))
        .or_else(|| lower.find("test evidence:"))
        .or_else(|| lower.find("verification:"))
        .or_else(|| lower.find("evidence:"));

    let (raw_summary, raw_evidence) = match (summary_start, evidence_start) {
        (Some(s_idx), Some(e_idx)) => {
            if s_idx < e_idx {
                (
                    full_text[s_idx..e_idx].trim().to_string(),
                    full_text[e_idx..].trim().to_string(),
                )
            } else {
                (
                    full_text[s_idx..].trim().to_string(),
                    full_text[e_idx..s_idx].trim().to_string(),
                )
            }
        }
        (Some(s_idx), None) => (
            full_text[s_idx..].trim().to_string(),
            DEFAULT_EVIDENCE.to_string(),
        ),
        (None, Some(e_idx)) => (
            DEFAULT_SUMMARY.to_string(),
            full_text[e_idx..].trim().to_string(),
        ),
        // Prose without an explicit summary section is not a summary. Slicing
        // an arbitrary tail of the transcript only ever produced tool noise.
        (None, None) => (DEFAULT_SUMMARY.to_string(), DEFAULT_EVIDENCE.to_string()),
    };

    let clean_summary =
        clean_header_prefix(&raw_summary, &["## summary", "### summary", "summary:"]);
    let clean_evidence = clean_header_prefix(
        &raw_evidence,
        &[
            "## verification",
            "### verification",
            "## tests",
            "### tests",
            "## evidence",
            "### evidence",
            "verification evidence:",
            "test evidence:",
            "verification:",
            "evidence:",
        ],
    );

    let summary = if clean_summary.is_empty() {
        DEFAULT_SUMMARY.to_string()
    } else {
        clean_summary
    };

    let evidence = if clean_evidence.is_empty() {
        DEFAULT_EVIDENCE.to_string()
    } else {
        clean_evidence
    };

    (summary, evidence)
}

fn extract_acceptance_criteria_from_body(body: &str) -> Option<String> {
    let lower = body.to_lowercase();
    let keywords = [
        "## acceptance criteria",
        "### acceptance criteria",
        "## acceptance",
        "### acceptance",
    ];
    for kw in &keywords {
        if let Some(idx) = lower.find(kw) {
            let section = &body[idx + kw.len()..];
            let end_idx = section.find("\n## ").unwrap_or(section.len());
            let criteria = section[..end_idx].trim();
            if !criteria.is_empty() {
                return Some(criteria.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_header_prefix() {
        assert_eq!(
            clean_header_prefix("## summary\nAll good", &["## summary"]),
            "All good"
        );
        assert_eq!(clean_header_prefix("summary: Done", &["summary:"]), "Done");
        assert_eq!(
            clean_header_prefix("No prefix", &["## summary"]),
            "No prefix"
        );
    }

    #[test]
    fn test_extract_acceptance_criteria() {
        let body = "# Task\n\n## Acceptance Criteria\n- Must pass tests\n- Clean code\n\n## Next\nDo something";
        let ac = extract_acceptance_criteria_from_body(body);
        assert_eq!(ac, Some("- Must pass tests\n- Clean code".to_string()));

        let body2 = "Just some description without criteria.";
        assert_eq!(extract_acceptance_criteria_from_body(body2), None);
    }

    fn assistant_message(text: &str) -> serde_json::Value {
        serde_json::json!({
            "jsonrpc": "2.0",
            "method": "session/update",
            "params": {
                "sessionId": "mock-session-1",
                "update": {
                    "sessionUpdate": "agent_message_chunk",
                    "content": { "type": "text", "text": text }
                }
            }
        })
    }

    #[test]
    fn test_extract_summary_and_evidence() {
        let events = vec![assistant_message(
            "## Summary\nImplemented feature xyz.\n\n## Verification\nRan cargo test and 30 tests passed.",
        )];
        let (summary, evidence) = extract_summary_and_evidence(&events);
        assert_eq!(summary, "Implemented feature xyz.");
        assert_eq!(evidence, "Ran cargo test and 30 tests passed.");
    }

    #[test]
    fn test_build_commit_pr_prompt() {
        let prompt = build_commit_pr_prompt(
            "Add cool feature",
            "item123",
            "technical",
            Some("high"),
            Some("- criterion 1"),
            "Detailed body text",
        );
        assert!(prompt.contains("# Task: Commit and Create PR for 'Add cool feature'"));
        assert!(prompt.contains("- Item ID: item123"));
        assert!(prompt.contains("- Priority: high"));
        assert!(prompt.contains("## Acceptance Criteria\n- criterion 1"));
        assert!(prompt.contains("Detailed body text"));
        assert!(prompt.contains("Stage and commit all relevant changes"));
        assert!(prompt.contains("Create a pull request targeting the main branch"));
    }

    #[test]
    fn test_start_commit_pr_run_rejects_unready_item() {
        let vault_dir = tempfile::tempdir().unwrap();
        let repo_dir = tempfile::tempdir().unwrap();
        let item_id = "testcommitpr";
        let body = "---\nschema: 2\nid: \"testcommitpr\"\ntitle: \"Test Commit PR\"\nstatus: \"planned\"\n---\n# Test Commit PR\n";
        let file_path = vault_dir.path().join(format!("{item_id}.md"));
        std::fs::write(&file_path, body).unwrap();

        let mgr = AcpManager::new(
            vault_dir.path().to_path_buf(),
            repo_dir.path().to_path_buf(),
            None,
        );
        let res = mgr.start_commit_pr_run(item_id);
        assert!(matches!(res, Err(AcpManagerError::Workflow(_))));
    }

    #[test]
    fn test_extract_summary_and_evidence_fallback() {
        let events = Vec::new();
        let (summary, evidence) = extract_summary_and_evidence(&events);
        assert_eq!(summary, DEFAULT_SUMMARY);
        assert_eq!(evidence, DEFAULT_EVIDENCE);
    }

    #[test]
    fn test_agent_prompt_includes_screenshot_path_and_file_is_readable() {
        use tempfile::tempdir;

        // 1. Create a dummy screenshot on disk in temp dir (as created by POST /screenshots)
        let screenshot_dir = std::env::temp_dir().join("pin-screenshots");
        std::fs::create_dir_all(&screenshot_dir).unwrap();
        let ss_filename = "screenshot-test-agent-readable.png";
        let ss_path = screenshot_dir.join(ss_filename);
        let png_bytes = b"\x89PNG\r\n\x1a\nagent-test-image-data";
        std::fs::write(&ss_path, png_bytes).unwrap();
        let ss_path_str = ss_path.to_string_lossy().to_string();

        // 2. Create vault and item with screenshot markdown in body
        let vault_dir = tempdir().unwrap();
        let item_id = "testss123456";
        let body_with_ss = format!("# Test Task\n\n## Description\nInvestigate issue.\n![screenshot]({ss_path_str})\n\n## Acceptance Criteria\n- [ ] Fix issue");
        let file_path = vault_dir.path().join(format!("{item_id}.md"));
        std::fs::write(&file_path, &body_with_ss).unwrap();

        // 3. Build prompt following the exact logic in AcpManager::start_run
        let content = std::fs::read_to_string(&file_path).unwrap();
        let mut issues = Vec::new();
        let meta = crate::frontmatter::parse_front_matter_detailed(
            &format!("{item_id}.md"),
            &content,
            &mut issues,
        );
        let title = meta.as_ref().map(|m| m.title.as_str()).unwrap_or(item_id);
        let parsed_body = meta.as_ref().map(|m| m.body.as_str()).unwrap_or(&content);
        let ac = extract_acceptance_criteria_from_body(parsed_body);
        let prompt_text = build_session_prompt(
            title,
            item_id,
            "technical",
            None,
            ac.as_deref(),
            parsed_body,
        );
        assert!(prompt_text.contains("Do not archive this item"));

        // 4. Verify prompt contains the screenshot path
        assert!(prompt_text.contains(&ss_path_str));

        // 5. Verify the agent can read the exact screenshot bytes from disk
        assert!(std::path::Path::new(&ss_path_str).is_file());
        let read_bytes = std::fs::read(&ss_path_str).unwrap();
        assert_eq!(read_bytes, png_bytes);

        // Cleanup
        let _ = std::fs::remove_file(&ss_path);
    }

    #[test]
    fn test_extract_summary_ignores_tool_output_and_files() {
        let events = vec![
            serde_json::json!({
                "params": {
                    "update": {
                        "sessionUpdate": "tool_call",
                        "title": "read .agents/skills/pin/SKILL.md",
                        "content": {
                            "type": "text",
                            "text": "---\nname: pin\n## Summary\nSkill frontmatter leaked"
                        }
                    }
                }
            }),
            serde_json::json!({
                "params": {
                    "update": {
                        "sessionUpdate": "tool_call_update",
                        "rawOutput": "## Verification\nfn main() { repo file body }"
                    }
                }
            }),
            serde_json::json!({
                "params": {
                    "update": {
                        "sessionUpdate": "agent_thought_chunk",
                        "content": { "type": "text", "text": "## Summary\ninternal reasoning" }
                    }
                }
            }),
            assistant_message("Wrapped up the change."),
        ];

        let (summary, evidence) = extract_summary_and_evidence(&events);
        assert_eq!(summary, DEFAULT_SUMMARY);
        assert_eq!(evidence, DEFAULT_EVIDENCE);
    }
}
