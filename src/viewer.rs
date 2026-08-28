use crate::assets::*;
use crate::model::{ArchiveFilter, Handoff, IdeaMeta, Kind, OutputFormat, Status};
use crate::output::JsonIdeaOutput;
use crate::vault::{
    collect_work_items_filtered, generate_token, validate_selector, WorkItemFilter,
};
use crate::workflow::{self, MutationAction};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const VIEW_CSP: &str = "default-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'; img-src 'self' data:; style-src 'self'; script-src 'self'; connect-src 'self'; sandbox allow-scripts allow-same-origin";
const MAX_REQUEST_BODY: usize = 64 * 1024;
const MAX_REQUEST_HEADERS: usize = 16 * 1024;

#[derive(Serialize)]
struct SnapshotData<'a> {
    scope: &'a str,
    archive_filter: &'a str,
    captured_at: String,
    actor: &'a str,
    items: Vec<JsonIdeaOutput<'a>>,
    count: usize,
}

pub struct ViewSnapshot {
    pub token: String,
    pub config: ViewConfig,
    pub origin: String,
}

#[derive(Clone)]
pub struct ViewConfig {
    pub vault_path: PathBuf,
    pub scope_label: String,
    pub project: Option<String>,
    pub tag: Option<String>,
    pub kind: Option<Kind>,
    pub item_type: Option<crate::model::WorkType>,
    pub status: Option<Status>,
    pub archive_filter: ArchiveFilter,
}

pub fn create_snapshot(config: ViewConfig) -> ViewSnapshot {
    let token = generate_token();

    ViewSnapshot {
        token,
        config,
        origin: String::new(),
    }
}

#[derive(Debug)]
struct HttpRequest {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

#[derive(Debug, Deserialize)]
struct ActionRequest {
    action: String,
    to: Option<String>,
    note: Option<String>,
    lease: Option<i64>,
    force: Option<bool>,
    progress: Option<String>,
    next: Option<String>,
    blocker: Option<String>,
    verification: Option<String>,
    evidence: Option<String>,
    dependency: Option<String>,
    expect_revision: Option<u64>,
}

fn read_request(stream: &TcpStream) -> io::Result<HttpRequest> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    if request_line.len() > MAX_REQUEST_HEADERS {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "request headers are too large",
        ));
    }
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 2 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid request line",
        ));
    }

    let mut headers = HashMap::new();
    let mut header_bytes = request_line.len();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        header_bytes = header_bytes.saturating_add(line.len());
        if header_bytes > MAX_REQUEST_HEADERS {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "request headers are too large",
            ));
        }
        if line == "\r\n" || line == "\n" || line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }

    let content_length = headers
        .get("content-length")
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid content length"))
        })
        .transpose()?
        .unwrap_or(0);
    if content_length > MAX_REQUEST_BODY {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "request body is too large",
        ));
    }
    let mut body = vec![0; content_length];
    reader.read_exact(&mut body)?;

    Ok(HttpRequest {
        method: parts[0].to_string(),
        path: parts[1].split('?').next().unwrap_or(parts[1]).to_string(),
        headers,
        body,
    })
}

fn write_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
    include_body: bool,
) {
    let headers = format!(
        "HTTP/1.1 {status}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         Content-Security-Policy: {VIEW_CSP}\r\n\
         X-Content-Type-Options: nosniff\r\n\
         X-Frame-Options: DENY\r\n\
         Cache-Control: no-store\r\n\
         Connection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(headers.as_bytes());
    if include_body {
        let _ = stream.write_all(body);
    }
}

fn json_error(message: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({ "error": message }))
        .unwrap_or_else(|_| b"{\"error\":\"request failed\"}".to_vec())
}

fn mutation_action(
    vault_path: &std::path::Path,
    selector: &str,
    request: ActionRequest,
) -> Result<IdeaMeta, String> {
    if !validate_selector(selector) {
        return Err("invalid item selector".to_string());
    }
    let actor = std::env::var("PIN_VIEWER_ACTOR")
        .ok()
        .filter(|actor| !actor.trim().is_empty())
        .unwrap_or_else(|| "human:viewer".to_string());
    let expected_revision = request.expect_revision;
    let action = match request.action.as_str() {
        "transition" => {
            let target = request
                .to
                .as_deref()
                .ok_or_else(|| "transition requires 'to'".to_string())?
                .parse::<Status>()
                .map_err(|_| "invalid transition status".to_string())?;
            MutationAction::Transition {
                target,
                actor,
                note: request.note,
            }
        }
        "claim" => MutationAction::Claim {
            actor,
            lease: request.lease.unwrap_or(workflow::DEFAULT_CLAIM_SECONDS),
        },
        "release" => MutationAction::Release {
            actor,
            force: request.force.unwrap_or(false),
        },
        "handoff" => MutationAction::Handoff {
            actor,
            handoff: Handoff {
                progress: request.progress,
                next: request.next,
                blocker: request.blocker,
                verification: request.verification,
            },
        },
        "complete" => {
            let evidence = request
                .evidence
                .ok_or_else(|| "complete requires 'evidence'".to_string())?;
            MutationAction::Complete { actor, evidence }
        }
        "close" => MutationAction::Close {
            actor,
            note: request.note,
        },
        "depend" => {
            let dependency_selector = request
                .dependency
                .ok_or_else(|| "depend requires 'dependency'".to_string())?;
            MutationAction::Depend {
                actor,
                dependency_selector,
            }
        }
        "parent" => {
            let parent_selector = request
                .dependency
                .ok_or_else(|| "parent requires 'dependency' (parent ID)".to_string())?;
            MutationAction::Parent {
                actor,
                parent_selector,
            }
        }
        "relate" => {
            let related_selector = request
                .dependency
                .ok_or_else(|| "relate requires 'dependency' (related ID)".to_string())?;
            MutationAction::Relate {
                actor,
                related_selector,
            }
        }
        unknown => {
            return Err(workflow::WorkflowError::InvalidAction(unknown.to_string()).to_string())
        }
    };

    workflow::execute_mutation(vault_path, selector, expected_revision, action)
        .map_err(|error| error.to_string())
}

fn current_data(snapshot: &ViewSnapshot) -> io::Result<String> {
    let config = &snapshot.config;
    let items = collect_work_items_filtered(
        &config.vault_path,
        &WorkItemFilter {
            project: config.project.as_deref(),
            tag: config.tag.as_deref(),
            kind: config.kind,
            item_type: config.item_type,
            status: config.status,
            archive_filter: config.archive_filter,
            ..WorkItemFilter::default()
        },
    )?;
    Ok(render_data(
        &items,
        &config.scope_label,
        config.archive_filter,
    ))
}

fn render_data(ideas: &[IdeaMeta], scope_label: &str, archive_filter: ArchiveFilter) -> String {
    let captured_at = Utc::now().to_rfc3339();
    let actor = std::env::var("PIN_VIEWER_ACTOR")
        .ok()
        .filter(|actor| !actor.trim().is_empty())
        .unwrap_or_else(|| "human:viewer".to_string());
    let items: Vec<JsonIdeaOutput> = ideas
        .iter()
        .map(|meta| {
            let mut item = JsonIdeaOutput::from(meta);
            item.content = Some(&meta.body);
            item
        })
        .collect();
    let archive_filter = match archive_filter {
        ArchiveFilter::Active => "active",
        ArchiveFilter::Archived => "archived",
        ArchiveFilter::All => "all",
    };
    serde_json::to_string(&SnapshotData {
        scope: scope_label,
        archive_filter,
        captured_at,
        actor: &actor,
        count: items.len(),
        items,
    })
    .unwrap_or_else(|_| "{}".to_string())
}

fn handle_client(mut stream: TcpStream, snapshot: &Arc<ViewSnapshot>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let request = match read_request(&stream) {
        Ok(request) => request,
        Err(error) => {
            write_response(
                &mut stream,
                "400 Bad Request",
                "application/json; charset=utf-8",
                &json_error(&error.to_string()),
                true,
            );
            return;
        }
    };

    let expected_prefix = format!("/{}/", snapshot.token);
    if !request.path.starts_with(&expected_prefix) {
        write_response(
            &mut stream,
            "404 Not Found",
            "text/plain; charset=utf-8",
            b"Not Found",
            request.method != "HEAD",
        );
        return;
    }

    let subpath = &request.path[expected_prefix.len()..];
    if request.method == "POST" && subpath.starts_with("items/") {
        let Some(selector) = subpath
            .strip_prefix("items/")
            .and_then(|path| path.strip_suffix("/action"))
        else {
            write_response(
                &mut stream,
                "404 Not Found",
                "text/plain; charset=utf-8",
                b"Not Found",
                true,
            );
            return;
        };
        let origin_ok = request.headers.get("origin") == Some(&snapshot.origin);
        let action_header_ok = request
            .headers
            .get("x-pin-action")
            .is_some_and(|value| value == "true");
        let content_type_ok = request
            .headers
            .get("content-type")
            .is_some_and(|value| value.starts_with("application/json"));
        if !origin_ok || !action_header_ok || !content_type_ok {
            write_response(
                &mut stream,
                "403 Forbidden",
                "application/json; charset=utf-8",
                &json_error("mutation requires a same-origin JSON request"),
                true,
            );
            return;
        }
        let action = match serde_json::from_slice::<ActionRequest>(&request.body) {
            Ok(action) => action,
            Err(error) => {
                write_response(
                    &mut stream,
                    "400 Bad Request",
                    "application/json; charset=utf-8",
                    &json_error(&format!("invalid action: {error}")),
                    true,
                );
                return;
            }
        };
        match mutation_action(&snapshot.config.vault_path, selector, action) {
            Ok(meta) => {
                let item = JsonIdeaOutput::from(&meta);
                let body = serde_json::to_vec(&serde_json::json!({
                    "action": "updated",
                    "item": item,
                }))
                .unwrap_or_else(|_| b"{}".to_vec());
                write_response(
                    &mut stream,
                    "200 OK",
                    "application/json; charset=utf-8",
                    &body,
                    true,
                );
            }
            Err(error) => write_response(
                &mut stream,
                "409 Conflict",
                "application/json; charset=utf-8",
                &json_error(&error),
                true,
            ),
        }
        return;
    }

    if request.method != "GET" && request.method != "HEAD" {
        write_response(
            &mut stream,
            "405 Method Not Allowed",
            "text/plain; charset=utf-8",
            b"Method Not Allowed",
            request.method != "HEAD",
        );
        return;
    }

    let (content_type, body) = match subpath {
        "" | "index.html" => {
            let html = INDEX_HTML.replace(
                "data-base=\"\"",
                &format!("data-base=\"/{}\"", snapshot.token),
            );
            ("text/html; charset=utf-8", html.into_bytes())
        }
        "app.css" => ("text/css; charset=utf-8", APP_CSS.as_bytes().to_vec()),
        "app.js" => ("text/javascript; charset=utf-8", APP_JS.as_bytes().to_vec()),
        "marked.min.js" => (
            "text/javascript; charset=utf-8",
            MARKED_JS.as_bytes().to_vec(),
        ),
        "purify.min.js" => (
            "text/javascript; charset=utf-8",
            PURIFY_JS.as_bytes().to_vec(),
        ),
        "data.json" => match current_data(snapshot) {
            Ok(data) => ("application/json; charset=utf-8", data.into_bytes()),
            Err(error) => {
                write_response(
                    &mut stream,
                    "500 Internal Server Error",
                    "application/json; charset=utf-8",
                    &json_error(&error.to_string()),
                    request.method != "HEAD",
                );
                return;
            }
        },
        _ => {
            write_response(
                &mut stream,
                "404 Not Found",
                "text/plain; charset=utf-8",
                b"Not Found",
                request.method != "HEAD",
            );
            return;
        }
    };
    write_response(
        &mut stream,
        "200 OK",
        content_type,
        &body,
        request.method != "HEAD",
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
) -> io::Result<()> {
    let listener = TcpListener::bind(format!("127.0.0.1:{port}"))?;
    let local_addr = listener.local_addr()?;
    let base_url = format!("http://127.0.0.1:{}/{}/", local_addr.port(), snapshot.token);
    snapshot.origin = format!("http://127.0.0.1:{}", local_addr.port());

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

    for stream in listener.incoming().flatten() {
        let snap = Arc::clone(&shared_snapshot);
        thread::spawn(move || {
            handle_client(stream, &snap);
        });
    }

    Ok(())
}
