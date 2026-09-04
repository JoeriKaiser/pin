use crate::assets::*;
use crate::model::{ArchiveFilter, IdeaMeta, OutputFormat, Status};
use crate::output::JsonIdeaOutput;
use crate::vault::{collect_ideas_with_filter, generate_token, resolve_selector, FilterOptions};
use crate::workflow;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::env;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::thread;

const VIEW_CSP: &str = "default-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'; img-src 'self' data:; style-src 'self'; script-src 'self'; connect-src 'self'; sandbox allow-scripts allow-same-origin";

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
}

#[derive(Deserialize)]
struct ActionPayload {
    action: String,
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

pub fn create_snapshot(
    _ideas: &[IdeaMeta],
    vault_path: PathBuf,
    scope_label: &str,
    archive_filter: ArchiveFilter,
) -> ViewSnapshot {
    let token = generate_token();
    ViewSnapshot {
        token,
        vault_path,
        scope_label: scope_label.to_string(),
        archive_filter,
    }
}

fn send_response(
    stream: &mut TcpStream,
    status_code: u16,
    status_text: &str,
    content_type: &str,
    body: &[u8],
    extra_security: bool,
    send_body: bool,
) {
    let mut header_str = format!(
        "HTTP/1.1 {status_code} {status_text}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
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
    let mut has_pin_action_header = false;
    let mut origin_header = None;

    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() || line.trim().is_empty() {
            break;
        }
        let trimmed = line.trim();
        if let Some(val) = trimmed.strip_prefix("Content-Length:").or_else(|| trimmed.strip_prefix("content-length:")) {
            content_length = val.trim().parse::<usize>().unwrap_or(0);
        }
        if trimmed.to_ascii_lowercase().starts_with("x-pin-action:") {
            has_pin_action_header = true;
        }
        if let Some(val) = trimmed.strip_prefix("Origin:").or_else(|| trimmed.strip_prefix("origin:")) {
            origin_header = Some(val.trim().to_string());
        }
        headers.push(line);
    }

    let expected_prefix = format!("/{}/", snapshot.token);
    if !raw_path.starts_with(&expected_prefix) {
        send_response(&mut stream, 404, "Not Found", "text/plain; charset=utf-8", b"Not Found", false, true);
        return;
    }

    let subpath = &raw_path[expected_prefix.len()..];

    // POST /items/{id}/action
    if method == "POST" {
        if let Some(stripped) = subpath.strip_prefix("items/") {
            if let Some(id_part) = stripped.strip_suffix("/action") {
                let id = id_part.trim();

                // CSRF verification: must have Origin matching server address and X-Pin-Action header
                let valid_origin = origin_header.as_deref().is_some_and(|orig| {
                    orig == format!("http://127.0.0.1:{port}")
                        || orig == format!("http://localhost:{port}")
                });

                if !valid_origin || !has_pin_action_header {
                    send_response(&mut stream, 403, "Forbidden", "application/json; charset=utf-8", b"{\"error\":\"Forbidden request\"}", false, true);
                    return;
                }

                // Read body
                let mut body_bytes = vec![0u8; content_length];
                if reader.read_exact(&mut body_bytes).is_err() {
                    send_response(&mut stream, 400, "Bad Request", "application/json; charset=utf-8", b"{\"error\":\"Failed to read body\"}", false, true);
                    return;
                }

                let payload: ActionPayload = match serde_json::from_slice(&body_bytes) {
                    Ok(p) => p,
                    Err(e) => {
                        let msg = format!("{{\"error\":\"Invalid JSON: {e}\"}}");
                        send_response(&mut stream, 400, "Bad Request", "application/json; charset=utf-8", msg.as_bytes(), false, true);
                        return;
                    }
                };

                let filename = match resolve_selector(&snapshot.vault_path, id) {
                    Ok(f) => f,
                    Err(e) => {
                        let msg = format!("{{\"error\":\"{e}\"}}");
                        send_response(&mut stream, 404, "Not Found", "application/json; charset=utf-8", msg.as_bytes(), false, true);
                        return;
                    }
                };

                let viewer_actor = env::var("PIN_VIEWER_ACTOR")
                    .unwrap_or_else(|_| "human:viewer".to_string());
                let actor_to_use = payload.actor.as_deref().unwrap_or(&viewer_actor);

                let mutation_result = match payload.action.as_str() {
                    "transition" => {
                        match payload.to {
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
                        }
                    }
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
                        let json_item = JsonIdeaOutput::from(&updated_meta);
                        let body_str = serde_json::to_string(&json_item).unwrap_or_default();
                        send_response(&mut stream, 200, "OK", "application/json; charset=utf-8", body_str.as_bytes(), true, true);
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
                        send_response(&mut stream, status_code, "Error", "application/json; charset=utf-8", msg.as_bytes(), true, true);
                        return;
                    }
                }
            }
        }

        send_response(&mut stream, 405, "Method Not Allowed", "text/plain; charset=utf-8", b"Method Not Allowed", false, true);
        return;
    }

    if method != "GET" && method != "HEAD" {
        send_response(&mut stream, 405, "Method Not Allowed", "text/plain; charset=utf-8", b"Method Not Allowed", false, true);
        return;
    }

    if subpath == "data.json" {
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
        send_response(&mut stream, 200, "OK", "application/json; charset=utf-8", data_json.as_bytes(), true, method == "GET");
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
            send_response(&mut stream, 404, "Not Found", "text/plain; charset=utf-8", b"Not Found", false, true);
            return;
        }
    };

    send_response(&mut stream, 200, "OK", content_type, body, true, method == "GET");
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
    snapshot: ViewSnapshot,
    port: u16,
    no_open: bool,
    format: OutputFormat,
) -> io::Result<()> {
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

    for stream in listener.incoming() {
        if let Ok(stream) = stream {
            let snap = Arc::clone(&shared_snapshot);
            thread::spawn(move || {
                handle_client(stream, &snap, server_port);
            });
        }
    }

    Ok(())
}
