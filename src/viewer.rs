use crate::acp::AcpManagerError;
use crate::assets::*;
use crate::model::{ArchiveFilter, IdeaMeta, Kind, OutputFormat, Priority, Status, WorkType};
use crate::output::JsonIdeaOutput;
use crate::vault::{collect_ideas_with_filter, generate_token, resolve_selector, FilterOptions};
use crate::workflow;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::env;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const VIEW_CSP: &str = "default-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'; img-src 'self' data:; style-src 'self'; script-src 'self'; connect-src 'self'; sandbox allow-scripts allow-same-origin allow-forms";

/// Bodies are small JSON payloads. Anything larger is a mistake or a stuck
/// client, and must not be turned into an allocation.
const MAX_REQUEST_BODY: usize = 1024 * 1024;

/// A client that connects and then stalls would otherwise hold its thread
/// forever, since the viewer is meant to stay up for hours.
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Serialize)]
struct SnapshotData<'a> {
    scope: &'a str,
    archive_filter: &'a str,
    captured_at: String,
    items: Vec<JsonIdeaOutput<'a>>,
    count: usize,
}

pub struct ViewSnapshot {
    pub token: String,
    pub vault_path: PathBuf,
    pub scope_label: String,
    pub archive_filter: ArchiveFilter,
    pub acp_command: Option<String>,
    pub acp_manager: Arc<crate::acp::AcpManager>,
}

#[derive(Deserialize)]
struct ActionPayload {
    action: String,
    #[serde(default)]
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
    #[serde(default)]
    pub use_worktree: Option<bool>,
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
        acp_command: None,
        acp_manager,
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
                    Ok(n) if n <= MAX_REQUEST_BODY => content_length = n,
                    Ok(_) => body_too_large = true,
                    Err(_) => content_length = 0,
                }
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

    if method == "POST" && body_too_large {
        send_response(
            &mut stream,
            413,
            "Payload Too Large",
            "application/json; charset=utf-8",
            b"{\"error\":\"Request body exceeds the 1 MiB limit\"}",
            false,
            true,
            None,
        );
        return;
    }

    // POST /items/{id}/action
    if method == "POST" {
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
            let body_content = payload
                .body
                .unwrap_or_else(|| format!("# {}\n", title_trimmed));

            let item = IdeaMeta::new_work_item(
                id.clone(),
                proj_name,
                title_trimmed.to_string(),
                body_content,
                final_kind,
                final_type,
                final_status,
                payload.priority,
                payload.tags,
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
                        None => Err(workflow::WorkflowError::InvalidTransition {
                            from: Status::Created,
                            to: Status::Created,
                            reason: "Missing 'to' status".to_string(),
                        }),
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
                    other => Err(workflow::WorkflowError::ParseError(format!(
                        "Unknown action '{other}'"
                    ))),
                };

                match mutation_result {
                    Ok(updated_meta) => {
                        if is_starting_run {
                            let _ = snapshot
                                .acp_manager
                                .start_run(id, payload.use_worktree.unwrap_or(false));
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
                    RunPayload { use_worktree: None }
                };

                match snapshot
                    .acp_manager
                    .start_run(id, payload.use_worktree.unwrap_or(false))
                {
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
                    Err(AcpManagerError::PrimaryBusy(active_id)) => {
                        let msg = format!(
                            "{{\"error\":\"Primary checkout is busy\",\"status\":\"primary_busy\",\"active_id\":\"{active_id}\"}}"
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

    if subpath == "runs" {
        #[derive(Serialize)]
        struct RunsOutput {
            running: Vec<String>,
            primary_busy: Option<String>,
        }
        let out = RunsOutput {
            running: snapshot.acp_manager.running_items(),
            primary_busy: snapshot.acp_manager.is_primary_busy(),
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
        if let Some(id) = stripped.strip_suffix("/stream") {
            let id = id.trim();
            let _ = stream.set_read_timeout(None);
            let _ = stream.set_write_timeout(None);

            let sse_headers = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\nAccess-Control-Allow-Origin: *\r\n\r\n";
            if stream.write_all(sse_headers.as_bytes()).is_err() || stream.flush().is_err() {
                return;
            }

            let buffered = snapshot.acp_manager.get_buffered_events(id);
            for event in buffered {
                let payload = serde_json::to_string(&event).unwrap_or_default();
                let frame = format!("data: {payload}\n\n");
                if stream.write_all(frame.as_bytes()).is_err() || stream.flush().is_err() {
                    return;
                }
            }

            if let Some(rx) = snapshot.acp_manager.subscribe(id) {
                while let Ok(event) = rx.recv() {
                    let payload = serde_json::to_string(&event).unwrap_or_default();
                    let frame = format!("data: {payload}\n\n");
                    if stream.write_all(frame.as_bytes()).is_err() || stream.flush().is_err() {
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
