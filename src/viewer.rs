use crate::assets::*;
use crate::model::{ArchiveFilter, IdeaMeta, OutputFormat};
use crate::output::JsonIdeaOutput;
use crate::vault::generate_token;
use chrono::Utc;
use serde::Serialize;
use std::io::{self, BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
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
    pub data_json: String,
}

pub fn create_snapshot(
    ideas: &[IdeaMeta],
    scope_label: &str,
    archive_filter: ArchiveFilter,
) -> ViewSnapshot {
    let token = generate_token();
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

    let archive_str = match archive_filter {
        ArchiveFilter::Active => "active",
        ArchiveFilter::Archived => "archived",
        ArchiveFilter::All => "all",
    };

    let data = SnapshotData {
        scope: scope_label,
        archive_filter: archive_str,
        captured_at,
        items: json_items,
        count,
    };

    let data_json = serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string());

    ViewSnapshot { token, data_json }
}

fn handle_client(mut stream: TcpStream, snapshot: &Arc<ViewSnapshot>) {
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

    if method != "GET" && method != "HEAD" {
        let response = format!(
            "HTTP/1.1 405 Method Not Allowed\r\n\
             Content-Type: text/plain; charset=utf-8\r\n\
             Content-Length: 18\r\n\
             Connection: close\r\n\r\n\
             Method Not Allowed"
        );
        let _ = stream.write_all(response.as_bytes());
        return;
    }

    let expected_prefix = format!("/{}/", snapshot.token);
    if !raw_path.starts_with(&expected_prefix) {
        let response = format!(
            "HTTP/1.1 404 Not Found\r\n\
             Content-Type: text/plain; charset=utf-8\r\n\
             Content-Length: 9\r\n\
             Connection: close\r\n\r\n\
             Not Found"
        );
        let _ = stream.write_all(response.as_bytes());
        return;
    }

    let subpath = &raw_path[expected_prefix.len()..];
    let (content_type, body) = match subpath {
        "" | "index.html" => ("text/html; charset=utf-8", INDEX_HTML.as_bytes()),
        "app.css" => ("text/css; charset=utf-8", APP_CSS.as_bytes()),
        "app.js" => ("text/javascript; charset=utf-8", APP_JS.as_bytes()),
        "marked.min.js" => ("text/javascript; charset=utf-8", MARKED_JS.as_bytes()),
        "purify.min.js" => ("text/javascript; charset=utf-8", PURIFY_JS.as_bytes()),
        "data.json" => (
            "application/json; charset=utf-8",
            snapshot.data_json.as_bytes(),
        ),
        _ => {
            let response = format!(
                "HTTP/1.1 404 Not Found\r\n\
                 Content-Type: text/plain; charset=utf-8\r\n\
                 Content-Length: 9\r\n\
                 Connection: close\r\n\r\n\
                 Not Found"
            );
            let _ = stream.write_all(response.as_bytes());
            return;
        }
    };

    let response_headers = format!(
        "HTTP/1.1 200 OK\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         Content-Security-Policy: {VIEW_CSP}\r\n\
         X-Content-Type-Options: nosniff\r\n\
         X-Frame-Options: DENY\r\n\
         Cache-Control: no-store\r\n\
         Connection: close\r\n\r\n",
        body.len()
    );

    let _ = stream.write_all(response_headers.as_bytes());
    if method == "GET" {
        let _ = stream.write_all(body);
    }
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

    for stream in listener.incoming() {
        if let Ok(stream) = stream {
            let snap = Arc::clone(&shared_snapshot);
            thread::spawn(move || {
                handle_client(stream, &snap);
            });
        }
    }

    Ok(())
}
