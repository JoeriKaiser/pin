use crate::acp::client::{AcpClient, AcpError};
use crate::acp::worktree::{WorktreeError, WorktreeManager};
use crate::workflow::WorkflowError;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufWriter, Write};
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
    InvalidSettings(String),
    ModelDiscovery(String),
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
            AcpManagerError::InvalidSettings(msg) => write!(f, "Invalid runner settings: {msg}"),
            AcpManagerError::ModelDiscovery(msg) => write!(f, "Model discovery failed: {msg}"),
        }
    }
}

/// The agent process, vault attribution, and model used for new runs.
///
/// Settings are captured when a run starts, so changing them in the viewer
/// never rewrites the attribution of a run that is already in flight.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct RunnerSettings {
    pub command: String,
    pub actor: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

pub const DEFAULT_ACP_COMMAND: &str = "omp acp";
pub const DEFAULT_ACP_ACTOR: &str = "agent:omp";

fn non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn env_non_empty(name: &str) -> Option<String> {
    non_empty(std::env::var(name).ok())
}

impl RunnerSettings {
    /// Explicit values win, then environment variables, then the historical
    /// defaults. This keeps `pin view` behaving exactly as before unless the
    /// user opts into a different runner.
    pub fn resolve(command: Option<String>, actor: Option<String>, model: Option<String>) -> Self {
        Self {
            command: non_empty(command)
                .or_else(|| env_non_empty("PIN_ACP_COMMAND"))
                .unwrap_or_else(|| DEFAULT_ACP_COMMAND.to_string()),
            actor: non_empty(actor)
                .or_else(|| env_non_empty("PIN_ACP_ACTOR"))
                .unwrap_or_else(|| DEFAULT_ACP_ACTOR.to_string()),
            model: non_empty(model).or_else(|| env_non_empty("PIN_ACP_MODEL")),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct RunnerModelOption {
    pub value: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct RunnerModels {
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<String>,
    pub options: Vec<RunnerModelOption>,
}

impl std::error::Error for AcpManagerError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMode {
    Implement {
        use_worktree: bool,
        interactive: bool,
    },
    CommitPr,
    Idea {
        use_worktree: bool,
    },
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

#[derive(Debug, Clone, Default)]
pub struct RunLiveState {
    pub waiting_for_user: bool,
    pub waiting_since: Option<i64>,
    pub current_turn_message: String,
    pub full_assistant_text: String,
    pub last_message: Option<String>,
    pub last_activity: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptCommand {
    Message(String),
    Finish,
}

#[derive(Clone)]
pub struct ActiveRun {
    pub item_id: String,
    pub client: Arc<AcpClient>,
    pub session_id: Arc<Mutex<Option<String>>>,
    pub pre_run_status: crate::model::Status,
    pub worktree_path: Option<PathBuf>,
    pub is_new_worktree: bool,
    pub is_primary_worktree: bool,
    pub event_buffer: Arc<Mutex<Vec<serde_json::Value>>>,
    pub subscribers: Arc<Mutex<Vec<Sender<serde_json::Value>>>>,
    pub started_at: i64,
    pub cancel_requested: Arc<AtomicBool>,
    pub prompt_sender: Option<Sender<PromptCommand>>,
    pub mode: RunMode,
    pub runner: RunnerSettings,
    pub live_state: Arc<Mutex<RunLiveState>>,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ActiveRunInfo {
    pub item_id: String,
    pub mode: String,
    pub interactive: bool,
    pub waiting_for_user: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub waiting_since: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_activity: Option<String>,
    pub is_primary_worktree: bool,
    pub worktree_path: Option<String>,
    pub started_at: i64,
    pub actor: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}
#[derive(Clone)]
pub struct CompletedRun {
    pub item_id: String,
    pub events: Vec<serde_json::Value>,
    pub started_at: i64,
    pub finished_at: i64,
    pub stop_reason: Option<String>,
}

struct StartingRunGuard {
    starting_runs: Arc<Mutex<HashSet<String>>>,
    key: String,
}

impl Drop for StartingRunGuard {
    fn drop(&mut self) {
        if let Ok(mut set) = self.starting_runs.lock() {
            set.remove(&self.key);
        }
    }
}

#[derive(Clone)]
pub struct AcpManager {
    vault_path: PathBuf,
    repo_path: PathBuf,
    runner: Arc<Mutex<RunnerSettings>>,
    model_cache: Arc<Mutex<HashMap<String, RunnerModels>>>,
    starting_runs: Arc<Mutex<HashSet<String>>>,
    active_runs: Arc<Mutex<HashMap<String, ActiveRun>>>,
    completed_runs: Arc<Mutex<Vec<CompletedRun>>>,
    global_subscribers: Arc<Mutex<Vec<std::sync::mpsc::Sender<serde_json::Value>>>>,
}

impl AcpManager {
    pub fn new(vault_path: PathBuf, repo_path: PathBuf, acp_command: Option<String>) -> Self {
        Self::with_runner(
            vault_path,
            repo_path,
            RunnerSettings::resolve(acp_command, None, None),
        )
    }

    pub fn with_runner(vault_path: PathBuf, repo_path: PathBuf, runner: RunnerSettings) -> Self {
        Self {
            vault_path,
            repo_path,
            runner: Arc::new(Mutex::new(runner)),
            model_cache: Arc::new(Mutex::new(HashMap::new())),
            starting_runs: Arc::new(Mutex::new(HashSet::new())),
            active_runs: Arc::new(Mutex::new(HashMap::new())),
            completed_runs: Arc::new(Mutex::new(Vec::new())),
            global_subscribers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn add_global_listener(&self, tx: std::sync::mpsc::Sender<serde_json::Value>) {
        if let Ok(mut subs) = self.global_subscribers.lock() {
            subs.push(tx);
        }
    }

    pub fn vault_path(&self) -> &Path {
        &self.vault_path
    }

    pub fn repo_path(&self) -> &Path {
        &self.repo_path
    }

    pub fn runner_settings(&self) -> RunnerSettings {
        self.runner
            .lock()
            .map(|runner| runner.clone())
            .unwrap_or_else(|_| RunnerSettings::resolve(None, None, None))
    }

    /// Updates runner settings in place. Omitted fields keep their current
    /// value; `model: null` clears the model and falls back to the agent's
    /// default for future runs.
    pub fn update_runner_settings(
        &self,
        command: Option<String>,
        actor: Option<String>,
        model: Option<Option<String>>,
    ) -> Result<RunnerSettings, AcpManagerError> {
        let mut runner = self.runner.lock().map_err(|_| {
            AcpManagerError::InvalidSettings("Runner settings lock poisoned".to_string())
        })?;

        if let Some(command) = command {
            let command = command.trim();
            if command.is_empty() {
                return Err(AcpManagerError::InvalidSettings(
                    "Agent command cannot be empty".to_string(),
                ));
            }
            runner.command = command.to_string();
        }

        if let Some(actor) = actor {
            let actor = actor.trim();
            if actor.is_empty() {
                return Err(AcpManagerError::InvalidSettings(
                    "Runner actor cannot be empty".to_string(),
                ));
            }
            runner.actor = actor.to_string();
        }

        if let Some(model) = model {
            runner.model = non_empty(model);
        }

        Ok(runner.clone())
    }

    /// Lists the model options advertised by the current agent command.
    ///
    /// Discovery is a short-lived ACP handshake: initialize, create a session,
    /// read `configOptions`, then kill the child. No prompt is ever sent.
    pub fn runner_models(&self, refresh: bool) -> Result<RunnerModels, AcpManagerError> {
        let settings = self.runner_settings();

        if !refresh {
            if let Ok(cache) = self.model_cache.lock() {
                if let Some(cached) = cache.get(&settings.command) {
                    return Ok(cached.clone());
                }
            }
        }

        let models = self.discover_models(&settings.command)?;
        if let Ok(mut cache) = self.model_cache.lock() {
            cache.insert(settings.command.clone(), models.clone());
        }

        Ok(models)
    }

    fn discover_models(&self, command: &str) -> Result<RunnerModels, AcpManagerError> {
        let log_dir = self.vault_path.join("runs");
        fs::create_dir_all(&log_dir)?;
        let log_path = log_dir.join("runner-models.log");

        let client = AcpClient::spawn(command, &self.repo_path, &log_path).map_err(|e| {
            AcpManagerError::ModelDiscovery(format!("Could not start '{command}': {e}"))
        })?;

        let result: Result<RunnerModels, AcpManagerError> = (|| {
            client.initialize().map_err(|e| {
                AcpManagerError::ModelDiscovery(format!("ACP initialize failed: {e}"))
            })?;

            let session = client
                .new_session_with_config(&self.repo_path)
                .map_err(|e| {
                    AcpManagerError::ModelDiscovery(format!("Could not create ACP session: {e}"))
                })?;

            let Some(model_option) = session
                .config_options
                .iter()
                .find(|option| option.category.as_deref() == Some("model") || option.id == "model")
            else {
                return Ok(RunnerModels {
                    command: command.to_string(),
                    current: None,
                    options: Vec::new(),
                });
            };

            let current = model_option
                .current_value
                .as_ref()
                .and_then(|value| value.as_str())
                .map(str::to_string);

            let options = model_option
                .options
                .iter()
                .filter_map(|choice| {
                    let value = choice.value.as_str()?.to_string();
                    Some(RunnerModelOption {
                        name: choice.name.clone().unwrap_or_else(|| value.clone()),
                        value,
                        description: choice.description.clone(),
                    })
                })
                .collect();

            Ok(RunnerModels {
                command: command.to_string(),
                current,
                options,
            })
        })();

        let _ = client.kill();
        result
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
    pub fn active_runs_info(&self) -> Vec<ActiveRunInfo> {
        if let Ok(runs) = self.active_runs.lock() {
            let mut list: Vec<ActiveRunInfo> =
                runs.values().map(active_run_info_from_run).collect();
            list.sort_by(|a, b| a.item_id.cmp(&b.item_id));
            list
        } else {
            Vec::new()
        }
    }

    pub fn get_active_run_info(&self, item_id: &str) -> Option<ActiveRunInfo> {
        let runs = self.active_runs.lock().ok()?;
        let key = resolve_run_key(&runs, item_id)?;
        runs.get(&key).map(active_run_info_from_run)
    }

    fn resolve_transcript_path(&self, item_id: &str) -> Option<PathBuf> {
        let canonical_id = match crate::vault::resolve_selector(&self.vault_path, item_id) {
            Ok(f) => f.strip_suffix(".md").unwrap_or(&f).to_string(),
            Err(_) if crate::vault::validate_selector(item_id) => item_id.to_string(),
            Err(_) => return None,
        };
        if !crate::vault::validate_selector(&canonical_id) {
            return None;
        }
        let disk_path = self
            .vault_path
            .join("runs")
            .join(format!("{canonical_id}.events.jsonl"));
        if disk_path.is_file() {
            Some(disk_path)
        } else {
            None
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
        let disk_path = self.resolve_transcript_path(item_id)?;
        read_latest_timing_from_transcript(&disk_path)
    }

    /// Streams buffered/persisted SSE frames directly to `writer` and atomically
    /// subscribes to live updates when the item has an active run, closing the
    /// replay-to-live gap and avoiding loading large append-only transcripts into memory.
    pub fn stream_events_and_subscribe<W: std::io::Write>(
        &self,
        item_id: &str,
        writer: &mut W,
    ) -> std::io::Result<(usize, bool, Option<Receiver<serde_json::Value>>)> {
        // 1. Active run: snapshot buffer, live run state, and subscribe atomically before releasing locks.
        let active_snapshot = if let Ok(runs) = self.active_runs.lock() {
            if let Some(key) = resolve_run_key(&runs, item_id) {
                if let Some(run) = runs.get(&key) {
                    let buffered = run
                        .event_buffer
                        .lock()
                        .map(|buf| buf.clone())
                        .unwrap_or_default();
                    let info = active_run_info_from_run(run);
                    let (tx, rx) = mpsc::channel();
                    if let Ok(mut subs) = run.subscribers.lock() {
                        subs.push(tx);
                    }
                    Some((buffered, info, rx))
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        if let Some((buffered, info, rx)) = active_snapshot {
            let (mut count, has_finished) = write_value_events_as_sse(&buffered, writer)?;
            if !has_finished {
                let state_frame = serde_json::json!({
                    "type": "run_state",
                    "mode": info.mode,
                    "interactive": info.interactive,
                    "waiting_for_user": info.waiting_for_user,
                    "waiting_since": info.waiting_since,
                    "last_message": info.last_message,
                });
                let payload = serde_json::to_string(&state_frame).unwrap_or_default();
                writer.write_all(b"data: ")?;
                writer.write_all(payload.as_bytes())?;
                writer.write_all(b"\n\n")?;
                count += 1;
            }
            return Ok((count, has_finished, Some(rx)));
        }

        // 2. Recently completed run in memory.
        let completed_snapshot = if let Ok(comp) = self.completed_runs.lock() {
            comp.iter()
                .rev()
                .find(|run| {
                    run.item_id == item_id
                        || run.item_id.starts_with(item_id)
                        || item_id.starts_with(&run.item_id)
                })
                .map(|run| run.events.clone())
        } else {
            None
        };

        if let Some(events) = completed_snapshot {
            let (count, has_finished) = write_value_events_as_sse(&events, writer)?;
            return Ok((count, has_finished, None));
        }

        // 3. Append-only transcript on disk: stream line-by-line without DOM parsing.
        if let Some(disk_path) = self.resolve_transcript_path(item_id) {
            if let Ok(file) = std::fs::File::open(&disk_path) {
                use std::io::BufRead;
                let mut reader = std::io::BufReader::new(file);
                let mut line = String::new();
                let mut count = 0usize;
                let mut has_finished = false;
                loop {
                    line.clear();
                    let n = reader.read_line(&mut line)?;
                    if n == 0 {
                        break;
                    }
                    let trimmed = line.trim();
                    if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
                        continue;
                    }
                    if trimmed.contains("\"type\":\"finished\"")
                        || trimmed.contains("\"type\": \"finished\"")
                        || trimmed.contains("\"type\":\"error\"")
                        || trimmed.contains("\"type\": \"error\"")
                    {
                        has_finished = true;
                    }
                    writer.write_all(b"data: ")?;
                    writer.write_all(trimmed.as_bytes())?;
                    writer.write_all(b"\n\n")?;
                    count += 1;
                }
                return Ok((count, has_finished, None));
            }
        }

        Ok((0, false, None))
    }

    #[allow(dead_code)]
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
        let Some(disk_path) = self.resolve_transcript_path(item_id) else {
            return Vec::new();
        };
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
        Vec::new()
    }

    #[allow(dead_code)]
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
        self.start_run_interactive(item_id, use_worktree, false)
    }

    pub fn start_run_interactive(
        &self,
        item_id: &str,
        use_worktree: bool,
        interactive: bool,
    ) -> Result<(), AcpManagerError> {
        self.start_run_mode(
            item_id,
            RunMode::Implement {
                use_worktree,
                interactive,
            },
        )
    }

    pub fn start_commit_pr_run(&self, item_id: &str) -> Result<(), AcpManagerError> {
        self.start_run_mode(item_id, RunMode::CommitPr)
    }

    pub fn start_idea_run(&self, item_id: &str, use_worktree: bool) -> Result<(), AcpManagerError> {
        self.start_run_mode(item_id, RunMode::Idea { use_worktree })
    }

    pub fn send_message(&self, item_id: &str, message: &str) -> Result<(), AcpManagerError> {
        let trimmed = message.trim();
        if trimmed.is_empty() {
            return Err(AcpManagerError::Workflow(
                "Message cannot be empty".to_string(),
            ));
        }
        let runs = self
            .active_runs
            .lock()
            .map_err(|_| AcpManagerError::Io("Lock poisoned on active_runs".to_string()))?;
        if let Some(key) = resolve_run_key(&runs, item_id) {
            if let Some(run) = runs.get(&key) {
                if let Some(sender) = &run.prompt_sender {
                    sender
                        .send(PromptCommand::Message(trimmed.to_string()))
                        .map_err(|_| {
                            AcpManagerError::Client("Run prompt receiver closed".to_string())
                        })?;
                    return Ok(());
                } else {
                    return Err(AcpManagerError::Workflow(
                        "Run does not accept interactive messages".to_string(),
                    ));
                }
            }
        }
        Err(AcpManagerError::ItemNotFound(format!(
            "Item '{item_id}' is not currently running"
        )))
    }

    pub fn finish_run(&self, item_id: &str) -> Result<(), AcpManagerError> {
        let runs = self
            .active_runs
            .lock()
            .map_err(|_| AcpManagerError::Io("Lock poisoned on active_runs".to_string()))?;
        if let Some(key) = resolve_run_key(&runs, item_id) {
            if let Some(run) = runs.get(&key) {
                if let Some(sender) = &run.prompt_sender {
                    sender.send(PromptCommand::Finish).map_err(|_| {
                        AcpManagerError::Client("Run prompt receiver closed".to_string())
                    })?;
                    return Ok(());
                } else {
                    return Err(AcpManagerError::Workflow(
                        "Run does not accept interactive finish".to_string(),
                    ));
                }
            }
        }
        Err(AcpManagerError::ItemNotFound(format!(
            "Item '{item_id}' is not currently running"
        )))
    }

    pub fn start_run_mode(&self, item_id: &str, mode: RunMode) -> Result<(), AcpManagerError> {
        // Resolve filename and canonical ID first
        let filename = crate::vault::resolve_selector(&self.vault_path, item_id)
            .map_err(|e| AcpManagerError::ItemNotFound(format!("{item_id}: {e}")))?;
        let canonical_id = filename
            .strip_suffix(".md")
            .unwrap_or(&filename)
            .to_string();

        // Capture the runner once so every attribution and ACP call in this
        // run uses the same command, actor, and model.
        let runner = self.runner_settings();

        let will_use_primary = match mode {
            RunMode::Implement { use_worktree, .. } | RunMode::Idea { use_worktree } => {
                !use_worktree
            }
            RunMode::CommitPr => {
                !WorktreeManager::worktree_path(&self.repo_path, &canonical_id).exists()
                    && !WorktreeManager::is_git_repo(&self.repo_path)
            }
        };

        // 1. Reserve canonical_id in starting_runs and reject if already running
        //    or primary checkout is busy before creating any worktree on disk.
        let _start_guard = {
            let mut starting = self
                .starting_runs
                .lock()
                .map_err(|_| AcpManagerError::Io("Lock poisoned on starting_runs".to_string()))?;
            let runs = self
                .active_runs
                .lock()
                .map_err(|_| AcpManagerError::Io("Lock poisoned on active_runs".to_string()))?;
            if starting.contains(&canonical_id) || resolve_run_key(&runs, &canonical_id).is_some() {
                return Err(AcpManagerError::AlreadyRunning(canonical_id));
            }

            if will_use_primary {
                if let Some(busy_run) = runs.values().find(|r| r.is_primary_worktree) {
                    return Err(AcpManagerError::PrimaryBusy(busy_run.item_id.clone()));
                }
            }
            starting.insert(canonical_id.clone());
            StartingRunGuard {
                starting_runs: Arc::clone(&self.starting_runs),
                key: canonical_id.clone(),
            }
        };

        // Capture the item's status before beginning the run so error/cancel
        // restores the real prior status instead of clobbering Created/Blocked.
        let pre_run_status = fs::read_to_string(self.vault_path.join(&filename))
            .ok()
            .and_then(|raw| {
                let mut diag = Vec::new();
                crate::frontmatter::parse_front_matter_detailed(&filename, &raw, &mut diag)
            })
            .map(|m| m.current_status())
            .filter(|s| *s != crate::model::Status::InProgress)
            .unwrap_or(if matches!(mode, RunMode::CommitPr) {
                crate::model::Status::Done
            } else {
                crate::model::Status::Planned
            });

        // Determine working directory using canonical_id so short prefixes match logs/worktrees.
        let expected_wt = WorktreeManager::worktree_path(&self.repo_path, &canonical_id);
        let wt_existed_before = expected_wt.exists();
        let (work_dir, worktree_path, is_primary) = match mode {
            RunMode::Implement { use_worktree, .. } | RunMode::Idea { use_worktree } => {
                if use_worktree {
                    let wt = WorktreeManager::provision_worktree(&self.repo_path, &canonical_id)
                        .map_err(|e| AcpManagerError::Worktree(e.to_string()))?;
                    (wt.clone(), Some(wt), false)
                } else {
                    (self.repo_path.clone(), None, true)
                }
            }
            RunMode::CommitPr => {
                if wt_existed_before {
                    (expected_wt.clone(), Some(expected_wt), false)
                } else if WorktreeManager::is_git_repo(&self.repo_path) {
                    match WorktreeManager::provision_worktree(&self.repo_path, &canonical_id) {
                        Ok(wt) => (wt.clone(), Some(wt), false),
                        Err(_) => (self.repo_path.clone(), None, true),
                    }
                } else {
                    (self.repo_path.clone(), None, true)
                }
            }
        };
        let is_new_worktree = worktree_path.is_some() && !wt_existed_before;

        // If CommitPr fell back to primary checkout, ensure primary is still free.
        if is_primary && !will_use_primary {
            let runs = self
                .active_runs
                .lock()
                .map_err(|_| AcpManagerError::Io("Lock poisoned on active_runs".to_string()))?;
            if let Some(busy_run) = runs.values().find(|r| r.is_primary_worktree) {
                return Err(AcpManagerError::PrimaryBusy(busy_run.item_id.clone()));
            }
        }

        // Attribute the item in the vault to this run
        let begin_result = match mode {
            RunMode::Implement { .. } | RunMode::Idea { .. } => {
                crate::workflow::begin_agent_run(&self.vault_path, &filename, &runner.actor)
            }
            RunMode::CommitPr => {
                crate::workflow::begin_commit_pr_run(&self.vault_path, &filename, &runner.actor)
            }
        };
        if let Err(e) = begin_result {
            if is_new_worktree {
                let _ = WorktreeManager::remove_worktree(&self.repo_path, &canonical_id);
            }
            return Err(AcpManagerError::Workflow(e.to_string()));
        }

        // 6. Read item content from vault to build prompt
        let file_path = self.vault_path.join(&filename);
        let content = match fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(e) => {
                let _ = crate::workflow::release_item(
                    &self.vault_path,
                    &filename,
                    Some(&runner.actor),
                    true,
                    None,
                );
                if is_new_worktree {
                    let _ = WorktreeManager::remove_worktree(&self.repo_path, &canonical_id);
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
                    serde_norway::Value::String(s) => acceptance_criteria = Some(s.clone()),
                    serde_norway::Value::Sequence(seq) => {
                        let items: Vec<String> = seq
                            .iter()
                            .filter_map(|item| match item {
                                serde_norway::Value::String(s) => Some(format!("- {s}")),
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
            RunMode::Implement { interactive, .. } => {
                let mut prompt = build_session_prompt(
                    title,
                    &canonical_id,
                    &kind.to_string(),
                    priority,
                    acceptance_criteria.as_deref(),
                    body,
                );
                if interactive {
                    prompt.push_str("\n\n## Interactive Session\nThis run is interactive. If requirements, tradeoffs, or scope are ambiguous, ask focused clarifying questions and pause for the user's reply before changing code. After implementing and verifying, summarize your changes under '## Summary' and '## Verification' and pause so the user can review or ask for follow-ups before completing the run.");
                }
                prompt
            }
            RunMode::CommitPr => build_commit_pr_prompt(
                title,
                &canonical_id,
                &kind.to_string(),
                priority,
                acceptance_criteria.as_deref(),
                body,
            ),
            RunMode::Idea { .. } => build_idea_prompt(
                title,
                &canonical_id,
                kind,
                priority,
                acceptance_criteria.as_deref(),
                body,
            ),
        };

        // 7. Prepare log path: self.vault_path.join("runs").join(format!("{canonical_id}.log"))
        let log_dir = self.vault_path.join("runs");
        if let Err(e) = fs::create_dir_all(&log_dir) {
            let _ = crate::workflow::release_item(
                &self.vault_path,
                &filename,
                Some(&runner.actor),
                true,
                None,
            );
            if is_new_worktree {
                let _ = WorktreeManager::remove_worktree(&self.repo_path, &canonical_id);
            }
            return Err(AcpManagerError::Io(e.to_string()));
        }
        let log_path = log_dir.join(format!("{canonical_id}.log"));

        // Spawn ACP client
        let client = match AcpClient::spawn(&runner.command, &work_dir, &log_path) {
            Ok(c) => Arc::new(c),
            Err(e) => {
                let _ = crate::workflow::release_item(
                    &self.vault_path,
                    &filename,
                    Some(&runner.actor),
                    true,
                    None,
                );
                if is_new_worktree {
                    let _ = WorktreeManager::remove_worktree(&self.repo_path, &canonical_id);
                }
                return Err(AcpManagerError::Client(e.to_string()));
            }
        };

        let event_buffer = Arc::new(Mutex::new(Vec::new()));
        let subscribers = Arc::new(Mutex::new(Vec::new()));
        let cancel_requested = Arc::new(AtomicBool::new(false));
        let live_state = Arc::new(Mutex::new(RunLiveState::default()));
        let started_at = chrono::Utc::now().timestamp();

        // 8. Insert ActiveRun into active_runs
        let (prompt_tx, prompt_rx) = mpsc::channel::<PromptCommand>();
        let is_interactive = matches!(
            mode,
            RunMode::Idea { .. }
                | RunMode::Implement {
                    interactive: true,
                    ..
                }
        );
        let prompt_sender = if is_interactive {
            Some(prompt_tx)
        } else {
            None
        };

        let session_id_shared: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

        let active_run = ActiveRun {
            item_id: canonical_id.clone(),
            client: Arc::clone(&client),
            session_id: Arc::clone(&session_id_shared),
            pre_run_status,
            worktree_path: worktree_path.clone(),
            is_new_worktree,
            is_primary_worktree: is_primary,
            event_buffer: Arc::clone(&event_buffer),
            subscribers: Arc::clone(&subscribers),
            started_at,
            cancel_requested: Arc::clone(&cancel_requested),
            prompt_sender,
            mode,
            runner: runner.clone(),
            live_state: Arc::clone(&live_state),
        };

        {
            if let Ok(mut runs) = self.active_runs.lock() {
                runs.insert(canonical_id.clone(), active_run);
            }
        }
        let mode_str = run_mode_label(mode);
        if let Ok(mut subs) = self.global_subscribers.lock() {
            let start_evt = serde_json::json!({
                "type": "run_started",
                "id": canonical_id,
                "mode": mode_str,
            });
            subs.retain(|tx| tx.send(start_evt.clone()).is_ok());
        }
        // 9. Spawn worker thread
        let worker_client = Arc::clone(&client);
        let worker_session_id = Arc::clone(&session_id_shared);
        let worker_event_buffer = Arc::clone(&event_buffer);
        let worker_subscribers = Arc::clone(&subscribers);
        let worker_cancel = Arc::clone(&cancel_requested);
        let worker_live_state = Arc::clone(&live_state);
        let worker_vault_path = self.vault_path.clone();
        let worker_repo_path = self.repo_path.clone();
        let worker_item_id = canonical_id.clone();
        let worker_title = title.to_string();
        let worker_filename = filename.clone();
        let worker_active_runs = Arc::clone(&self.active_runs);
        let worker_completed_runs = Arc::clone(&self.completed_runs);
        let worker_global_subscribers = Arc::clone(&self.global_subscribers);
        let worker_work_dir = work_dir;
        let worker_is_new_worktree = is_new_worktree;
        let worker_prompt_rx = prompt_rx;
        let worker_runner = runner.clone();
        thread::spawn(move || {
            // Forward client update events (client.subscribe()) to event_buffer and subscribers
            let client_event_rx = worker_client.subscribe();
            let fwd_buffer = Arc::clone(&worker_event_buffer);
            let fwd_subs = Arc::clone(&worker_subscribers);
            let fwd_live = Arc::clone(&worker_live_state);
            let disk_events_path = worker_vault_path
                .join("runs")
                .join(format!("{worker_item_id}.events.jsonl"));
            let fwd_disk_path = disk_events_path.clone();
            thread::spawn(move || {
                let mut disk_writer = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&fwd_disk_path)
                    .ok()
                    .map(BufWriter::new);
                while let Ok(event) = client_event_rx.recv() {
                    record_live_event(&fwd_live, &event);
                    push_event_with_writer(
                        &fwd_buffer,
                        &fwd_subs,
                        disk_writer.as_mut(),
                        None,
                        event,
                    );
                }
                if let Some(ref mut w) = disk_writer {
                    let _ = w.flush();
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
                let session = worker_client
                    .new_session_with_config(&worker_work_dir)
                    .map_err(|e| format!("New session failed: {e}"))?;
                let session_id = session.session_id.clone();
                session_id_opt = Some(session_id.clone());
                if let Ok(mut sid) = worker_session_id.lock() {
                    *sid = Some(session_id.clone());
                }

                // The user picked a model in the viewer: apply it to this
                // session before the first prompt so every turn uses it.
                if let Some(model) = worker_runner.model.as_deref() {
                    if worker_cancel.load(Ordering::SeqCst) {
                        return Err("Run cancelled before model selection".to_string());
                    }
                    let config_id = session
                        .config_options
                        .iter()
                        .find(|option| {
                            option.category.as_deref() == Some("model") || option.id == "model"
                        })
                        .map(|option| option.id.as_str())
                        .unwrap_or("model");
                    worker_client
                        .set_config_option(&session_id, config_id, serde_json::json!(model))
                        .map_err(|e| format!("Model selection failed for '{model}': {e}"))?;
                }

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

                if is_interactive {
                    let mut last_stop_reason = prompt_res
                        .stop_reason
                        .unwrap_or_else(|| "end_turn".to_string());

                    enter_waiting_state(
                        &worker_live_state,
                        &worker_event_buffer,
                        &worker_subscribers,
                        &worker_global_subscribers,
                        Some(&disk_events_path),
                        &worker_item_id,
                        &worker_title,
                        mode_str,
                        &last_stop_reason,
                    );

                    while let Ok(cmd) = worker_prompt_rx.recv() {
                        if worker_cancel.load(Ordering::SeqCst) {
                            return Err("Run cancelled by user".to_string());
                        }
                        let user_msg = match cmd {
                            PromptCommand::Finish => {
                                clear_waiting_state(
                                    &worker_live_state,
                                    &worker_global_subscribers,
                                    &worker_item_id,
                                    mode_str,
                                    "Finishing",
                                );
                                break;
                            }
                            PromptCommand::Message(msg) => msg,
                        };
                        let trimmed = user_msg.trim();
                        if trimmed.is_empty() {
                            continue;
                        }

                        clear_waiting_state(
                            &worker_live_state,
                            &worker_global_subscribers,
                            &worker_item_id,
                            mode_str,
                            "Running",
                        );

                        let user_event = serde_json::json!({
                            "type": "user_message",
                            "update": {
                                "type": "user_message",
                                "content": trimmed,
                                "timestamp": chrono::Utc::now().timestamp(),
                            }
                        });
                        push_event(
                            &worker_event_buffer,
                            &worker_subscribers,
                            Some(&disk_events_path),
                            user_event,
                        );

                        let next_res = worker_client
                            .prompt(&session_id, trimmed)
                            .map_err(|e| format!("Prompt failed: {e}"))?;

                        if worker_cancel.load(Ordering::SeqCst) {
                            return Err("Run cancelled during prompt".to_string());
                        }

                        last_stop_reason = next_res
                            .stop_reason
                            .unwrap_or_else(|| "end_turn".to_string());

                        enter_waiting_state(
                            &worker_live_state,
                            &worker_event_buffer,
                            &worker_subscribers,
                            &worker_global_subscribers,
                            Some(&disk_events_path),
                            &worker_item_id,
                            &worker_title,
                            mode_str,
                            &last_stop_reason,
                        );
                    }

                    if worker_cancel.load(Ordering::SeqCst) {
                        return Err("Run cancelled by user".to_string());
                    }

                    return Ok(last_stop_reason);
                }

                Ok(prompt_res
                    .stop_reason
                    .unwrap_or_else(|| "end_turn".to_string()))
            })();
            match run_result {
                Ok(stop_reason) => {
                    // Give the event forwarding thread a moment to flush queued events
                    thread::sleep(Duration::from_millis(50));

                    // Extract summary and test evidence from full accumulated assistant prose
                    // (which is not truncated by the ring buffer) with fallback to buffered events.
                    let events = worker_event_buffer
                        .lock()
                        .map(|buf| buf.clone())
                        .unwrap_or_default();
                    let full_text = worker_live_state
                        .lock()
                        .map(|live| live.full_assistant_text.clone())
                        .unwrap_or_default();
                    let (summary, evidence) = if !full_text.trim().is_empty() {
                        extract_summary_and_evidence_from_text(&full_text)
                    } else {
                        extract_summary_and_evidence(&events)
                    };
                    let (mut pr_url, mut pr_number) = extract_pr_info(&events);
                    if pr_url.is_none() {
                        let (u, n) = crate::workflow::extract_pr_from_text(&summary);
                        if u.is_some() {
                            pr_url = u;
                            pr_number = n;
                        }
                    }
                    if pr_url.is_none() {
                        let (u, n) = crate::workflow::extract_pr_from_text(&evidence);
                        if u.is_some() {
                            pr_url = u;
                            pr_number = n;
                        }
                    }
                    if pr_url.is_none() {
                        if let Some(prs) = crate::sync::query_gh_prs(&worker_repo_path) {
                            if let Some(gh_pr) =
                                crate::sync::find_pr_for_item(&prs, &worker_item_id)
                            {
                                pr_url = Some(gh_pr.url.clone());
                                pr_number = Some(gh_pr.number);
                            }
                        }
                    }
                    // Record the handoff, then complete the item. `done` is the
                    // single end status: the agent verified its own work, so the
                    // run finishes by completing with the extracted evidence. A
                    // user who moved the item elsewhere while the run was active
                    // keeps that status.
                    let handoff_meta = crate::workflow::handoff_item(
                        &worker_vault_path,
                        &worker_filename,
                        Some(&worker_runner.actor),
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
                        let _ = crate::workflow::complete_item_with_pr(
                            &worker_vault_path,
                            &worker_filename,
                            Some(&worker_runner.actor),
                            &evidence,
                            pr_url.clone(),
                            pr_number,
                            None,
                        );
                    }

                    // Completion already released the claim; nothing else to
                    // attribute here.

                    // Sends {"type": "finished", "stopReason": stop_reason} to subscribers
                    let finished_at = chrono::Utc::now().timestamp();
                    let mut finish_event = serde_json::json!({
                        "type": "finished",
                        "stopReason": stop_reason.clone(),
                        "startedAt": started_at,
                        "finishedAt": finished_at
                    });
                    if let Some(u) = &pr_url {
                        finish_event["pr_url"] = serde_json::Value::String(u.clone());
                    }
                    if let Some(n) = pr_number {
                        finish_event["pr_number"] = serde_json::json!(n);
                    }
                    push_event(
                        &worker_event_buffer,
                        &worker_subscribers,
                        Some(&disk_events_path),
                        finish_event,
                    );
                    if let Ok(mut subs) = worker_global_subscribers.lock() {
                        let mut fin_evt = serde_json::json!({
                            "type": "run_finished",
                            "id": worker_item_id.clone()
                        });
                        if let Some(u) = &pr_url {
                            fin_evt["pr_url"] = serde_json::Value::String(u.clone());
                        }
                        if let Some(n) = pr_number {
                            fin_evt["pr_number"] = serde_json::json!(n);
                        }
                        subs.retain(|tx| tx.send(fin_evt.clone()).is_ok());
                    }
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
                    // On user cancellation, cancel_run already released the item,
                    // cleaned up any new worktree, recorded the cancel event,
                    // and broadcast run_finished. Avoid duplicating teardown.
                    if worker_cancel.load(Ordering::SeqCst) {
                        if let Some(session_id) = session_id_opt.as_deref() {
                            let _ = worker_client.cancel(session_id);
                        }
                        let _ = worker_client.kill();
                        if let Ok(mut runs) = worker_active_runs.lock() {
                            runs.remove(&worker_item_id);
                        }
                        return;
                    }

                    // Return an item that is still in_progress to its pre-run
                    // status without overwriting a status the user set manually.
                    let _ = crate::workflow::release_item_to_status(
                        &worker_vault_path,
                        &worker_filename,
                        Some(&worker_runner.actor),
                        true,
                        None,
                        pre_run_status,
                    );

                    let mut final_err_msg = err_msg.clone();
                    if worker_is_new_worktree {
                        if WorktreeManager::has_uncommitted_changes(
                            &worker_repo_path,
                            &worker_item_id,
                        ) {
                            let wt_path =
                                WorktreeManager::worktree_path(&worker_repo_path, &worker_item_id);
                            final_err_msg.push_str(&format!(
                                " (preserved worktree with uncommitted changes at {})",
                                wt_path.display()
                            ));
                        } else {
                            let _ = WorktreeManager::remove_worktree(
                                &worker_repo_path,
                                &worker_item_id,
                            );
                        }
                    }

                    // Sends {"type": "error", "message": err.to_string()} to subscribers
                    let finished_at = chrono::Utc::now().timestamp();
                    let error_event = serde_json::json!({
                        "type": "error",
                        "message": final_err_msg.clone(),
                        "startedAt": started_at,
                        "finishedAt": finished_at
                    });
                    push_event(
                        &worker_event_buffer,
                        &worker_subscribers,
                        Some(&disk_events_path),
                        error_event,
                    );
                    if let Ok(mut subs) = worker_global_subscribers.lock() {
                        let fin_evt = serde_json::json!({
                            "type": "run_finished",
                            "id": worker_item_id.clone()
                        });
                        subs.retain(|tx| tx.send(fin_evt.clone()).is_ok());
                    }
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
                key.and_then(|k| runs.remove(&k))
            } else {
                None
            }
        };
        if let Some(run) = run {
            run.cancel_requested.store(true, Ordering::SeqCst);
            if let Ok(sid_guard) = run.session_id.lock() {
                if let Some(sid) = sid_guard.as_deref() {
                    let _ = run.client.cancel(sid);
                }
            }
            let _ = run.client.terminate_gracefully(Duration::from_millis(400));

            let canonical_id = &run.item_id;
            let mut cancel_msg = "Cancelled by user".to_string();
            if run.is_new_worktree && run.worktree_path.is_some() {
                if WorktreeManager::has_uncommitted_changes(&self.repo_path, canonical_id) {
                    if let Some(wt_path) = &run.worktree_path {
                        cancel_msg.push_str(&format!(
                            " (preserved worktree with uncommitted changes at {})",
                            wt_path.display()
                        ));
                    }
                } else {
                    let _ = WorktreeManager::remove_worktree(&self.repo_path, canonical_id);
                }
            }

            let finished_at = chrono::Utc::now().timestamp();
            let cancel_event = serde_json::json!({
                "type": "error",
                "message": cancel_msg.clone(),
                "startedAt": run.started_at,
                "finishedAt": finished_at
            });
            let cancel_disk_path = self
                .vault_path
                .join("runs")
                .join(format!("{canonical_id}.events.jsonl"));
            push_event(
                &run.event_buffer,
                &run.subscribers,
                Some(&cancel_disk_path),
                cancel_event,
            );

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
                    finished_at,
                    stop_reason: Some(cancel_msg),
                });
                if comp.len() > 50 {
                    comp.remove(0);
                }
            }

            if let Ok(filename) = crate::vault::resolve_selector(&self.vault_path, canonical_id) {
                let _ = crate::workflow::release_item_to_status(
                    &self.vault_path,
                    &filename,
                    Some(&run.runner.actor),
                    true,
                    None,
                    run.pre_run_status,
                );
            }

            if let Ok(mut subs) = self.global_subscribers.lock() {
                let cancel_evt = serde_json::json!({
                    "type": "run_finished",
                    "id": canonical_id
                });
                subs.retain(|tx| tx.send(cancel_evt.clone()).is_ok());
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
    prompt_parts.push("\n## Instructions\n1. Inspect git status and changed files in this worktree/repository.\n2. Stage and commit all relevant changes with a clear, conventional commit message referencing the task title and ID.\n3. If a git remote is configured, push the branch to origin.\n4. Create a pull request targeting the main branch (e.g. via `gh pr create` or git) with a concise title and summary of changes.\n5. Babysit the pull request until CI checks are green before claiming to be done:\n   - Monitor CI checks on the pull request (e.g. via `gh pr checks --watch` or `gh pr checks`).\n   - If any CI check fails, inspect the failure logs (e.g. via `gh run view --log-failed`), resolve the root cause, commit the fix, and push the branch.\n   - Repeat until all CI checks are green. Never claim completion while checks are failing or pending.\n6. If `gh` is unavailable, no remote is configured, or no CI checks are configured on the repository, report the commit details and status.\nSummarize what was committed, the pull request status, and the CI check results under '## Summary' and '## Verification'. Do not archive this item; completion and handoff recording are handled automatically.".to_string());

    prompt_parts.join("\n")
}

pub fn build_idea_prompt(
    title: &str,
    item_id: &str,
    kind: &str,
    priority: Option<&str>,
    acceptance_criteria: Option<&str>,
    body: &str,
) -> String {
    let mut prompt_parts = Vec::new();
    prompt_parts.push(format!("# Idea Mode: {title}"));
    prompt_parts.push(format!("- Item ID: {item_id}"));
    prompt_parts.push(format!("- Kind: {kind}"));
    if let Some(p) = priority {
        prompt_parts.push(format!("- Priority: {p}"));
    }
    if let Some(ac) = acceptance_criteria {
        prompt_parts.push(format!("\n## Initial Acceptance Criteria\n{ac}"));
    }
    if !body.trim().is_empty() {
        prompt_parts.push(format!("\n## Description / Idea\n{}", body.trim()));
    }
    prompt_parts.push("\n## Instructions\nYou are in IDEA mode. Your objective is to explore the codebase and collaborate with the user to turn this idea into a clean, comprehensive specification ticket.\n\n1. Explore the codebase:\n   - Use tools (read files, search/grep, inspect directory structure) to understand existing patterns, architecture, dependencies, and relevant files.\n   - Ground your advice in real code evidence.\n\n2. Ask questions & clarify:\n   - Ask the user focused questions about requirements, tradeoffs, edge cases, scope, and interfaces.\n   - Work through ambiguity together until an implementation plan covering all bases from A to Z is established.\n\n3. Create the ticket in the backlog:\n   - When you and the user agree on the plan, use a tool call (such as `pin add` via bash tool) to create the ticket in the backlog (`--type task`, `--kind <kind>`, `--status created`).\n   - Ensure the ticket details contain:\n     * Title\n     * Problem & Context\n     * Implementation Proposal (covering all bases from A to Z)\n     * Acceptance Criteria checklist (`- [ ] ...`)\n   - Announce the created ticket ID and details to the user.".to_string());

    prompt_parts.join("\n")
}

fn run_mode_label(mode: RunMode) -> &'static str {
    match mode {
        RunMode::Implement { .. } => "implement",
        RunMode::CommitPr => "commit_pr",
        RunMode::Idea { .. } => "idea",
    }
}

fn active_run_info_from_run(run: &ActiveRun) -> ActiveRunInfo {
    let live = run.live_state.lock().map(|s| s.clone()).unwrap_or_default();
    ActiveRunInfo {
        item_id: run.item_id.clone(),
        mode: run_mode_label(run.mode).to_string(),
        interactive: run.prompt_sender.is_some(),
        waiting_for_user: live.waiting_for_user,
        waiting_since: live.waiting_since,
        last_message: live.last_message,
        last_activity: live.last_activity,
        is_primary_worktree: run.is_primary_worktree,
        worktree_path: run.worktree_path.as_ref().map(|p| p.display().to_string()),
        started_at: run.started_at,
        actor: run.runner.actor.clone(),
        model: run.runner.model.clone(),
    }
}

fn truncate_utf8(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

fn record_live_event(live_state: &Arc<Mutex<RunLiveState>>, event: &serde_json::Value) {
    let Some(params) = event.get("params") else {
        return;
    };
    let update = params.get("update").unwrap_or(params);
    if is_assistant_message(update) {
        let mut chunks = Vec::new();
        extract_text_from_value(update, &mut chunks);
        if !chunks.is_empty() {
            if let Ok(mut live) = live_state.lock() {
                const MAX_TURN_BYTES: usize = 16 * 1024;
                const MAX_FULL_ASSISTANT_BYTES: usize = 256 * 1024;
                for chunk in chunks {
                    if live.current_turn_message.len() < MAX_TURN_BYTES {
                        let remaining = MAX_TURN_BYTES - live.current_turn_message.len();
                        if chunk.len() <= remaining {
                            live.current_turn_message.push_str(&chunk);
                        } else {
                            live.current_turn_message
                                .push_str(&truncate_utf8(&chunk, remaining));
                        }
                    }
                    if live.full_assistant_text.len() < MAX_FULL_ASSISTANT_BYTES {
                        let remaining = MAX_FULL_ASSISTANT_BYTES - live.full_assistant_text.len();
                        if chunk.len() <= remaining {
                            live.full_assistant_text.push_str(&chunk);
                        } else {
                            live.full_assistant_text
                                .push_str(&truncate_utf8(&chunk, remaining));
                        }
                    }
                }
            }
        }
        return;
    }

    let kind = update
        .get("sessionUpdate")
        .or_else(|| update.get("type"))
        .and_then(|v| v.as_str());
    if matches!(kind, Some("tool_call") | Some("tool_call_update")) {
        let label = update
            .get("title")
            .or_else(|| update.get("name"))
            .or_else(|| update.get("tool"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty());
        if let Some(tool_label) = label {
            if let Ok(mut live) = live_state.lock() {
                live.last_activity = Some(truncate_utf8(tool_label, 120));
            }
        }
    }
}

fn extract_last_turn_message(events: &[serde_json::Value]) -> Option<String> {
    let start_idx = events
        .iter()
        .rposition(|e| e.get("type").and_then(|v| v.as_str()) == Some("user_message"))
        .map(|i| i + 1)
        .unwrap_or(0);
    let mut chunks = Vec::new();
    for event in &events[start_idx..] {
        let Some(params) = event.get("params") else {
            continue;
        };
        let update = params.get("update").unwrap_or(params);
        if is_assistant_message(update) {
            extract_text_from_value(update, &mut chunks);
        }
    }
    let combined = chunks.concat();
    let trimmed = combined.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(truncate_utf8(trimmed, 4000))
    }
}

#[allow(clippy::too_many_arguments)]
fn enter_waiting_state(
    live_state: &Arc<Mutex<RunLiveState>>,
    event_buffer: &Arc<Mutex<Vec<serde_json::Value>>>,
    subscribers: &Arc<Mutex<Vec<Sender<serde_json::Value>>>>,
    global_subscribers: &Arc<Mutex<Vec<Sender<serde_json::Value>>>>,
    disk_path: Option<&Path>,
    item_id: &str,
    item_title: &str,
    mode_str: &str,
    stop_reason: &str,
) {
    // Allow trailing stdout chunks already queued in the forwarding thread to land.
    thread::sleep(Duration::from_millis(25));
    let waiting_since = chrono::Utc::now().timestamp();
    let buffer_fallback = event_buffer
        .lock()
        .ok()
        .and_then(|buf| extract_last_turn_message(&buf));
    let last_message = if let Ok(mut live) = live_state.lock() {
        let from_turn = live.current_turn_message.trim().to_string();
        let captured = if !from_turn.is_empty() {
            Some(truncate_utf8(&from_turn, 4000))
        } else {
            buffer_fallback
        };
        live.waiting_for_user = true;
        live.waiting_since = Some(waiting_since);
        live.last_message = captured;
        live.last_activity = Some("Waiting for reply".to_string());
        live.last_message.clone()
    } else {
        buffer_fallback
    };

    let mut waiting_event = serde_json::json!({
        "type": "waiting_for_user",
        "stopReason": stop_reason,
        "waitingSince": waiting_since,
    });
    if let Some(msg) = &last_message {
        waiting_event["message"] = serde_json::Value::String(msg.clone());
    }
    push_event(event_buffer, subscribers, disk_path, waiting_event);

    if let Ok(mut subs) = global_subscribers.lock() {
        let mut evt = serde_json::json!({
            "type": "run_waiting",
            "id": item_id,
            "title": item_title,
            "mode": mode_str,
            "waiting_since": waiting_since,
        });
        if let Some(msg) = &last_message {
            evt["message"] = serde_json::Value::String(msg.clone());
        }
        subs.retain(|tx| tx.send(evt.clone()).is_ok());
    }
}

fn clear_waiting_state(
    live_state: &Arc<Mutex<RunLiveState>>,
    global_subscribers: &Arc<Mutex<Vec<Sender<serde_json::Value>>>>,
    item_id: &str,
    mode_str: &str,
    activity: &str,
) {
    if let Ok(mut live) = live_state.lock() {
        live.waiting_for_user = false;
        live.waiting_since = None;
        live.current_turn_message.clear();
        live.last_activity = Some(activity.to_string());
    }
    if let Ok(mut subs) = global_subscribers.lock() {
        let evt = serde_json::json!({
            "type": "run_resumed",
            "id": item_id,
            "mode": mode_str,
        });
        subs.retain(|tx| tx.send(evt.clone()).is_ok());
    }
}

fn resolve_run_key(runs: &HashMap<String, ActiveRun>, id: &str) -> Option<String> {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return None;
    }
    if runs.contains_key(trimmed) {
        return Some(trimmed.to_string());
    }
    let mut matched: Option<&String> = None;
    for key in runs.keys() {
        if key.starts_with(trimmed) || trimmed.starts_with(key.as_str()) {
            if matched.is_some() {
                return None;
            }
            matched = Some(key);
        }
    }
    matched.cloned()
}
fn write_value_events_as_sse<W: std::io::Write>(
    events: &[serde_json::Value],
    writer: &mut W,
) -> std::io::Result<(usize, bool)> {
    let mut has_finished = false;
    for event in events {
        if let Some(t) = event.get("type").and_then(|v| v.as_str()) {
            if t == "finished" || t == "error" {
                has_finished = true;
            }
        }
        let payload = serde_json::to_string(event).unwrap_or_default();
        writer.write_all(b"data: ")?;
        writer.write_all(payload.as_bytes())?;
        writer.write_all(b"\n\n")?;
    }
    Ok((events.len(), has_finished))
}

fn read_latest_timing_from_transcript(path: &Path) -> Option<(i64, Option<i64>)> {
    use std::io::{BufRead, Read, Seek, SeekFrom};

    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();

    // Fast path: terminal events carrying startedAt/finishedAt sit at the end
    // of the append-only transcript, so inspect a 64 KiB tail window first.
    const TAIL_BYTES: u64 = 64 * 1024;
    let start = len.saturating_sub(TAIL_BYTES);
    if file.seek(SeekFrom::Start(start)).is_ok() {
        let mut tail = Vec::new();
        if file.read_to_end(&mut tail).is_ok() {
            let text = String::from_utf8_lossy(&tail);
            let lines: Vec<&str> = text.lines().collect();
            let skip_first = usize::from(start > 0 && lines.len() > 1);
            for line in lines[skip_first..].iter().rev() {
                if !line.contains("\"startedAt\"") {
                    continue;
                }
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
                    if let Some(st) = val.get("startedAt").and_then(|v| v.as_i64()) {
                        let ft = val.get("finishedAt").and_then(|v| v.as_i64());
                        return Some((st, ft));
                    }
                }
            }
        }
    }

    if start == 0 {
        return None;
    }

    // Fallback when no timing line appeared in the tail window: stream forward
    // and only parse lines mentioning startedAt or finishedAt.
    file.seek(SeekFrom::Start(0)).ok()?;
    let reader = std::io::BufReader::new(file);
    let mut latest_started: Option<i64> = None;
    let mut latest_finished: Option<i64> = None;
    for line in reader.lines().map_while(Result::ok) {
        if !line.contains("\"startedAt\"") && !line.contains("\"finishedAt\"") {
            continue;
        }
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) {
            if let Some(st) = val.get("startedAt").and_then(|v| v.as_i64()) {
                latest_started = Some(st);
                latest_finished = val.get("finishedAt").and_then(|v| v.as_i64());
            } else if let Some(ft) = val.get("finishedAt").and_then(|v| v.as_i64()) {
                latest_finished = Some(ft);
            }
        }
    }
    latest_started.map(|st| (st, latest_finished))
}

fn push_event(
    event_buffer: &Arc<Mutex<Vec<serde_json::Value>>>,
    subscribers: &Arc<Mutex<Vec<Sender<serde_json::Value>>>>,
    disk_path: Option<&Path>,
    event: serde_json::Value,
) {
    push_event_with_writer(event_buffer, subscribers, None, disk_path, event);
}

fn push_event_with_writer(
    event_buffer: &Arc<Mutex<Vec<serde_json::Value>>>,
    subscribers: &Arc<Mutex<Vec<Sender<serde_json::Value>>>>,
    disk_writer: Option<&mut BufWriter<fs::File>>,
    disk_path: Option<&Path>,
    event: serde_json::Value,
) {
    if let Ok(mut buf) = event_buffer.lock() {
        if buf.len() >= 2000 {
            buf.remove(0);
        }
        buf.push(event.clone());
    }
    if let Ok(mut subs) = subscribers.lock() {
        subs.retain(|sub| sub.send(event.clone()).is_ok());
    }
    if let Some(writer) = disk_writer {
        if let Ok(line) = serde_json::to_string(&event) {
            let _ = writeln!(writer, "{line}");
            let _ = writer.flush();
        }
    } else if let Some(path) = disk_path {
        if let Ok(line) = serde_json::to_string(&event) {
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
            if !s.is_empty() {
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
                    break;
                }
            }
        }
        _ => {}
    }
}

fn clean_header_prefix(text: &str, prefixes: &[&str]) -> String {
    let mut trimmed = text.trim();
    let lower = trimmed.to_ascii_lowercase();
    for prefix in prefixes {
        if lower.starts_with(prefix) {
            trimmed = trimmed[prefix.len()..].trim();
            break;
        }
    }
    trimmed.to_string()
}

fn collect_all_text_from_value(val: &serde_json::Value, out: &mut Vec<String>) {
    match val {
        serde_json::Value::String(s) => {
            if !s.is_empty() {
                out.push(s.clone());
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                collect_all_text_from_value(item, out);
            }
        }
        serde_json::Value::Object(map) => {
            for v in map.values() {
                collect_all_text_from_value(v, out);
            }
        }
        _ => {}
    }
}

pub fn extract_pr_info(events: &[serde_json::Value]) -> (Option<String>, Option<u64>) {
    let mut texts = Vec::new();
    for event in events {
        collect_all_text_from_value(event, &mut texts);
    }
    let combined = texts.join("\n");
    crate::workflow::extract_pr_from_text(&combined)
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

    let full_text = text_chunks.concat();
    extract_summary_and_evidence_from_text(&full_text)
}

fn extract_summary_and_evidence_from_text(full_text: &str) -> (String, String) {
    let full_text_trimmed = full_text.trim();

    if full_text_trimmed.is_empty() {
        return (DEFAULT_SUMMARY.to_string(), DEFAULT_EVIDENCE.to_string());
    }

    let lower = full_text.to_ascii_lowercase();

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
    let lower = body.to_ascii_lowercase();
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
    fn test_extract_summary_and_evidence_streamed_chunks() {
        let events = vec![
            assistant_message("## Sum"),
            assistant_message("mary\n"),
            assistant_message("1. Imple"),
            assistant_message("mented feature xyz in `src/app.js`.\n\n"),
            assistant_message("## Verification\n"),
            assistant_message("- cargo test: "),
            assistant_message("30 passed.\n"),
            assistant_message("- bash tests/cli.sh: "),
            assistant_message("passed."),
        ];
        let (summary, evidence) = extract_summary_and_evidence(&events);
        assert_eq!(summary, "1. Implemented feature xyz in `src/app.js`.");
        assert_eq!(
            evidence,
            "- cargo test: 30 passed.\n- bash tests/cli.sh: passed."
        );
    }

    #[test]
    fn test_extract_pr_info() {
        let events = vec![
            serde_json::json!({
                "params": {
                    "update": {
                        "sessionUpdate": "tool_call",
                        "title": "$ gh pr create --title 'feat' --body 'body'",
                        "rawOutput": "https://github.com/JoeriKaiser/pin/pull/150\n"
                    }
                }
            }),
            assistant_message(
                "Opened pull request https://github.com/JoeriKaiser/pin/pull/150 for this work.",
            ),
        ];
        let (url, num) = extract_pr_info(&events);
        assert_eq!(
            url.as_deref(),
            Some("https://github.com/JoeriKaiser/pin/pull/150")
        );
        assert_eq!(num, Some(150));
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
        assert!(prompt.contains("Babysit the pull request until CI checks are green"));
        assert!(prompt.contains("gh pr checks"));
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

    #[test]
    fn test_build_idea_prompt() {
        let prompt = build_idea_prompt(
            "Explore new auth system",
            "auth123",
            "technical",
            Some("high"),
            Some("- should be secure"),
            "We need auth but not sure between JWT or session cookies.",
        );
        assert!(prompt.contains("# Idea Mode: Explore new auth system"));
        assert!(prompt.contains("- Item ID: auth123"));
        assert!(prompt.contains("- Priority: high"));
        assert!(prompt.contains("## Initial Acceptance Criteria\n- should be secure"));
        assert!(prompt.contains("We need auth but not sure between JWT or session cookies."));
        assert!(prompt.contains("Explore the codebase"));
        assert!(prompt.contains("Ask questions & clarify"));
        assert!(prompt.contains("Create the ticket in the backlog"));
        assert!(prompt.contains("pin add"));
    }

    #[test]
    fn test_send_message_rejects_non_running_item() {
        let vault_dir = tempfile::tempdir().unwrap();
        let repo_dir = tempfile::tempdir().unwrap();
        let mgr = AcpManager::new(
            vault_dir.path().to_path_buf(),
            repo_dir.path().to_path_buf(),
            None,
        );
        let res = mgr.send_message("nonexistent", "hello");
        assert!(matches!(res, Err(AcpManagerError::ItemNotFound(_))));
        let empty_res = mgr.send_message("nonexistent", "   \n\t  ");
        assert!(matches!(empty_res, Err(AcpManagerError::Workflow(_))));
    }

    #[test]
    fn test_start_idea_run_rejects_nonexistent_item() {
        let vault_dir = tempfile::tempdir().unwrap();
        let repo_dir = tempfile::tempdir().unwrap();
        let mgr = AcpManager::new(
            vault_dir.path().to_path_buf(),
            repo_dir.path().to_path_buf(),
            None,
        );
        let res = mgr.start_idea_run("nonexistent", false);
        assert!(matches!(res, Err(AcpManagerError::ItemNotFound(_))));
    }
    #[test]
    fn test_active_runs_info() {
        let vault_dir = tempfile::tempdir().unwrap();
        let repo_dir = tempfile::tempdir().unwrap();
        let mgr = AcpManager::new(
            vault_dir.path().to_path_buf(),
            repo_dir.path().to_path_buf(),
            None,
        );

        assert!(mgr.active_runs_info().is_empty());

        let log_path = vault_dir.path().join("test.log");
        let client = Arc::new(AcpClient::spawn("cat", repo_dir.path(), &log_path).unwrap());
        let runner = RunnerSettings {
            command: "mock acp".to_string(),
            actor: "agent:test".to_string(),
            model: Some("mock/fast".to_string()),
        };
        let (tx, _rx) = mpsc::channel();
        let live1 = Arc::new(Mutex::new(RunLiveState {
            waiting_for_user: true,
            waiting_since: Some(250),
            current_turn_message: String::new(),
            full_assistant_text: String::new(),
            last_message: Some("Which option do you prefer?".to_string()),
            last_activity: Some("Waiting for reply".to_string()),
        }));
        let run1 = ActiveRun {
            item_id: "item-b".to_string(),
            client: Arc::clone(&client),
            session_id: Arc::new(Mutex::new(None)),
            pre_run_status: crate::model::Status::Planned,
            worktree_path: Some(PathBuf::from("/tmp/wt-b")),
            is_new_worktree: false,
            is_primary_worktree: false,
            event_buffer: Arc::new(Mutex::new(Vec::new())),
            subscribers: Arc::new(Mutex::new(Vec::new())),
            started_at: 200,
            cancel_requested: Arc::new(AtomicBool::new(false)),
            prompt_sender: Some(tx),
            mode: RunMode::Idea { use_worktree: true },
            runner: runner.clone(),
            live_state: live1,
        };
        let run2 = ActiveRun {
            item_id: "item-a".to_string(),
            client: Arc::clone(&client),
            session_id: Arc::new(Mutex::new(None)),
            pre_run_status: crate::model::Status::Done,
            worktree_path: None,
            is_new_worktree: false,
            is_primary_worktree: true,
            event_buffer: Arc::new(Mutex::new(Vec::new())),
            subscribers: Arc::new(Mutex::new(Vec::new())),
            started_at: 100,
            cancel_requested: Arc::new(AtomicBool::new(false)),
            prompt_sender: None,
            mode: RunMode::CommitPr,
            runner,
            live_state: Arc::new(Mutex::new(RunLiveState::default())),
        };

        {
            let mut runs = mgr.active_runs.lock().unwrap();
            runs.insert("item-b".to_string(), run1);
            runs.insert("item-a".to_string(), run2);
        }

        let info = mgr.active_runs_info();
        assert_eq!(info.len(), 2);
        // Verified sorted by item_id
        assert_eq!(info[0].item_id, "item-a");
        assert_eq!(info[0].mode, "commit_pr");
        assert!(!info[0].interactive);
        assert!(!info[0].waiting_for_user);
        assert_eq!(info[0].waiting_since, None);
        assert_eq!(info[0].last_message, None);
        assert!(info[0].is_primary_worktree);
        assert_eq!(info[0].worktree_path, None);
        assert_eq!(info[0].started_at, 100);
        assert_eq!(info[0].actor, "agent:test");
        assert_eq!(info[0].model.as_deref(), Some("mock/fast"));

        assert_eq!(info[1].item_id, "item-b");
        assert_eq!(info[1].mode, "idea");
        assert!(info[1].interactive);
        assert!(info[1].waiting_for_user);
        assert_eq!(info[1].waiting_since, Some(250));
        assert_eq!(
            info[1].last_message.as_deref(),
            Some("Which option do you prefer?")
        );
        assert_eq!(info[1].last_activity.as_deref(), Some("Waiting for reply"));
        assert!(!info[1].is_primary_worktree);
        assert_eq!(info[1].worktree_path.as_deref(), Some("/tmp/wt-b"));
        assert_eq!(info[1].started_at, 200);
        assert_eq!(info[1].actor, "agent:test");
        assert_eq!(info[1].model.as_deref(), Some("mock/fast"));
    }

    #[test]
    fn test_interactive_run_broadcasts_waiting_and_resumed() {
        let vault_dir = tempfile::tempdir().unwrap();
        let repo_dir = tempfile::tempdir().unwrap();
        let item_id = "0123456789ab";
        let item_body = "---\nschema: 2\nid: \"0123456789ab\"\ntitle: \"Interactive Spec\"\ntype: \"idea\"\nstatus: \"created\"\n---\n# Interactive Spec\n";
        fs::write(vault_dir.path().join(format!("{item_id}.md")), item_body).unwrap();

        let script_path = repo_dir.path().join("mock_interactive_acp.sh");
        let script = r###"#!/bin/sh
turn=0
while IFS= read -r line; do
  req_id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  case "$line" in
    *initialize*)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":1,"agentCapabilities":{}}}\n' "$req_id"
      ;;
    *session/new*)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"sessionId":"sess-int-1"}}\n' "$req_id"
      ;;
    *session/prompt*)
      turn=$((turn + 1))
      if [ "$turn" -eq 1 ]; then
        printf '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"sess-int-1","update":{"sessionUpdate":"agent_thought_chunk","content":{"type":"text","text":"internal reasoning"}}}}\n'
        printf '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"sess-int-1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"1. Should we use SSE?\\n2. Should we add sound?"}}}}\n'
      else
        printf '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"sess-int-1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"## Summary\\nAgreed on SSE and sound.\\n\\n## Verification\\nConfirmed with user."}}}}\n'
      fi
      printf '{"jsonrpc":"2.0","id":%s,"result":{"stopReason":"end_turn"}}\n' "$req_id"
      ;;
    *session/cancel*)
      exit 0
      ;;
  esac
done
"###;
        fs::write(&script_path, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755)).unwrap();
        }

        let mgr = AcpManager::new(
            vault_dir.path().to_path_buf(),
            repo_dir.path().to_path_buf(),
            Some(script_path.to_string_lossy().to_string()),
        );
        let (global_tx, global_rx) = mpsc::channel();
        mgr.add_global_listener(global_tx);

        mgr.start_idea_run(item_id, false).unwrap();

        let mut saw_waiting = None;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            if let Ok(evt) = global_rx.recv_timeout(Duration::from_millis(200)) {
                if evt.get("type").and_then(|v| v.as_str()) == Some("run_waiting") {
                    saw_waiting = Some(evt);
                    break;
                }
            }
        }
        let waiting_evt = saw_waiting.expect("expected run_waiting global event");
        assert_eq!(waiting_evt["id"], item_id);
        assert_eq!(waiting_evt["title"], "Interactive Spec");
        assert_eq!(waiting_evt["mode"], "idea");
        assert!(waiting_evt["message"]
            .as_str()
            .unwrap()
            .contains("1. Should we use SSE?"));
        assert!(!waiting_evt["message"]
            .as_str()
            .unwrap()
            .contains("internal reasoning"));

        let info = mgr
            .get_active_run_info(item_id)
            .expect("active run info present");
        assert!(info.interactive);
        assert!(info.waiting_for_user);
        assert!(info.waiting_since.is_some());
        assert_eq!(info.last_activity.as_deref(), Some("Waiting for reply"));

        // Late subscriber receives authoritative run_state frame
        let mut sse_out = Vec::new();
        let (_count, _finished, _sub) = mgr
            .stream_events_and_subscribe(item_id, &mut sse_out)
            .unwrap();
        let sse_text = String::from_utf8(sse_out).unwrap();
        assert!(sse_text.contains("\"type\":\"run_state\""));
        assert!(sse_text.contains("\"waiting_for_user\":true"));

        // Send user reply -> expect run_resumed then second run_waiting
        mgr.send_message(item_id, "1. Yes\n2. Yes").unwrap();

        let mut saw_resumed = false;
        let mut saw_second_waiting = false;
        let deadline2 = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline2 && !(saw_resumed && saw_second_waiting) {
            if let Ok(evt) = global_rx.recv_timeout(Duration::from_millis(200)) {
                match evt.get("type").and_then(|v| v.as_str()) {
                    Some("run_resumed") => saw_resumed = true,
                    Some("run_waiting") => saw_second_waiting = true,
                    _ => {}
                }
            }
        }
        assert!(saw_resumed, "expected run_resumed global event");
        assert!(saw_second_waiting, "expected second run_waiting event");

        mgr.finish_run(item_id).unwrap();
        let mut saw_finished = false;
        let deadline3 = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline3 {
            if let Ok(evt) = global_rx.recv_timeout(Duration::from_millis(200)) {
                if evt.get("type").and_then(|v| v.as_str()) == Some("run_finished") {
                    saw_finished = true;
                    break;
                }
            }
        }
        assert!(saw_finished, "expected run_finished global event");
    }

    #[test]
    fn test_interactive_implement_run_pauses_until_finished() {
        let vault_dir = tempfile::tempdir().unwrap();
        let repo_dir = tempfile::tempdir().unwrap();
        let item_id = "0123456789ac";
        let item_body = "---\nschema: 2\nid: \"0123456789ac\"\ntitle: \"Interactive Task\"\ntype: \"task\"\nstatus: \"planned\"\n---\n# Interactive Task\n";
        fs::write(vault_dir.path().join(format!("{item_id}.md")), item_body).unwrap();

        let script_path = repo_dir.path().join("mock_impl_interactive.sh");
        let script = r###"#!/bin/sh
while IFS= read -r line; do
  req_id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  case "$line" in
    *initialize*)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":1,"agentCapabilities":{}}}\n' "$req_id"
      ;;
    *session/new*)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"sessionId":"sess-impl-1"}}\n' "$req_id"
      ;;
    *session/prompt*)
      printf '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"sess-impl-1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Need your confirmation before editing."}}}}\n'
      printf '{"jsonrpc":"2.0","id":%s,"result":{"stopReason":"end_turn"}}\n' "$req_id"
      ;;
    *session/cancel*)
      exit 0
      ;;
  esac
done
"###;
        fs::write(&script_path, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755)).unwrap();
        }

        let mgr = AcpManager::new(
            vault_dir.path().to_path_buf(),
            repo_dir.path().to_path_buf(),
            Some(script_path.to_string_lossy().to_string()),
        );
        let (global_tx, global_rx) = mpsc::channel();
        mgr.add_global_listener(global_tx);

        mgr.start_run_interactive(item_id, false, true).unwrap();

        let mut saw_waiting = false;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            if let Ok(evt) = global_rx.recv_timeout(Duration::from_millis(200)) {
                if evt.get("type").and_then(|v| v.as_str()) == Some("run_waiting") {
                    assert_eq!(evt["mode"], "implement");
                    saw_waiting = true;
                    break;
                }
            }
        }
        assert!(
            saw_waiting,
            "interactive implement run should pause with run_waiting"
        );
        let info = mgr.get_active_run_info(item_id).unwrap();
        assert_eq!(info.mode, "implement");
        assert!(info.interactive);
        assert!(info.waiting_for_user);

        mgr.finish_run(item_id).unwrap();
        let mut saw_finished = false;
        let deadline2 = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline2 {
            if let Ok(evt) = global_rx.recv_timeout(Duration::from_millis(200)) {
                if evt.get("type").and_then(|v| v.as_str()) == Some("run_finished") {
                    saw_finished = true;
                    break;
                }
            }
        }
        assert!(saw_finished);
    }

    #[test]
    fn test_runner_settings_resolve_prefers_explicit_values() {
        let settings = RunnerSettings::resolve(
            Some(" custom acp ".to_string()),
            Some(" agent:custom ".to_string()),
            Some(" model/fast ".to_string()),
        );
        assert_eq!(settings.command, "custom acp");
        assert_eq!(settings.actor, "agent:custom");
        assert_eq!(settings.model.as_deref(), Some("model/fast"));

        let blank = RunnerSettings::resolve(
            Some("   ".to_string()),
            Some("".to_string()),
            Some("  ".to_string()),
        );
        // Blank explicit values fall back to environment/default rather than
        // producing an unusable empty runner.
        assert!(!blank.command.trim().is_empty());
        assert!(!blank.actor.trim().is_empty());
        let env_model = std::env::var("PIN_ACP_MODEL")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        assert_eq!(blank.model, env_model);
    }

    #[test]
    fn test_update_runner_settings_partial_and_clear() {
        let vault_dir = tempfile::tempdir().unwrap();
        let repo_dir = tempfile::tempdir().unwrap();
        let mgr = AcpManager::with_runner(
            vault_dir.path().to_path_buf(),
            repo_dir.path().to_path_buf(),
            RunnerSettings {
                command: "omp acp".to_string(),
                actor: "agent:omp".to_string(),
                model: Some("model/one".to_string()),
            },
        );

        let updated = mgr
            .update_runner_settings(
                Some("opencode acp".to_string()),
                Some("agent:opencode".to_string()),
                Some(Some("model/two".to_string())),
            )
            .unwrap();
        assert_eq!(updated.command, "opencode acp");
        assert_eq!(updated.actor, "agent:opencode");
        assert_eq!(updated.model.as_deref(), Some("model/two"));

        let partial = mgr.update_runner_settings(None, None, Some(None)).unwrap();
        assert_eq!(partial.command, "opencode acp");
        assert_eq!(partial.actor, "agent:opencode");
        assert_eq!(partial.model, None);
        assert_eq!(mgr.runner_settings(), partial);

        assert!(mgr
            .update_runner_settings(Some("  ".to_string()), None, None)
            .is_err());
        assert!(mgr
            .update_runner_settings(None, Some("".to_string()), None)
            .is_err());
    }

    #[test]
    fn test_extract_handles_multibyte_case_expanding_utf8() {
        // Turkish capital I with dot (\u{0130}) expands from 2 bytes to 3 bytes
        // under Unicode to_lowercase(), which previously panicked when slicing.
        let body = "\u{0130}stanbul note\n\n## Acceptance Criteria\n- [ ] Works cleanly\n";
        let criteria = extract_acceptance_criteria_from_body(body).unwrap();
        assert_eq!(criteria, "- [ ] Works cleanly");

        let events = vec![serde_json::json!({
            "params": {
                "update": {
                    "sessionUpdate": "agent_message_chunk",
                    "content": {
                        "type": "text",
                        "text": "\u{0130}\u{0130}\n## Summary\nFixed encoding.\n## Verification\ncargo test passed."
                    }
                }
            }
        })];
        let (summary, evidence) = extract_summary_and_evidence(&events);
        assert_eq!(summary, "Fixed encoding.");
        assert_eq!(evidence, "cargo test passed.");
    }

    #[test]
    fn test_stream_events_and_tail_timing_from_append_only_transcript() {
        let vault_dir = tempfile::tempdir().unwrap();
        let repo_dir = tempfile::tempdir().unwrap();
        let runs_dir = vault_dir.path().join("runs");
        fs::create_dir_all(&runs_dir).unwrap();

        let item_id = "stream012345";
        let transcript = runs_dir.join(format!("{item_id}.events.jsonl"));
        let mut content = String::new();
        // Earlier run followed by >70 KiB of streamed chunks, then a newer run's terminal event
        content.push_str("{\"type\":\"finished\",\"startedAt\":1000,\"finishedAt\":1050}\n");
        for i in 0..800 {
            content.push_str(&format!(
                "{{\"type\":\"session/update\",\"chunk\":{i},\"pad\":\"{}\"}}\n",
                "x".repeat(90)
            ));
        }
        content.push_str("{\"type\":\"finished\",\"startedAt\":2000,\"finishedAt\":2120}\n");
        fs::write(&transcript, &content).unwrap();

        let mgr = AcpManager::new(
            vault_dir.path().to_path_buf(),
            repo_dir.path().to_path_buf(),
            None,
        );

        // Tail reader must return the latest run's timestamps rather than mixing run 1 and run 2
        assert_eq!(mgr.get_run_timing(item_id), Some((2000, Some(2120))));

        // Streaming replay emits every JSONL event as an SSE data frame without modifying the file
        let mut out = Vec::new();
        let (count, has_finished, sub) =
            mgr.stream_events_and_subscribe(item_id, &mut out).unwrap();
        assert_eq!(count, 802);
        assert!(has_finished);
        assert!(sub.is_none());
        let sse = String::from_utf8(out).unwrap();
        assert!(sse
            .contains("data: {\"type\":\"finished\",\"startedAt\":2000,\"finishedAt\":2120}\n\n"));
    }

    #[test]
    fn test_cancel_commit_pr_preserves_existing_worktree_and_restores_done() {
        let vault_dir = tempfile::tempdir().unwrap();
        let repo_dir = tempfile::tempdir().unwrap();
        let item_id = "cancel012345";
        let item_file = vault_dir.path().join(format!("{item_id}.md"));
        fs::write(
            &item_file,
            format!(
                "---\nschema: 2\nid: \"{item_id}\"\nproject: \"pin\"\nkind: \"technical\"\ntype: \"task\"\nstatus: \"in_progress\"\nclaimed_by: \"agent:test\"\ntimestamp: 1700000000\ntitle: \"Cancel Commit PR\"\n---\n# Cancel Commit PR\n"
            ),
        )
        .unwrap();

        let existing_wt = repo_dir.path().join(".pin_worktrees").join(item_id);
        fs::create_dir_all(&existing_wt).unwrap();
        let sentinel = existing_wt.join("uncommitted.txt");
        fs::write(&sentinel, "important work").unwrap();

        let mgr = AcpManager::new(
            vault_dir.path().to_path_buf(),
            repo_dir.path().to_path_buf(),
            None,
        );
        let log_path = vault_dir.path().join("cancel.log");
        let client = Arc::new(AcpClient::spawn("sleep 60", repo_dir.path(), &log_path).unwrap());
        let run = ActiveRun {
            item_id: item_id.to_string(),
            client,
            session_id: Arc::new(Mutex::new(None)),
            pre_run_status: crate::model::Status::Done,
            worktree_path: Some(existing_wt.clone()),
            is_new_worktree: false,
            is_primary_worktree: false,
            event_buffer: Arc::new(Mutex::new(Vec::new())),
            subscribers: Arc::new(Mutex::new(Vec::new())),
            started_at: 1700000000,
            cancel_requested: Arc::new(AtomicBool::new(false)),
            prompt_sender: None,
            mode: RunMode::CommitPr,
            runner: RunnerSettings {
                command: "sleep 60".to_string(),
                actor: "agent:test".to_string(),
                model: None,
            },
            live_state: Arc::new(Mutex::new(RunLiveState::default())),
        };
        mgr.active_runs
            .lock()
            .unwrap()
            .insert(item_id.to_string(), run);

        mgr.cancel_run(item_id).unwrap();

        assert!(
            sentinel.exists(),
            "cancelling CommitPr must not delete pre-existing worktree"
        );
        let content = fs::read_to_string(&item_file).unwrap();
        let mut issues = Vec::new();
        let meta = crate::frontmatter::parse_front_matter_detailed(
            &format!("{item_id}.md"),
            &content,
            &mut issues,
        )
        .unwrap();
        assert_eq!(meta.current_status(), crate::model::Status::Done);
        assert_eq!(meta.claimed_by, None);
    }

    #[test]
    fn test_resolve_run_key_rejects_empty_and_ambiguous_prefixes() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("key.log");
        let client = Arc::new(AcpClient::spawn("sleep 5", dir.path(), &log_path).unwrap());
        let make_run = |id: &str| ActiveRun {
            item_id: id.to_string(),
            client: Arc::clone(&client),
            session_id: Arc::new(Mutex::new(None)),
            pre_run_status: crate::model::Status::Done,
            worktree_path: None,
            is_new_worktree: false,
            is_primary_worktree: true,
            event_buffer: Arc::new(Mutex::new(Vec::new())),
            subscribers: Arc::new(Mutex::new(Vec::new())),
            started_at: 100,
            cancel_requested: Arc::new(AtomicBool::new(false)),
            prompt_sender: None,
            mode: RunMode::CommitPr,
            runner: RunnerSettings {
                command: "sleep 5".to_string(),
                actor: "agent:test".to_string(),
                model: None,
            },
            live_state: Arc::new(Mutex::new(RunLiveState::default())),
        };
        let mut runs = HashMap::new();
        runs.insert("abc111111111".to_string(), make_run("abc111111111"));
        runs.insert("abc222222222".to_string(), make_run("abc222222222"));

        assert_eq!(resolve_run_key(&runs, ""), None);
        assert_eq!(resolve_run_key(&runs, "   "), None);
        assert_eq!(resolve_run_key(&runs, "abc"), None);
        assert_eq!(
            resolve_run_key(&runs, "abc1"),
            Some("abc111111111".to_string())
        );
        assert_eq!(
            resolve_run_key(&runs, "abc222222222"),
            Some("abc222222222".to_string())
        );
        let _ = client.kill();
    }

    #[test]
    fn test_cancel_restores_pre_run_created_status_and_preserves_dirty_new_worktree() {
        let vault_dir = tempfile::tempdir().unwrap();
        let repo_dir = tempfile::tempdir().unwrap();
        let item_id = "cc1122334455";
        let item_file = vault_dir.path().join(format!("{item_id}.md"));
        fs::write(
            &item_file,
            format!(
                "---\nschema: 2\nid: \"{item_id}\"\nproject: \"pin\"\nkind: \"technical\"\ntype: \"idea\"\ntimestamp: 1700000000\ncreated_at_ns: 1700000000000000000\ntitle: \"Explore idea\"\nstatus: \"in_progress\"\nclaimed_by: \"agent:test\"\nrevision: 2\n---\n\n# Explore idea\n"
            ),
        )
        .unwrap();

        // Initialize a real git repo and provision a worktree with an uncommitted file.
        let repo = repo_dir.path();
        std::process::Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["init", "-b", "main"])
            .output()
            .unwrap();
        std::process::Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["config", "user.name", "Test"])
            .output()
            .unwrap();
        std::process::Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["config", "user.email", "test@test.com"])
            .output()
            .unwrap();
        fs::write(repo.join("README.md"), "base").unwrap();
        std::process::Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["add", "."])
            .output()
            .unwrap();
        std::process::Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["commit", "-m", "init"])
            .output()
            .unwrap();

        let wt = WorktreeManager::provision_worktree(repo, item_id).unwrap();
        let dirty_file = wt.join("draft.txt");
        fs::write(&dirty_file, "uncommitted agent work").unwrap();

        let mgr = AcpManager::new(vault_dir.path().to_path_buf(), repo.to_path_buf(), None);
        let log_path = vault_dir.path().join("cancel_dirty.log");
        let client = Arc::new(AcpClient::spawn("sleep 60", repo, &log_path).unwrap());
        let run = ActiveRun {
            item_id: item_id.to_string(),
            client,
            session_id: Arc::new(Mutex::new(None)),
            pre_run_status: crate::model::Status::Created,
            worktree_path: Some(wt.clone()),
            is_new_worktree: true,
            is_primary_worktree: false,
            event_buffer: Arc::new(Mutex::new(Vec::new())),
            subscribers: Arc::new(Mutex::new(Vec::new())),
            started_at: 1700000000,
            cancel_requested: Arc::new(AtomicBool::new(false)),
            prompt_sender: None,
            mode: RunMode::Idea { use_worktree: true },
            runner: RunnerSettings {
                command: "sleep 60".to_string(),
                actor: "agent:test".to_string(),
                model: None,
            },
            live_state: Arc::new(Mutex::new(RunLiveState::default())),
        };
        mgr.active_runs
            .lock()
            .unwrap()
            .insert(item_id.to_string(), run);

        mgr.cancel_run(item_id).unwrap();

        assert!(
            dirty_file.exists(),
            "dirty new worktree must be preserved on cancel"
        );
        let content = fs::read_to_string(&item_file).unwrap();
        let mut issues = Vec::new();
        let meta = crate::frontmatter::parse_front_matter_detailed(
            &format!("{item_id}.md"),
            &content,
            &mut issues,
        )
        .unwrap();
        assert_eq!(meta.current_status(), crate::model::Status::Created);
        assert_eq!(meta.claimed_by, None);
    }
}
