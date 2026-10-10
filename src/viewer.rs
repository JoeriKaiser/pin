use crate::acp::AcpManagerError;
use crate::assets::*;
use crate::model::{ArchiveFilter, IdeaMeta, Kind, OutputFormat, Priority, Status, WorkType};
use crate::output::JsonIdeaOutput;
use crate::vault::{collect_ideas_with_filter, generate_token, resolve_selector, FilterOptions};
use crate::workflow;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::env;
use std::io::{self, BufRead, BufReader, BufWriter, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
const VIEW_CSP: &str = "default-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'; img-src 'self' data:; style-src 'self'; script-src 'self'; connect-src 'self'; sandbox allow-scripts allow-same-origin allow-forms";

/// Bodies are small JSON payloads. Anything larger is a mistake or a stuck
/// client, and must not be turned into an allocation.
const MAX_REQUEST_BODY: usize = 1024 * 1024;
const MAX_SCREENSHOT_BODY: usize = 10 * 1024 * 1024;
/// A client that connects and then stalls would otherwise hold its thread
/// forever, since the viewer is meant to stay up for hours.
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Serialize)]
struct SnapshotData<'a> {
    scope: &'a str,
    archive_filter: &'a str,
    captured_at: String,
    filters: SnapshotFilters,
    items: Vec<JsonIdeaOutput<'a>>,
    count: usize,
}

/// The CLI filters the server applied, so the board can say what it is showing
/// instead of looking like a vault that happens to be missing items.
#[derive(Serialize, Default)]
struct SnapshotFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    item_type: Option<String>,
}

pub struct ViewSnapshot {
    pub token: String,
    pub vault_path: PathBuf,
    pub scope_label: String,
    pub archive_filter: ArchiveFilter,
    pub filter_status: Option<Status>,
    pub filter_tag: Option<String>,
    pub filter_kind: Option<Kind>,
    pub filter_type: Option<WorkType>,
    pub acp_command: Option<String>,
    pub acp_manager: Arc<crate::acp::AcpManager>,
}

#[derive(Deserialize)]
struct ActionPayload {
    action: String,
    #[serde(default, alias = "confirm_worktree", alias = "worktree")]
    pub use_worktree: Option<bool>,
    #[serde(default)]
    to: Option<Status>,
    #[serde(default)]
    actor: Option<String>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    evidence: Option<String>,
    #[serde(default)]
    progress: Option<String>,
    #[serde(default)]
    next: Option<String>,
    #[serde(default)]
    blocker: Option<String>,
    #[serde(default)]
    verification: Option<String>,
    #[serde(default)]
    lease: Option<u64>,
    #[serde(default)]
    force: Option<bool>,
    #[serde(default)]
    expect_revision: Option<u64>,
}

#[derive(Deserialize)]
struct RunPayload {
    #[serde(default, alias = "confirm_worktree", alias = "worktree")]
    pub use_worktree: Option<bool>,
    #[serde(default)]
    pub mode: Option<String>,
}
#[derive(Deserialize)]
struct CreateItemPayload {
    title: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(rename = "type", default)]
    item_type: Option<WorkType>,
    #[serde(default)]
    kind: Option<Kind>,
    #[serde(default)]
    status: Option<Status>,
    #[serde(default)]
    priority: Option<Priority>,
    #[serde(default)]
    project: Option<String>,
    #[serde(default)]
    tags: Option<String>,
    #[serde(default)]
    actor: Option<String>,
}

pub fn create_snapshot(
    _ideas: &[IdeaMeta],
    vault_path: PathBuf,
    scope_label: &str,
    archive_filter: ArchiveFilter,
    filter_status: Option<Status>,
    filter_tag: Option<String>,
    filter_kind: Option<Kind>,
    filter_type: Option<WorkType>,
) -> ViewSnapshot {
    let token = generate_token();
    let repo_path = std::env::current_dir()
        .unwrap_or_else(|_| vault_path.parent().unwrap_or(&vault_path).to_path_buf());
    let acp_manager = Arc::new(crate::acp::AcpManager::new(
        vault_path.clone(),
        repo_path,
        None,
    ));
    ViewSnapshot {
        token,
        vault_path,
        scope_label: scope_label.to_string(),
        archive_filter,
        filter_status,
        filter_tag,
        filter_kind,
        filter_type,
        acp_command: None,
        acp_manager,
    }
}
/// Reverts the status change that opened a run which then failed to start.
///
/// The previous status comes from the mutation's own activity event, so a
/// `created -> in_progress` attempt lands back in `created`.
fn rollback_failed_start(snapshot: &ViewSnapshot, filename: &str, updated_meta: &IdeaMeta) {
    let Some(previous) = updated_meta.activity.last().and_then(|event| event.from) else {
        return;
    };
    if previous == updated_meta.current_status() {
        return;
    }
    let _ = workflow::transition_item(
        &snapshot.vault_path,
        filename,
        previous,
        Some("agent:omp"),
        Some("Agent run failed to start"),
        None,
    );
}

/// Maps a failed run launch to an HTTP status and JSON body the client can act
/// on. `primary_busy` is the one the worktree modal listens for.
fn start_failure_response(err: &AcpManagerError) -> (u16, &'static str, String) {
    match err {
        AcpManagerError::PrimaryBusy(active_id) => (
            409,
            "Conflict",
            serde_json::json!({
                "error": "Primary checkout is busy",
                "status": "primary_busy",
                "active_id": active_id,
            })
            .to_string(),
        ),
        AcpManagerError::AlreadyRunning(active_id) => (
            409,
            "Conflict",
            serde_json::json!({
                "error": err.to_string(),
                "status": "already_running",
                "active_id": active_id,
            })
            .to_string(),
        ),
        AcpManagerError::Workflow(_) => (
            400,
            "Bad Request",
            serde_json::json!({ "error": err.to_string() }).to_string(),
        ),
        AcpManagerError::ItemNotFound(_) => (
            404,
            "Not Found",
            serde_json::json!({ "error": err.to_string() }).to_string(),
        ),
        _ => (
            500,
            "Internal Server Error",
            serde_json::json!({ "error": err.to_string() }).to_string(),
        ),
    }
}

fn compute_vault_etag(vault_path: &Path) -> String {
    let mut count: u64 = 0;
    let mut max_mtime_secs: u64 = 0;
    let mut max_mtime_nanos: u32 = 0;
    let mut total_size: u64 = 0;

    if let Ok(entries) = std::fs::read_dir(vault_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("md") {
                let filename = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                if filename.starts_with('.') {
                    continue;
                }
                count += 1;
                if let Ok(meta) = entry.metadata() {
                    total_size = total_size.wrapping_add(meta.len());
                    if let Ok(modified) = meta.modified() {
                        if let Ok(dur) = modified.duration_since(std::time::UNIX_EPOCH) {
                            let s = dur.as_secs();
                            let n = dur.subsec_nanos();
                            if s > max_mtime_secs || (s == max_mtime_secs && n > max_mtime_nanos) {
                                max_mtime_secs = s;
                                max_mtime_nanos = n;
                            }
                        }
                    }
                }
            }
        }
    }

    format!("{count:x}-{max_mtime_secs:x}-{max_mtime_nanos:x}-{total_size:x}")
}

fn etags_match(client_header: &str, server_etag: &str) -> bool {
    let client = client_header.trim();
    if client == "*" {
        return true;
    }
    let s = server_etag.trim().trim_matches('"');
    for item in client.split(',') {
        let item = item
            .trim()
            .strip_prefix("W/")
            .unwrap_or(item.trim())
            .trim_matches('"');
        if !item.is_empty() && item == s {
            return true;
        }
    }
    false
}

fn send_not_modified(stream: &mut TcpStream, etag: &str) {
    let clean_etag = etag.trim().trim_matches('"');
    let header_str =
        format!("HTTP/1.1 304 Not Modified\r\nETag: \"{clean_etag}\"\r\nConnection: close\r\n\r\n");
    let _ = stream.write_all(header_str.as_bytes());
}

fn send_response(
    stream: &mut TcpStream,
    status_code: u16,
    status_text: &str,
    content_type: &str,
    body: &[u8],
    extra_security: bool,
    send_body: bool,
    etag: Option<&str>,
) {
    let mut header_str = format!(
        "HTTP/1.1 {status_code} {status_text}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    if let Some(e) = etag {
        let clean_etag = e.trim().trim_matches('"');
        header_str.push_str(&format!("ETag: \"{clean_etag}\"\r\n"));
    }
    if extra_security {
        header_str.push_str(&format!(
            "Content-Security-Policy: {VIEW_CSP}\r\nX-Content-Type-Options: nosniff\r\nX-Frame-Options: DENY\r\nCache-Control: no-store\r\n"
        ));
    }
    header_str.push_str("\r\n");

    let _ = stream.write_all(header_str.as_bytes());
    if send_body {
        let _ = stream.write_all(body);
    }
}

fn handle_client(mut stream: TcpStream, snapshot: &Arc<ViewSnapshot>, port: u16) {
    let _ = stream.set_read_timeout(Some(CLIENT_TIMEOUT));
    let _ = stream.set_write_timeout(Some(CLIENT_TIMEOUT));

    let mut reader = BufReader::new(&stream);
    let mut request_line = String::new();

    if reader.read_line(&mut request_line).is_err() || request_line.is_empty() {
        return;
    }

    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 2 {
        return;
    }

    let method = parts[0];
    let raw_path = parts[1];

    // Read headers
    let mut headers = Vec::new();
    let mut content_length = 0;
    let mut body_too_large = false;
    let mut has_pin_action_header = false;
    let mut origin_header = None;
    let mut content_type_header = None;
    let mut if_none_match = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() || line.trim().is_empty() {
            break;
        }
        let trimmed = line.trim();
        if trimmed.to_ascii_lowercase().starts_with("content-length:") {
            if let Some((_, val)) = trimmed.split_once(':') {
                match val.trim().parse::<usize>() {
                    Ok(n) if n <= MAX_SCREENSHOT_BODY => content_length = n,
                    Ok(_) => body_too_large = true,
                    Err(_) => content_length = 0,
                }
            }
        }
        if trimmed.to_ascii_lowercase().starts_with("content-type:") {
            if let Some((_, val)) = trimmed.split_once(':') {
                content_type_header = Some(val.trim().to_string());
            }
        }
        if trimmed.to_ascii_lowercase().starts_with("x-pin-action:") {
            has_pin_action_header = true;
        }
        if trimmed.to_ascii_lowercase().starts_with("origin:") {
            if let Some((_, val)) = trimmed.split_once(':') {
                origin_header = Some(val.trim().to_string());
            }
        }
        if trimmed.to_ascii_lowercase().starts_with("if-none-match:") {
            if let Some((_, val)) = trimmed.split_once(':') {
                if_none_match = Some(val.trim().to_string());
            }
        }
        headers.push(line);
    }

    let expected_prefix = format!("/{}/", snapshot.token);
    if !raw_path.starts_with(&expected_prefix) {
        send_response(
            &mut stream,
            404,
            "Not Found",
            "text/plain; charset=utf-8",
            b"Not Found",
            false,
            true,
            None,
        );
        return;
    }

    let subpath = &raw_path[expected_prefix.len()..];

    let is_screenshot_upload = subpath == "screenshots" || subpath == "screenshots/upload";
    let is_oversized = if is_screenshot_upload {
        body_too_large || content_length > MAX_SCREENSHOT_BODY
    } else {
        body_too_large || content_length > MAX_REQUEST_BODY
    };

    if method == "POST" && is_oversized {
        let limit_msg: &[u8] = if is_screenshot_upload {
            b"{\"error\":\"Request body exceeds the 10 MiB limit\"}"
        } else {
            b"{\"error\":\"Request body exceeds the 1 MiB limit\"}"
        };
        send_response(
            &mut stream,
            413,
            "Payload Too Large",
            "application/json; charset=utf-8",
            limit_msg,
            false,
            true,
            None,
        );
        return;
    }
    // POST /items/{id}/action
    if method == "POST" {
        if subpath == "screenshots" || subpath == "screenshots/upload" {
            let valid_origin = origin_header.as_deref().is_some_and(|orig| {
                orig == format!("http://127.0.0.1:{port}")
                    || orig == format!("http://localhost:{port}")
            });

            if !valid_origin || !has_pin_action_header {
                send_response(
                    &mut stream,
                    403,
                    "Forbidden",
                    "application/json; charset=utf-8",
                    b"{\"error\":\"Forbidden request\"}",
                    false,
                    true,
                    None,
                );
                return;
            }

            if content_length == 0 {
                send_response(
                    &mut stream,
                    400,
                    "Bad Request",
                    "application/json; charset=utf-8",
                    b"{\"error\":\"Screenshot payload cannot be empty\"}",
                    false,
                    true,
                    None,
                );
                return;
            }

            let mut body_bytes = vec![0u8; content_length];
            if reader.read_exact(&mut body_bytes).is_err() {
                send_response(
                    &mut stream,
                    400,
                    "Bad Request",
                    "application/json; charset=utf-8",
                    b"{\"error\":\"Failed to read body\"}",
                    false,
                    true,
                    None,
                );
                return;
            }

            let ext = match content_type_header.as_deref() {
                Some(ct) if ct.contains("jpeg") || ct.contains("jpg") => "jpg",
                Some(ct) if ct.contains("gif") => "gif",
                Some(ct) if ct.contains("webp") => "webp",
                Some(ct) if ct.contains("svg") => "svg",
                _ => "png",
            };

            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default();
            let secs = now.as_secs();
            let nanos = now.subsec_nanos();
            static SCREENSHOT_COUNTER: AtomicU64 = AtomicU64::new(1);
            let count = SCREENSHOT_COUNTER.fetch_add(1, Ordering::Relaxed);
            let filename = format!("screenshot-{secs}_{nanos}_{count}.{ext}");

            let dir = std::env::temp_dir().join("pin-screenshots");
            if let Err(e) = std::fs::create_dir_all(&dir) {
                let msg = format!("{{\"error\":\"Failed to create screenshot directory: {e}\"}}");
                send_response(
                    &mut stream,
                    500,
                    "Internal Server Error",
                    "application/json; charset=utf-8",
                    msg.as_bytes(),
                    false,
                    true,
                    None,
                );
                return;
            }

            let file_path = dir.join(&filename);
            if let Err(e) = std::fs::write(&file_path, &body_bytes) {
                let msg = format!("{{\"error\":\"Failed to write screenshot file: {e}\"}}");
                send_response(
                    &mut stream,
                    500,
                    "Internal Server Error",
                    "application/json; charset=utf-8",
                    msg.as_bytes(),
                    false,
                    true,
                    None,
                );
                return;
            }

            let abs_path = file_path.to_string_lossy().to_string();
            let resp = serde_json::json!({
                "path": abs_path,
                "filename": filename,
                "markdown": format!("![screenshot]({abs_path})")
            });
            let resp_bytes = serde_json::to_vec(&resp).unwrap_or_default();
            send_response(
                &mut stream,
                200,
                "OK",
                "application/json; charset=utf-8",
                &resp_bytes,
                false,
                true,
                None,
            );
            return;
        }

        if subpath == "items" || subpath == "items/create" {
            let valid_origin = origin_header.as_deref().is_some_and(|orig| {
                orig == format!("http://127.0.0.1:{port}")
                    || orig == format!("http://localhost:{port}")
            });

            if !valid_origin || !has_pin_action_header {
                send_response(
                    &mut stream,
                    403,
                    "Forbidden",
                    "application/json; charset=utf-8",
                    b"{\"error\":\"Forbidden request\"}",
                    false,
                    true,
                    None,
                );
                return;
            }

            let mut body_bytes = vec![0u8; content_length];
            if reader.read_exact(&mut body_bytes).is_err() {
                send_response(
                    &mut stream,
                    400,
                    "Bad Request",
                    "application/json; charset=utf-8",
                    b"{\"error\":\"Failed to read body\"}",
                    false,
                    true,
                    None,
                );
                return;
            }

            let payload: CreateItemPayload = match serde_json::from_slice(&body_bytes) {
                Ok(p) => p,
                Err(e) => {
                    let msg = format!("{{\"error\":\"Invalid JSON: {e}\"}}");
                    send_response(
                        &mut stream,
                        400,
                        "Bad Request",
                        "application/json; charset=utf-8",
                        msg.as_bytes(),
                        false,
                        true,
                        None,
                    );
                    return;
                }
            };

            let title_trimmed = payload.title.trim();
            if title_trimmed.is_empty() {
                send_response(
                    &mut stream,
                    400,
                    "Bad Request",
                    "application/json; charset=utf-8",
                    b"{\"error\":\"Title cannot be empty\"}",
                    false,
                    true,
                    None,
                );
                return;
            }

            let proj_name = payload
                .project
                .filter(|p| !p.trim().is_empty())
                .unwrap_or_else(|| {
                    if snapshot.scope_label != "all" {
                        snapshot.scope_label.clone()
                    } else {
                        "default".to_string()
                    }
                });

            let id = crate::vault::generate_id();
            let final_type = payload.item_type.unwrap_or(WorkType::Task);
            let final_kind = payload.kind.unwrap_or(Kind::Technical);
            let final_status = payload.status.unwrap_or(Status::Created);
            let viewer_actor =
                env::var("PIN_VIEWER_ACTOR").unwrap_or_else(|_| "human:viewer".to_string());
            let creator = payload.actor.unwrap_or(viewer_actor);
            let body_content = match payload.body {
                Some(b) if !b.trim().is_empty() => {
                    let trimmed = b.trim();
                    if trimmed.starts_with('#') {
                        format!("{trimmed}\n")
                    } else {
                        format!("# {}\n\n{trimmed}\n", title_trimmed)
                    }
                }
                _ => format!("# {}\n", title_trimmed),
            };

            let clean_tags = payload.tags.and_then(|t| {
                let trimmed = t.trim().to_string();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed)
                }
            });

            let item = IdeaMeta::new_work_item(
                id.clone(),
                proj_name,
                title_trimmed.to_string(),
                body_content,
                final_kind,
                final_type,
                final_status,
                payload.priority,
                clean_tags,
                Some(creator),
            );

            let file_path = snapshot.vault_path.join(format!("{id}.md"));
            let rendered = crate::frontmatter::render_full_document(&item);
            if let Err(e) = crate::vault::atomic_write(&file_path, &rendered) {
                let msg = format!("{{\"error\":\"Failed to save item: {e}\"}}");
                send_response(
                    &mut stream,
                    500,
                    "Internal Server Error",
                    "application/json; charset=utf-8",
                    msg.as_bytes(),
                    false,
                    true,
                    None,
                );
                return;
            }

            let json_item = JsonIdeaOutput::from(&item);
            let body_str = serde_json::to_string(&json_item).unwrap_or_default();
            send_response(
                &mut stream,
                201,
                "Created",
                "application/json; charset=utf-8",
                body_str.as_bytes(),
                true,
                true,
                None,
            );
            return;
        }

        if let Some(stripped) = subpath.strip_prefix("items/") {
            if let Some(id_part) = stripped.strip_suffix("/action") {
                let id = id_part.trim();

                // CSRF verification: must have Origin matching server address and X-Pin-Action header
                let valid_origin = origin_header.as_deref().is_some_and(|orig| {
                    orig == format!("http://127.0.0.1:{port}")
                        || orig == format!("http://localhost:{port}")
                });

                if !valid_origin || !has_pin_action_header {
                    send_response(
                        &mut stream,
                        403,
                        "Forbidden",
                        "application/json; charset=utf-8",
                        b"{\"error\":\"Forbidden request\"}",
                        false,
                        true,
                        None,
                    );
                    return;
                }

                // Read body
                let mut body_bytes = vec![0u8; content_length];
                if reader.read_exact(&mut body_bytes).is_err() {
                    send_response(
                        &mut stream,
                        400,
                        "Bad Request",
                        "application/json; charset=utf-8",
                        b"{\"error\":\"Failed to read body\"}",
                        false,
                        true,
                        None,
                    );
                    return;
                }

                let payload: ActionPayload = match serde_json::from_slice(&body_bytes) {
                    Ok(p) => p,
                    Err(e) => {
                        let msg = format!("{{\"error\":\"Invalid JSON: {e}\"}}");
                        send_response(
                            &mut stream,
                            400,
                            "Bad Request",
                            "application/json; charset=utf-8",
                            msg.as_bytes(),
                            false,
                            true,
                            None,
                        );
                        return;
                    }
                };

                let filename = match resolve_selector(&snapshot.vault_path, id) {
                    Ok(f) => f,
                    Err(e) => {
                        let msg = format!("{{\"error\":\"{e}\"}}");
                        send_response(
                            &mut stream,
                            404,
                            "Not Found",
                            "application/json; charset=utf-8",
                            msg.as_bytes(),
                            false,
                            true,
                            None,
                        );
                        return;
                    }
                };

                let viewer_actor =
                    env::var("PIN_VIEWER_ACTOR").unwrap_or_else(|_| "human:viewer".to_string());
                let actor_to_use = payload.actor.as_deref().unwrap_or(&viewer_actor);
                // Check if moving to in_progress or claiming requires worktree when primary is busy
                let is_starting_run = payload.action == "claim"
                    || (payload.action == "transition" && payload.to == Some(Status::InProgress));
                if is_starting_run
                    && payload.use_worktree != Some(true)
                    && snapshot.acp_manager.is_primary_busy().is_some()
                {
                    let busy_id = snapshot.acp_manager.is_primary_busy().unwrap();
                    let msg = format!(
                        "{{\"error\":\"Primary checkout is busy\",\"status\":\"primary_busy\",\"active_id\":\"{busy_id}\"}}"
                    );
                    send_response(
                        &mut stream,
                        409,
                        "Conflict",
                        "application/json; charset=utf-8",
                        msg.as_bytes(),
                        true,
                        true,
                        None,
                    );
                    return;
                }

                let mutation_result = match payload.action.as_str() {
                    "transition" => match payload.to {
                        Some(target_status) => workflow::transition_item(
                            &snapshot.vault_path,
                            &filename,
                            target_status,
                            Some(actor_to_use),
                            payload.note.as_deref(),
                            payload.expect_revision,
                        ),
                        None => Err(workflow::WorkflowError::MissingTransitionTarget),
                    },
                    "claim" => workflow::claim_item(
                        &snapshot.vault_path,
                        &filename,
                        actor_to_use,
                        payload.lease.unwrap_or(3600),
                        payload.expect_revision,
                    ),
                    "release" => workflow::release_item(
                        &snapshot.vault_path,
                        &filename,
                        Some(actor_to_use),
                        payload.force.unwrap_or(false),
                        payload.expect_revision,
                    ),
                    "handoff" => workflow::handoff_item(
                        &snapshot.vault_path,
                        &filename,
                        Some(actor_to_use),
                        payload.progress.as_deref(),
                        payload.next.as_deref(),
                        payload.blocker.as_deref(),
                        payload.verification.as_deref(),
                        payload.expect_revision,
                    ),
                    "complete" => {
                        let ev = payload.evidence.as_deref().unwrap_or("");
                        workflow::complete_item(
                            &snapshot.vault_path,
                            &filename,
                            Some(actor_to_use),
                            ev,
                            payload.expect_revision,
                        )
                    }
                    "close" => workflow::close_item(
                        &snapshot.vault_path,
                        &filename,
                        Some(actor_to_use),
                        payload.note.as_deref(),
                        payload.expect_revision,
                    ),
                    "commit_pr" | "commit+pr" => {
                        match snapshot.acp_manager.start_commit_pr_run(id) {
                            Ok(()) => {
                                send_response(
                                    &mut stream,
                                    200,
                                    "OK",
                                    "application/json; charset=utf-8",
                                    b"{\"status\":\"started\",\"mode\":\"commit_pr\"}",
                                    true,
                                    true,
                                    None,
                                );
                                return;
                            }
                            Err(err) => {
                                let (status_code, reason, body) = start_failure_response(&err);
                                send_response(
                                    &mut stream,
                                    status_code,
                                    reason,
                                    "application/json; charset=utf-8",
                                    body.as_bytes(),
                                    true,
                                    true,
                                    None,
                                );
                                return;
                            }
                        }
                    }
                    other => Err(workflow::WorkflowError::UnknownAction(other.to_string())),
                };

                match mutation_result {
                    Ok(updated_meta) => {
                        if is_starting_run {
                            if let Err(err) = snapshot
                                .acp_manager
                                .start_run(id, payload.use_worktree.unwrap_or(false))
                            {
                                // The mutation already moved the item. A run
                                // that never launched must not strand it.
                                rollback_failed_start(&snapshot, &filename, &updated_meta);
                                let (status_code, reason, body) = start_failure_response(&err);
                                send_response(
                                    &mut stream,
                                    status_code,
                                    reason,
                                    "application/json; charset=utf-8",
                                    body.as_bytes(),
                                    true,
                                    true,
                                    None,
                                );
                                return;
                            }
                        } else if payload.action == "release"
                            || (payload.action == "transition"
                                && payload.to != Some(Status::InProgress))
                        {
                            let _ = snapshot.acp_manager.cancel_run(id);
                        }
                        let json_item = JsonIdeaOutput::from(&updated_meta);
                        let body_str = serde_json::to_string(&json_item).unwrap_or_default();
                        send_response(
                            &mut stream,
                            200,
                            "OK",
                            "application/json; charset=utf-8",
                            body_str.as_bytes(),
                            true,
                            true,
                            None,
                        );
                        return;
                    }
                    Err(err) => {
                        let status_code = match err {
                            workflow::WorkflowError::RevisionConflict { .. }
                            | workflow::WorkflowError::ClaimConflict { .. }
                            | workflow::WorkflowError::LockConflict(_) => 409,
                            workflow::WorkflowError::ItemNotFound(_) => 404,
                            workflow::WorkflowError::PermissionDenied(_) => 403,
                            _ => 400,
                        };
                        let msg = format!("{{\"error\":\"{err}\"}}");
                        send_response(
                            &mut stream,
                            status_code,
                            "Error",
                            "application/json; charset=utf-8",
                            msg.as_bytes(),
                            true,
                            true,
                            None,
                        );
                        return;
                    }
                }
            }
            if let Some(id_part) = stripped.strip_suffix("/run") {
                let id = id_part.trim();

                let mut body_bytes = vec![0u8; content_length];
                if content_length > 0 && reader.read_exact(&mut body_bytes).is_err() {
                    send_response(
                        &mut stream,
                        400,
                        "Bad Request",
                        "application/json; charset=utf-8",
                        b"{\"error\":\"Failed to read body\"}",
                        false,
                        true,
                        None,
                    );
                    return;
                }

                let payload: RunPayload = if content_length > 0 {
                    match serde_json::from_slice(&body_bytes) {
                        Ok(p) => p,
                        Err(e) => {
                            let msg = format!("{{\"error\":\"Invalid JSON: {e}\"}}");
                            send_response(
                                &mut stream,
                                400,
                                "Bad Request",
                                "application/json; charset=utf-8",
                                msg.as_bytes(),
                                false,
                                true,
                                None,
                            );
                            return;
                        }
                    }
                } else {
                    RunPayload {
                        use_worktree: None,
                        mode: None,
                    }
                };

                let run_result = if payload.mode.as_deref() == Some("commit_pr")
                    || payload.mode.as_deref() == Some("commit+pr")
                {
                    snapshot.acp_manager.start_commit_pr_run(id)
                } else {
                    snapshot
                        .acp_manager
                        .start_run(id, payload.use_worktree.unwrap_or(false))
                };

                match run_result {
                    Ok(()) => {
                        send_response(
                            &mut stream,
                            200,
                            "OK",
                            "application/json; charset=utf-8",
                            b"{\"status\":\"started\"}",
                            true,
                            true,
                            None,
                        );
                        return;
                    }
                    Err(err) => {
                        let (status_code, reason, body) = start_failure_response(&err);
                        send_response(
                            &mut stream,
                            status_code,
                            reason,
                            "application/json; charset=utf-8",
                            body.as_bytes(),
                            true,
                            true,
                            None,
                        );
                        return;
                    }
                }
            }

            if let Some(id_part) = stripped
                .strip_suffix("/commit-pr")
                .or_else(|| stripped.strip_suffix("/commit_pr"))
            {
                let id = id_part.trim();
                match snapshot.acp_manager.start_commit_pr_run(id) {
                    Ok(()) => {
                        send_response(
                            &mut stream,
                            200,
                            "OK",
                            "application/json; charset=utf-8",
                            b"{\"status\":\"started\",\"mode\":\"commit_pr\"}",
                            true,
                            true,
                            None,
                        );
                        return;
                    }
                    Err(err) => {
                        let (status_code, reason, body) = start_failure_response(&err);
                        send_response(
                            &mut stream,
                            status_code,
                            reason,
                            "application/json; charset=utf-8",
                            body.as_bytes(),
                            true,
                            true,
                            None,
                        );
                        return;
                    }
                }
            }

            if let Some(id_part) = stripped.strip_suffix("/cancel") {
                let id = id_part.trim();
                match snapshot.acp_manager.cancel_run(id) {
                    Ok(()) => {
                        send_response(
                            &mut stream,
                            200,
                            "OK",
                            "application/json; charset=utf-8",
                            b"{\"status\":\"cancelled\"}",
                            true,
                            true,
                            None,
                        );
                    }
                    Err(err) => {
                        let msg = format!("{{\"error\":\"{err}\"}}");
                        send_response(
                            &mut stream,
                            400,
                            "Bad Request",
                            "application/json; charset=utf-8",
                            msg.as_bytes(),
                            true,
                            true,
                            None,
                        );
                    }
                }
                return;
            }
        }

        send_response(
            &mut stream,
            405,
            "Method Not Allowed",
            "text/plain; charset=utf-8",
            b"Method Not Allowed",
            false,
            true,
            None,
        );
        return;
    }

    if method != "GET" && method != "HEAD" {
        send_response(
            &mut stream,
            405,
            "Method Not Allowed",
            "text/plain; charset=utf-8",
            b"Method Not Allowed",
            false,
            true,
            None,
        );
        return;
    }

    if let Some(filename) = subpath.strip_prefix("screenshots/") {
        let filename = filename.trim();
        if !is_valid_screenshot_filename(filename) {
            send_response(
                &mut stream,
                400,
                "Bad Request",
                "text/plain; charset=utf-8",
                b"Invalid screenshot filename",
                false,
                true,
                None,
            );
            return;
        }

        let file_path = std::env::temp_dir().join("pin-screenshots").join(filename);
        if file_path.is_file() {
            if let Ok(bytes) = std::fs::read(&file_path) {
                let mime = if filename.ends_with(".png") {
                    "image/png"
                } else if filename.ends_with(".jpg") || filename.ends_with(".jpeg") {
                    "image/jpeg"
                } else if filename.ends_with(".gif") {
                    "image/gif"
                } else if filename.ends_with(".webp") {
                    "image/webp"
                } else if filename.ends_with(".svg") {
                    "image/svg+xml"
                } else {
                    "application/octet-stream"
                };
                send_response(
                    &mut stream,
                    200,
                    "OK",
                    mime,
                    &bytes,
                    false,
                    method == "GET",
                    None,
                );
                return;
            }
        }

        send_response(
            &mut stream,
            404,
            "Not Found",
            "text/plain; charset=utf-8",
            b"Screenshot not found",
            false,
            true,
            None,
        );
        return;
    }

    if subpath == "runs" {
        #[derive(Serialize)]
        struct RunsOutput {
            running: Vec<String>,
            primary_busy: Option<String>,
            timings: std::collections::HashMap<String, i64>,
        }
        let (running, timings) = snapshot.acp_manager.running_items_with_timing();
        let out = RunsOutput {
            running,
            primary_busy: snapshot.acp_manager.is_primary_busy(),
            timings,
        };
        let out_json = serde_json::to_string(&out).unwrap_or_else(|_| "{}".to_string());
        send_response(
            &mut stream,
            200,
            "OK",
            "application/json; charset=utf-8",
            out_json.as_bytes(),
            true,
            method == "GET",
            None,
        );
        return;
    }

    if let Some(stripped) = subpath.strip_prefix("items/") {
        if let Some(id_part) = stripped.strip_suffix("/log") {
            let id = id_part.trim();
            let canonical_id = match crate::vault::resolve_selector(&snapshot.vault_path, id) {
                Ok(f) => f.strip_suffix(".md").unwrap_or(&f).to_string(),
                Err(_) => id.to_string(),
            };
            let log_path = snapshot
                .vault_path
                .join("runs")
                .join(format!("{canonical_id}.log"));
            if log_path.is_file() {
                if let Ok(bytes) = std::fs::read(&log_path) {
                    send_response(
                        &mut stream,
                        200,
                        "OK",
                        "text/plain; charset=utf-8",
                        &bytes,
                        true,
                        method == "GET",
                        None,
                    );
                    return;
                }
            }
            send_response(
                &mut stream,
                404,
                "Not Found",
                "text/plain; charset=utf-8",
                b"Log not found",
                false,
                true,
                None,
            );
            return;
        }

        if let Some(id) = stripped.strip_suffix("/stream") {
            let id = id.trim();
            let _ = stream.set_read_timeout(None);
            let _ = stream.set_write_timeout(None);

            let mut writer = BufWriter::new(stream);
            let sse_headers = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\nAccess-Control-Allow-Origin: *\r\n\r\n";
            if writer.write_all(sse_headers.as_bytes()).is_err() || writer.flush().is_err() {
                return;
            }
            let timing = snapshot.acp_manager.get_run_timing(id);
            if let Some((started_at, finished_at_opt)) = timing {
                let init_timing_event = serde_json::json!({
                    "type": "run_timing",
                    "startedAt": started_at,
                    "finishedAt": finished_at_opt
                });
                let payload = serde_json::to_string(&init_timing_event).unwrap_or_default();
                let frame = format!("data: {payload}\n\n");
                let _ = writer.write_all(frame.as_bytes());
            }

            let buffered = snapshot.acp_manager.get_buffered_events(id);
            let mut has_finished = false;
            for event in &buffered {
                if let Some(t) = event.get("type").and_then(|v| v.as_str()) {
                    if t == "finished" || t == "error" {
                        has_finished = true;
                    }
                }
                let payload = serde_json::to_string(event).unwrap_or_default();
                let frame = format!("data: {payload}\n\n");
                if writer.write_all(frame.as_bytes()).is_err() {
                    return;
                }
            }
            if writer.flush().is_err() {
                return;
            }

            let sub_rx = snapshot.acp_manager.subscribe(id);
            if sub_rx.is_none() && !buffered.is_empty() {
                if !has_finished {
                    let finish_frame = serde_json::json!({
                        "type": "finished",
                        "stopReason": "completed"
                    });
                    let payload = serde_json::to_string(&finish_frame).unwrap_or_default();
                    let frame = format!("data: {payload}\n\n");
                    let _ = writer.write_all(frame.as_bytes());
                    let _ = writer.flush();
                }
                return;
            }

            if let Some(rx) = sub_rx {
                while let Ok(event) = rx.recv() {
                    let payload = serde_json::to_string(&event).unwrap_or_default();
                    let frame = format!("data: {payload}\n\n");
                    if writer.write_all(frame.as_bytes()).is_err() || writer.flush().is_err() {
                        return;
                    }
                }
            }
            return;
        }
    }

    if subpath == "data.json" {
        let etag = compute_vault_etag(&snapshot.vault_path);
        if let Some(if_none) = &if_none_match {
            if etags_match(if_none, &etag) {
                send_not_modified(&mut stream, &etag);
                return;
            }
        }

        let filter_project = if snapshot.scope_label == "all" {
            None
        } else {
            Some(snapshot.scope_label.as_str())
        };
        let filter = FilterOptions {
            project: filter_project,
            status: snapshot.filter_status,
            tag: snapshot.filter_tag.as_deref(),
            kind: snapshot.filter_kind,
            item_type: snapshot.filter_type,
            archive_filter: snapshot.archive_filter,
            ..Default::default()
        };
        let ideas = collect_ideas_with_filter(&snapshot.vault_path, &filter).unwrap_or_default();
        let captured_at = Utc::now().to_rfc3339();

        let json_items: Vec<JsonIdeaOutput> = ideas
            .iter()
            .map(|meta| {
                let mut item = JsonIdeaOutput::from(meta);
                item.content = Some(&meta.body);
                item
            })
            .collect();

        let count = json_items.len();
        let archive_str = match snapshot.archive_filter {
            ArchiveFilter::Active => "active",
            ArchiveFilter::Archived => "archived",
            ArchiveFilter::All => "all",
        };

        let data = SnapshotData {
            scope: &snapshot.scope_label,
            archive_filter: archive_str,
            captured_at,
            filters: SnapshotFilters {
                status: snapshot.filter_status.map(|s| s.as_str().to_string()),
                tag: snapshot.filter_tag.clone(),
                kind: snapshot.filter_kind.map(|k| k.as_str().to_string()),
                item_type: snapshot.filter_type.map(|t| t.as_str().to_string()),
            },
            items: json_items,
            count,
        };

        let data_json = serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string());
        send_response(
            &mut stream,
            200,
            "OK",
            "application/json; charset=utf-8",
            data_json.as_bytes(),
            true,
            method == "GET",
            Some(&etag),
        );
        return;
    }

    let modified_html;
    let (content_type, body): (&str, &[u8]) = match subpath {
        "" | "index.html" => {
            let token_base = format!("data-base=\"/{}/\"", snapshot.token);
            modified_html = INDEX_HTML.replace("data-base=\"\"", &token_base);
            ("text/html; charset=utf-8", modified_html.as_bytes())
        }
        "app.css" => ("text/css; charset=utf-8", APP_CSS.as_bytes()),
        "app.js" => ("text/javascript; charset=utf-8", APP_JS.as_bytes()),
        "marked.min.js" => ("text/javascript; charset=utf-8", MARKED_JS.as_bytes()),
        "purify.min.js" => ("text/javascript; charset=utf-8", PURIFY_JS.as_bytes()),
        _ => {
            send_response(
                &mut stream,
                404,
                "Not Found",
                "text/plain; charset=utf-8",
                b"Not Found",
                false,
                true,
                None,
            );
            return;
        }
    };

    send_response(
        &mut stream,
        200,
        "OK",
        content_type,
        body,
        true,
        method == "GET",
        None,
    );
}

pub fn open_browser(url: &str) {
    let res = if cfg!(target_os = "windows") {
        Command::new("cmd").args(["/c", "start", "", url]).spawn()
    } else if cfg!(target_os = "macos") {
        Command::new("open").arg(url).spawn()
    } else {
        Command::new("xdg-open").arg(url).spawn()
    };

    match res {
        Ok(mut child) => {
            if child.wait().is_ok() {
                eprintln!("Opened in browser.");
            } else {
                eprintln!("Warning: browser launcher did not exit cleanly");
            }
        }
        Err(e) => {
            eprintln!("Warning: failed to open browser: {e}");
        }
    }
}

pub fn serve_view(
    mut snapshot: ViewSnapshot,
    port: u16,
    no_open: bool,
    format: OutputFormat,
    acp_command: Option<String>,
) -> io::Result<()> {
    if acp_command.is_some() {
        snapshot.acp_command = acp_command.clone();
    }
    let repo_path = std::env::current_dir().unwrap_or_else(|_| {
        snapshot
            .vault_path
            .parent()
            .unwrap_or(&snapshot.vault_path)
            .to_path_buf()
    });
    let acp_mgr = Arc::new(crate::acp::AcpManager::new(
        snapshot.vault_path.clone(),
        repo_path,
        acp_command,
    ));
    snapshot.acp_manager = acp_mgr;
    let listener = TcpListener::bind(format!("127.0.0.1:{port}"))?;
    let local_addr = listener.local_addr()?;
    let base_url = format!("http://127.0.0.1:{}/{}/", local_addr.port(), snapshot.token);

    match format {
        OutputFormat::Json => {
            #[derive(Serialize)]
            struct ViewJsonOutput<'a> {
                url: &'a str,
                port: u16,
                token: &'a str,
            }
            let out = ViewJsonOutput {
                url: &base_url,
                port: local_addr.port(),
                token: &snapshot.token,
            };
            println!("{}", serde_json::to_string(&out).unwrap_or_default());
        }
        OutputFormat::Plain | OutputFormat::Table => {
            println!("{base_url}");
        }
    }

    if !no_open {
        open_browser(&base_url);
    }

    let shared_snapshot = Arc::new(snapshot);
    let server_port = local_addr.port();

    for stream in listener.incoming().flatten() {
        let snap = Arc::clone(&shared_snapshot);
        thread::spawn(move || {
            handle_client(stream, &snap, server_port);
        });
    }

    Ok(())
}

fn is_valid_screenshot_filename(filename: &str) -> bool {
    let filename = filename.trim();
    !filename.is_empty()
        && !filename.contains('/')
        && !filename.contains('\\')
        && !filename.contains("..")
        && filename
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_snapshot_preserves_cli_filters() {
        let snapshot = create_snapshot(
            &[],
            PathBuf::from("/tmp/pin-vault"),
            "all",
            ArchiveFilter::All,
            Some(Status::Blocked),
            Some("perf".to_string()),
            Some(Kind::Technical),
            Some(WorkType::Bug),
        );

        assert_eq!(snapshot.filter_status, Some(Status::Blocked));
        assert_eq!(snapshot.filter_tag.as_deref(), Some("perf"));
        assert_eq!(snapshot.filter_kind, Some(Kind::Technical));
        assert_eq!(snapshot.filter_type, Some(WorkType::Bug));
        assert_eq!(snapshot.archive_filter, ArchiveFilter::All);
    }

    #[test]
    fn test_screenshot_filename_validation() {
        assert!(is_valid_screenshot_filename("screenshot-123_456_1.png"));
        assert!(is_valid_screenshot_filename("image.jpg"));
        assert!(is_valid_screenshot_filename("test-img.webp"));
        assert!(!is_valid_screenshot_filename(""));
        assert!(!is_valid_screenshot_filename("../secret.txt"));
        assert!(!is_valid_screenshot_filename("sub/dir/img.png"));
        assert!(!is_valid_screenshot_filename("sub\\dir\\img.png"));
        assert!(!is_valid_screenshot_filename("foo..bar.png"));
        assert!(!is_valid_screenshot_filename("test;rm -rf.png"));
    }

    #[test]
    fn test_screenshot_file_roundtrip() {
        let dir = std::env::temp_dir().join("pin-screenshots");
        std::fs::create_dir_all(&dir).unwrap();
        let filename = "screenshot-test-roundtrip.png";
        let file_path = dir.join(filename);
        let fake_bytes = b"fake-png-data-for-testing";
        std::fs::write(&file_path, fake_bytes).unwrap();

        assert!(file_path.is_file());
        let read_back = std::fs::read(&file_path).unwrap();
        assert_eq!(read_back, fake_bytes);

        let _ = std::fs::remove_file(&file_path);
    }
}
