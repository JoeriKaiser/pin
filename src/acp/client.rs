use std::collections::HashMap;
use std::fs::{create_dir_all, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::acp::protocol::{
    ClientInfo, ContentBlock, InitializeParams, InitializeResult, JsonRpcRequest, JsonRpcResponse,
    RequestPermissionOutcome, RequestPermissionParams, RequestPermissionResult,
    SessionCancelParams, SessionNewParams, SessionNewResult, SessionPromptParams,
    SessionPromptResult,
};
use crate::cli::split_shell_words;

#[derive(Debug)]
pub enum AcpError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Timeout(String),
    Protocol(String),
    ChildExited,
}

impl std::fmt::Display for AcpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AcpError::Io(e) => write!(f, "I/O error: {e}"),
            AcpError::Json(e) => write!(f, "JSON error: {e}"),
            AcpError::Timeout(msg) => write!(f, "Timeout: {msg}"),
            AcpError::Protocol(msg) => write!(f, "Protocol error: {msg}"),
            AcpError::ChildExited => write!(f, "Child process exited unexpectedly"),
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
    timeout: Duration,
}

impl AcpClient {
    pub fn spawn(cmd_line: &str, cwd: &Path, log_path: &Path) -> Result<Self, AcpError> {
        let words = split_shell_words(cmd_line);
        if words.is_empty() {
            return Err(AcpError::Protocol(
                "Empty command line provided".to_string(),
            ));
        }

        let mut cmd = Command::new(&words[0]);
        if words.len() > 1 {
            cmd.args(&words[1..]);
        }
        cmd.current_dir(cwd);
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

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

        // Ensure parent directories exist for log file
        if let Some(parent) = log_path.parent() {
            create_dir_all(parent)?;
        }

        // Spawn stderr logging thread
        let stderr_log_path = log_path.to_path_buf();
        thread::spawn(move || {
            let mut file = match OpenOptions::new()
                .create(true)
                .append(true)
                .open(&stderr_log_path)
            {
                Ok(f) => f,
                Err(_) => return,
            };

            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                let _ = writeln!(file, "{line}");
                let _ = file.flush();
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

        // Spawn stdout reader thread
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                if line.trim().is_empty() {
                    continue;
                }
                let value: serde_json::Value = match serde_json::from_str(&line) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                // Check if it's an incoming request or notification from the agent
                let method = value.get("method").and_then(|m| m.as_str());
                let id_val = value.get("id");

                if let Some(method) = method {
                    // Agent calling session/request_permission
                    if method == "session/request_permission" {
                        if let Some(req_id) = id_val.and_then(|id| id.as_u64()) {
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
                                    .or_else(|| params.options.first())
                                    .map(|o| o.option_id.clone())
                                    .unwrap_or_else(|| "allow".to_string())
                            } else {
                                "allow".to_string()
                            };

                            let resp = JsonRpcResponse::ok(
                                req_id,
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
                        }
                        continue;
                    }

                    // Agent sending notification (e.g., session/update)
                    if id_val.is_none() {
                        let mut subs = reader_subscribers.lock().unwrap();
                        subs.retain(|sub| sub.send(value.clone()).is_ok());
                        continue;
                    }
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
            timeout: Duration::from_secs(30),
        })
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
                        return Err(AcpError::ChildExited);
                    }
                },
                None => match rx.recv() {
                    Ok(v) => v,
                    Err(_) => {
                        let mut dispatch = self.response_dispatch.lock().map_err(|_| {
                            AcpError::Protocol("Lock poisoned on dispatch".to_string())
                        })?;
                        dispatch.remove(&id);
                        return Err(AcpError::ChildExited);
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
            client_capabilities: serde_json::json!({
                "fs": {
                    "read": true,
                    "write": true
                }
            }),
            client_info: ClientInfo {
                name: "pin".to_string(),
                title: Some("Pin Idea Registry".to_string()),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
        };

        self.send_request("initialize", params)
    }

    pub fn new_session(&self, cwd: &Path) -> Result<String, AcpError> {
        let params = SessionNewParams {
            cwd: cwd.to_string_lossy().to_string(),
            mcp_servers: vec![],
        };

        let res: SessionNewResult = self.send_request("session/new", params)?;
        Ok(res.session_id)
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

        let req = JsonRpcRequest::new(
            self.next_id.fetch_add(1, Ordering::SeqCst),
            "session/cancel",
            params,
        );
        let serialized = serde_json::to_string(&req)?;
        self.write_json(&serialized)
    }

    pub fn kill(&self) -> Result<(), AcpError> {
        let mut guard = self
            .child
            .lock()
            .map_err(|_| AcpError::Protocol("Lock poisoned on child".to_string()))?;
        if let Some(mut child) = guard.take() {
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
  case "$line" in
    *initialize*)
      echo '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentCapabilities":{}}}'
      ;;
    *session/new*)
      echo '{"jsonrpc":"2.0","id":2,"result":{"sessionId":"mock-session-42"}}'
      ;;
    *session/prompt*)
      echo '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"mock-session-42","update":{"text":"all done"}}}'
      echo '{"jsonrpc":"2.0","id":3,"result":{"stopReason":"end_turn"}}'
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

        let sess_id = client.new_session(dir.path()).unwrap();
        assert_eq!(sess_id, "mock-session-42");

        let prompt_res = client.prompt(&sess_id, "test prompt").unwrap();
        assert_eq!(prompt_res.stop_reason.as_deref(), Some("end_turn"));

        let received = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(
            received.get("method").and_then(|m| m.as_str()),
            Some("session/update")
        );

        client.cancel(&sess_id).unwrap();

        thread::sleep(Duration::from_millis(100));
        let log_content = std::fs::read_to_string(&log_path).unwrap_or_default();
        assert!(log_content.contains("agent stderr log"));
    }
}
