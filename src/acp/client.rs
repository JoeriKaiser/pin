use std::collections::{HashMap, VecDeque};
use std::fs::{create_dir_all, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::acp::protocol::{
    ClientInfo, ContentBlock, InitializeParams, InitializeResult, JsonRpcError,
    JsonRpcNotification, JsonRpcRequest, JsonRpcResponse, RequestPermissionOutcome,
    RequestPermissionParams, RequestPermissionResult, SessionCancelParams, SessionNewParams,
    SessionNewResult, SessionPromptParams, SessionPromptResult, SessionSetConfigOptionParams,
};
use crate::cli::split_shell_words;

#[derive(Debug)]
pub enum AcpError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Timeout(String),
    Protocol(String),
    ChildExited(Option<String>),
}

impl std::fmt::Display for AcpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AcpError::Io(e) => write!(f, "I/O error: {e}"),
            AcpError::Json(e) => write!(f, "JSON error: {e}"),
            AcpError::Timeout(msg) => write!(f, "Timeout: {msg}"),
            AcpError::Protocol(msg) => write!(f, "Protocol error: {msg}"),
            AcpError::ChildExited(Some(detail)) => {
                write!(f, "Child process exited unexpectedly ({detail})")
            }
            AcpError::ChildExited(None) => write!(f, "Child process exited unexpectedly"),
        }
    }
}

impl std::error::Error for AcpError {}

impl From<std::io::Error> for AcpError {
    fn from(e: std::io::Error) -> Self {
        AcpError::Io(e)
    }
}

impl From<serde_json::Error> for AcpError {
    fn from(e: serde_json::Error) -> Self {
        AcpError::Json(e)
    }
}

pub struct AcpClient {
    child: Arc<Mutex<Option<Child>>>,
    stdin_writer: Arc<Mutex<ChildStdin>>,
    next_id: AtomicU64,
    response_dispatch: Arc<Mutex<HashMap<u64, Sender<serde_json::Value>>>>,
    subscribers: Arc<Mutex<Vec<Sender<serde_json::Value>>>>,
    recent_stderr: Arc<Mutex<VecDeque<String>>>,
    timeout: Duration,
}

const MAX_RECENT_STDERR_LINES: usize = 8;

impl AcpClient {
    pub fn spawn(cmd_line: &str, cwd: &Path, log_path: &Path) -> Result<Self, AcpError> {
        let words = split_shell_words(cmd_line);
        if words.is_empty() {
            return Err(AcpError::Protocol(
                "Empty command line provided".to_string(),
            ));
        }

        // Create the log parent directory before spawning so a filesystem error
        // never leaks a live child process.
        if let Some(parent) = log_path.parent() {
            create_dir_all(parent)?;
        }

        let mut cmd = Command::new(&words[0]);
        if words.len() > 1 {
            cmd.args(&words[1..]);
        }
        cmd.current_dir(cwd);
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }

        let mut child = cmd.spawn()?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| AcpError::Protocol("Failed to open child stdin".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AcpError::Protocol("Failed to open child stdout".to_string()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| AcpError::Protocol("Failed to open child stderr".to_string()))?;

        let recent_stderr: Arc<Mutex<VecDeque<String>>> =
            Arc::new(Mutex::new(VecDeque::with_capacity(MAX_RECENT_STDERR_LINES)));

        // Spawn stderr logging thread (read raw bytes so invalid UTF-8 never kills the loop)
        let stderr_log_path = log_path.to_path_buf();
        let stderr_tail = Arc::clone(&recent_stderr);
        thread::spawn(move || {
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&stderr_log_path)
                .ok();

            let mut reader = BufReader::new(stderr);
            let mut buf = Vec::new();
            loop {
                buf.clear();
                match reader.read_until(b'\n', &mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        let line = String::from_utf8_lossy(&buf);
                        let trimmed = line.trim_end_matches(['\r', '\n']);
                        if let Some(ref mut f) = file {
                            let _ = writeln!(f, "{trimmed}");
                            let _ = f.flush();
                        }
                        if !trimmed.is_empty() {
                            if let Ok(mut tail) = stderr_tail.lock() {
                                if tail.len() >= MAX_RECENT_STDERR_LINES {
                                    tail.pop_front();
                                }
                                tail.push_back(trimmed.to_string());
                            }
                        }
                    }
                }
            }
        });

        let stdin_writer = Arc::new(Mutex::new(stdin));
        let response_dispatch: Arc<Mutex<HashMap<u64, Sender<serde_json::Value>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let subscribers: Arc<Mutex<Vec<Sender<serde_json::Value>>>> =
            Arc::new(Mutex::new(Vec::new()));

        let reader_dispatch = Arc::clone(&response_dispatch);
        let reader_subscribers = Arc::clone(&subscribers);
        let reader_stdin = Arc::clone(&stdin_writer);

        // Spawn stdout reader thread (read raw bytes so non-UTF-8 output never kills the loop)
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut buf = Vec::new();
            loop {
                buf.clear();
                match reader.read_until(b'\n', &mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
                let line = String::from_utf8_lossy(&buf);
                if line.trim().is_empty() {
                    continue;
                }
                let value: serde_json::Value = match serde_json::from_str(&line) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                // Check if it's an incoming request or notification from the agent
                let method = value.get("method").and_then(|m| m.as_str());
                let id_val = value.get("id").filter(|id| !id.is_null());

                if let Some(method) = method {
                    if let Some(req_id) = id_val {
                        if method == "session/request_permission" {
                            let params_val = value
                                .get("params")
                                .cloned()
                                .unwrap_or(serde_json::Value::Null);
                            let perm_params: Result<RequestPermissionParams, _> =
                                serde_json::from_value(params_val);

                            let selected_id = if let Ok(params) = perm_params {
                                params
                                    .options
                                    .iter()
                                    .find(|o| o.kind == "allow_once")
                                    .or_else(|| {
                                        params.options.iter().find(|o| o.kind == "allow_always")
                                    })
                                    .or_else(|| {
                                        params.options.iter().find(|o| o.kind.starts_with("allow"))
                                    })
                                    .or_else(|| params.options.first())
                                    .map(|o| o.option_id.clone())
                                    .unwrap_or_else(|| "allow".to_string())
                            } else {
                                "allow".to_string()
                            };

                            let resp = JsonRpcResponse::ok(
                                req_id.clone(),
                                RequestPermissionResult {
                                    outcome: RequestPermissionOutcome {
                                        outcome: "selected".to_string(),
                                        option_id: selected_id,
                                    },
                                },
                            );

                            if let Ok(serialized) = serde_json::to_string(&resp) {
                                if let Ok(mut writer) = reader_stdin.lock() {
                                    let _ = writeln!(writer, "{serialized}");
                                    let _ = writer.flush();
                                }
                            }
                        } else {
                            // Reply with JSON-RPC Method not found so an agent calling an
                            // unsupported client method does not deadlock waiting for a response.
                            let resp = JsonRpcResponse::<serde_json::Value>::err(
                                req_id.clone(),
                                JsonRpcError {
                                    code: -32601,
                                    message: format!("Method not found: {method}"),
                                    data: None,
                                },
                            );
                            if let Ok(serialized) = serde_json::to_string(&resp) {
                                if let Ok(mut writer) = reader_stdin.lock() {
                                    let _ = writeln!(writer, "{serialized}");
                                    let _ = writer.flush();
                                }
                            }
                        }
                        continue;
                    }

                    // Agent sending notification (e.g., session/update)
                    let mut subs = reader_subscribers.lock().unwrap();
                    subs.retain(|sub| sub.send(value.clone()).is_ok());
                    continue;
                }

                // If response with id: dispatch to waiting caller
                if let Some(resp_id) = id_val.and_then(|id| id.as_u64()) {
                    let mut dispatch = reader_dispatch.lock().unwrap();
                    if let Some(sender) = dispatch.remove(&resp_id) {
                        let _ = sender.send(value);
                    }
                }
            }
            if let Ok(mut dispatch) = reader_dispatch.lock() {
                dispatch.clear();
            }
        });

        Ok(Self {
            child: Arc::new(Mutex::new(Some(child))),
            stdin_writer,
            next_id: AtomicU64::new(1),
            response_dispatch,
            subscribers,
            recent_stderr,
            timeout: Duration::from_secs(30),
        })
    }

    fn exit_diagnostics(&self) -> Option<String> {
        let mut parts = Vec::new();
        if let Ok(mut guard) = self.child.lock() {
            if let Some(ref mut child) = *guard {
                if let Ok(Some(status)) = child.try_wait() {
                    parts.push(format!("status: {status}"));
                }
            }
        }
        // Give stderr thread a brief moment to flush final error lines when child exits.
        thread::sleep(Duration::from_millis(20));
        if let Ok(tail) = self.recent_stderr.lock() {
            if !tail.is_empty() {
                let joined = tail.iter().cloned().collect::<Vec<_>>().join(" | ");
                parts.push(format!("stderr: {joined}"));
            }
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join("; "))
        }
    }

    fn write_json(&self, json_str: &str) -> Result<(), AcpError> {
        let mut writer = self
            .stdin_writer
            .lock()
            .map_err(|_| AcpError::Protocol("Lock poisoned on stdin".to_string()))?;
        writeln!(writer, "{json_str}")?;
        writer.flush()?;
        Ok(())
    }

    fn send_request<P: serde::Serialize, R: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: P,
    ) -> Result<R, AcpError> {
        self.send_request_timeout(method, params, Some(self.timeout))
    }

    fn send_request_timeout<P: serde::Serialize, R: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: P,
        timeout: Option<Duration>,
    ) -> Result<R, AcpError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let req = JsonRpcRequest::new(id, method, params);
        let serialized = serde_json::to_string(&req)?;

        let (tx, rx) = mpsc::channel();
        {
            let mut dispatch = self
                .response_dispatch
                .lock()
                .map_err(|_| AcpError::Protocol("Lock poisoned on dispatch".to_string()))?;
            dispatch.insert(id, tx);
        }

        if let Err(e) = self.write_json(&serialized) {
            let mut dispatch = self
                .response_dispatch
                .lock()
                .map_err(|_| AcpError::Protocol("Lock poisoned on dispatch".to_string()))?;
            dispatch.remove(&id);
            if matches!(&e, AcpError::Io(io_err) if io_err.kind() == std::io::ErrorKind::BrokenPipe)
            {
                return Err(AcpError::ChildExited(self.exit_diagnostics()));
            }
            return Err(e);
        }

        let resp_val =
            match timeout {
                Some(t) => match rx.recv_timeout(t) {
                    Ok(v) => v,
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        let mut dispatch = self.response_dispatch.lock().map_err(|_| {
                            AcpError::Protocol("Lock poisoned on dispatch".to_string())
                        })?;
                        dispatch.remove(&id);
                        return Err(AcpError::Timeout(format!(
                            "Timeout waiting for response to {method}"
                        )));
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        let mut dispatch = self.response_dispatch.lock().map_err(|_| {
                            AcpError::Protocol("Lock poisoned on dispatch".to_string())
                        })?;
                        dispatch.remove(&id);
                        return Err(AcpError::ChildExited(self.exit_diagnostics()));
                    }
                },
                None => match rx.recv() {
                    Ok(v) => v,
                    Err(_) => {
                        let mut dispatch = self.response_dispatch.lock().map_err(|_| {
                            AcpError::Protocol("Lock poisoned on dispatch".to_string())
                        })?;
                        dispatch.remove(&id);
                        return Err(AcpError::ChildExited(self.exit_diagnostics()));
                    }
                },
            };

        if let Some(err_val) = resp_val.get("error") {
            if !err_val.is_null() {
                return Err(AcpError::Protocol(format!("RPC error response: {err_val}")));
            }
        }

        let result_val = resp_val
            .get("result")
            .ok_or_else(|| AcpError::Protocol("Missing result field in response".to_string()))?;

        serde_json::from_value(result_val.clone()).map_err(AcpError::Json)
    }

    pub fn initialize(&self) -> Result<InitializeResult, AcpError> {
        let params = InitializeParams {
            protocol_version: 1,
            client_capabilities: serde_json::json!({}),
            client_info: ClientInfo {
                name: "pin".to_string(),
                title: Some("Pin Idea Registry".to_string()),
                version: env!("PIN_VERSION").to_string(),
            },
        };

        self.send_request("initialize", params)
    }

    pub fn new_session(&self, cwd: &Path) -> Result<String, AcpError> {
        self.new_session_with_config(cwd)
            .map(|result| result.session_id)
    }

    /// Creates a session and returns the agent's advertised config options,
    /// which is where model choices come from.
    pub fn new_session_with_config(&self, cwd: &Path) -> Result<SessionNewResult, AcpError> {
        let params = SessionNewParams {
            cwd: cwd.to_string_lossy().to_string(),
            mcp_servers: vec![],
        };

        self.send_request("session/new", params)
    }

    /// Applies a session config option such as the model. Must be called after
    /// `session/new` and before the first prompt.
    pub fn set_config_option(
        &self,
        session_id: &str,
        config_id: &str,
        value: serde_json::Value,
    ) -> Result<(), AcpError> {
        let params = SessionSetConfigOptionParams {
            session_id: session_id.to_string(),
            config_id: config_id.to_string(),
            value,
        };

        let _: serde_json::Value = self.send_request("session/set_config_option", params)?;
        Ok(())
    }

    pub fn prompt(&self, session_id: &str, text: &str) -> Result<SessionPromptResult, AcpError> {
        let params = SessionPromptParams {
            session_id: session_id.to_string(),
            prompt: vec![ContentBlock::text(text)],
        };

        let prompt_timeout = match std::env::var("PIN_ACP_TIMEOUT") {
            Ok(val) => match val.trim().parse::<u64>() {
                Ok(0) => None,
                Ok(secs) => Some(Duration::from_secs(secs)),
                Err(_) => None,
            },
            Err(_) => None,
        };

        self.send_request_timeout("session/prompt", params, prompt_timeout)
    }

    pub fn cancel(&self, session_id: &str) -> Result<(), AcpError> {
        let params = SessionCancelParams {
            session_id: session_id.to_string(),
        };

        let notif = JsonRpcNotification::new("session/cancel", params);
        let serialized = serde_json::to_string(&notif)?;
        self.write_json(&serialized)
    }

    /// Sends SIGTERM on Unix and waits up to `grace` for the child to exit cleanly
    /// before falling back to `kill()` (SIGKILL).
    pub fn terminate_gracefully(&self, grace: Duration) -> Result<(), AcpError> {
        #[cfg(unix)]
        {
            extern "C" {
                fn kill(pid: i32, sig: i32) -> i32;
            }
            let pgid = {
                let guard = self
                    .child
                    .lock()
                    .map_err(|_| AcpError::Protocol("Lock poisoned on child".to_string()))?;
                guard
                    .as_ref()
                    .and_then(|c| i32::try_from(c.id()).ok())
                    .filter(|&id| id > 0)
            };
            if let Some(pgid) = pgid {
                unsafe {
                    kill(-pgid, 15);
                }
                let deadline = Instant::now() + grace;
                while Instant::now() < deadline {
                    let exited = {
                        let mut guard = self.child.lock().map_err(|_| {
                            AcpError::Protocol("Lock poisoned on child".to_string())
                        })?;
                        match guard.as_mut() {
                            Some(child) => matches!(child.try_wait(), Ok(Some(_))),
                            None => true,
                        }
                    };
                    if exited {
                        break;
                    }
                    thread::sleep(Duration::from_millis(25));
                }
            }
        }
        #[cfg(not(unix))]
        let _ = grace;
        self.kill()
    }

    pub fn kill(&self) -> Result<(), AcpError> {
        let mut guard = self
            .child
            .lock()
            .map_err(|_| AcpError::Protocol("Lock poisoned on child".to_string()))?;
        if let Some(mut child) = guard.take() {
            #[cfg(unix)]
            {
                extern "C" {
                    fn kill(pid: i32, sig: i32) -> i32;
                }
                if let Ok(pgid) = i32::try_from(child.id()) {
                    if pgid > 0 {
                        // Signal the whole process group started via process_group(0)
                        // so child test runners and tools do not outlive a cancelled run.
                        unsafe {
                            kill(-pgid, 9);
                        }
                    }
                }
            }
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Ok(mut dispatch) = self.response_dispatch.lock() {
            dispatch.clear();
        }
        Ok(())
    }

    pub fn subscribe(&self) -> Receiver<serde_json::Value> {
        let (tx, rx) = mpsc::channel();
        let mut subs = self.subscribers.lock().unwrap();
        subs.push(tx);
        rx
    }
}

impl Drop for AcpClient {
    fn drop(&mut self) {
        let _ = self.kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_acp_client_empty_command() {
        let dir = tempdir().unwrap();
        let log_path = dir.path().join("test.log");
        let res = AcpClient::spawn("", dir.path(), &log_path);
        assert!(matches!(res, Err(AcpError::Protocol(_))));
    }

    #[test]
    fn test_acp_client_stdio_handshake() {
        let dir = tempdir().unwrap();
        let log_path = dir.path().join("agent.log");
        let script_path = dir.path().join("mock_agent.sh");

        let script = r#"#!/bin/sh
echo "agent stderr log" >&2
while IFS= read -r line; do
  req_id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  case "$line" in
    *initialize*)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":1,"agentCapabilities":{}}}\n' "$req_id"
      ;;
    *session/new*)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"sessionId":"mock-session-42","configOptions":[{"id":"model","name":"Model","category":"model","type":"select","currentValue":"mock/default","options":[{"value":"mock/default","name":"Mock Default"},{"value":"mock/fast","name":"Mock Fast"}]}]}}\n' "$req_id"
      ;;
    *session/set_config_option*)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"configOptions":[{"id":"model","name":"Model","category":"model","type":"select","currentValue":"mock/fast","options":[{"value":"mock/default","name":"Mock Default"},{"value":"mock/fast","name":"Mock Fast"}]}]}}\n' "$req_id"
      ;;
    *session/prompt*)
      printf '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"mock-session-42","update":{"text":"all done"}}}\n'
      printf '{"jsonrpc":"2.0","id":%s,"result":{"stopReason":"end_turn"}}\n' "$req_id"
      ;;
    *session/cancel*)
      exit 0
      ;;
  esac
done
"#;
        std::fs::write(&script_path, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let cmd = script_path.to_string_lossy().to_string();
        let client = AcpClient::spawn(&cmd, dir.path(), &log_path).unwrap();
        let rx = client.subscribe();

        let init_res = client.initialize().unwrap();
        assert_eq!(init_res.protocol_version, 1);

        let session = client.new_session_with_config(dir.path()).unwrap();
        assert_eq!(session.session_id, "mock-session-42");
        let model = session
            .config_options
            .iter()
            .find(|option| option.category.as_deref() == Some("model"))
            .unwrap();
        assert_eq!(model.current_value, Some(serde_json::json!("mock/default")));
        assert_eq!(model.options.len(), 2);

        client
            .set_config_option(&session.session_id, "model", serde_json::json!("mock/fast"))
            .unwrap();

        let prompt_res = client.prompt(&session.session_id, "test prompt").unwrap();
        assert_eq!(prompt_res.stop_reason.as_deref(), Some("end_turn"));

        let received = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(
            received.get("method").and_then(|m| m.as_str()),
            Some("session/update")
        );

        client.cancel(&session.session_id).unwrap();

        thread::sleep(Duration::from_millis(100));
        let log_content = std::fs::read_to_string(&log_path).unwrap_or_default();
        assert!(log_content.contains("agent stderr log"));
    }

    #[test]
    fn test_acp_client_handles_string_ids_unhandled_methods_and_non_utf8() {
        let dir = tempdir().unwrap();
        let log_path = dir.path().join("nested").join("agent.log");
        let script_path = dir.path().join("mock_agent_edge.sh");

        let script = r#"#!/bin/sh
# Emit non-UTF-8 bytes on stdout and stderr before JSON-RPC traffic
printf '\xff\xfe non-utf8 stdout\n'
printf '\xff\xfe non-utf8 stderr\n' >&2
while IFS= read -r line; do
  req_id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  case "$line" in
    *initialize*)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":1,"agentCapabilities":{}}}\n' "$req_id"
      ;;
    *session/new*)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"sessionId":"s-edge","configOptions":[]}}\n' "$req_id"
      ;;
    *session/prompt*)
      # Send permission request with a STRING id and reject_once before allow_always
      printf '{"jsonrpc":"2.0","id":"perm-str-1","method":"session/request_permission","params":{"sessionId":"s-edge","options":[{"optionId":"rej-1","name":"Reject","kind":"reject_once"},{"optionId":"all-1","name":"Allow always","kind":"allow_always"}]}}\n'
      IFS= read -r perm_resp
      case "$perm_resp" in
        *'"id":"perm-str-1"'*'"optionId":"all-1"'*) ;;
        *) exit 2 ;;
      esac
      # Send an unsupported client method request with a string id
      printf '{"jsonrpc":"2.0","id":"fs-req-1","method":"fs/read_text_file","params":{"path":"foo"}}\n'
      IFS= read -r fs_resp
      case "$fs_resp" in
        *'"id":"fs-req-1"'*'-32601'*) ;;
        *) exit 3 ;;
      esac
      printf '{"jsonrpc":"2.0","id":%s,"result":{"stopReason":"end_turn"}}\n' "$req_id"
      ;;
  esac
done
"#;
        std::fs::write(&script_path, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let cmd = format!("sh {}", script_path.to_string_lossy());
        let client = AcpClient::spawn(&cmd, dir.path(), &log_path).unwrap();
        assert_eq!(client.initialize().unwrap().protocol_version, 1);
        let session_id = client.new_session(dir.path()).unwrap();
        let prompt_res = client.prompt(&session_id, "go").unwrap();
        assert_eq!(prompt_res.stop_reason.as_deref(), Some("end_turn"));
    }

    #[test]
    fn test_acp_client_child_exited_includes_stderr_diagnostics() {
        let dir = tempdir().unwrap();
        let log_path = dir.path().join("crash.log");
        let script_path = dir.path().join("crash_agent.sh");
        std::fs::write(
            &script_path,
            "#!/bin/sh\necho 'missing API key' >&2\nexit 1\n",
        )
        .unwrap();

        let cmd = format!("sh {}", script_path.to_string_lossy());
        let client = AcpClient::spawn(&cmd, dir.path(), &log_path).unwrap();
        let err = client.initialize().unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("missing API key"),
            "expected stderr in error, got: {msg}"
        );
    }
}
