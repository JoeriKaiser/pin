mod assets;
mod doctor;
mod frontmatter;
mod model;
mod output;
mod search;
mod stats;
mod vault;
mod viewer;

use doctor::{emit_doctor_report, repair_vault, scan_vault};
use frontmatter::{
    get_default_title, parse_front_matter_detailed, render_full_document, truncate_title, Severity,
};
use model::{ArchiveFilter, IdeaMeta, Kind, OutputFormat, Priority, Resolution};
use output::{default_format, emit_context, emit_ideas, emit_single_idea};
use search::sort_search_results;
use stats::{calculate_stats, emit_stats};
use vault::{
    atomic_write, collect_ideas_filtered, generate_id, resolve_project, resolve_selector,
    resolve_vault_path,
};
use viewer::{create_snapshot, serve_view};

use std::env;
use std::fs;
use std::io::{self, IsTerminal, Read};
use std::path::{Path, PathBuf};
use std::process::{self, Command};

const VERSION: &str = "1.3.0";

fn print_usage() {
    print!(
        "Usage: pin <command> [options]\n\n\
         Commands:\n  \
         init --local [--project <name>] [--format json|plain]\n  \
         add <markdown> --kind technical|product|business|project\n                 \
         [--stdin] [--project <name>] [--title <title>]\n                 \
         [--tags <csv>] [--priority low|medium|high]\n                 \
         [--allow-duplicate] [--format json|plain]\n  \
         list [--project <name>] [--tag <name>] [--kind <kind>]\n       \
         [--archived|--all] [--format json|table|plain]\n  \
         list-project [--tag <name>] [--kind <kind>] [--archived|--all]\n               \
         [--format json|table|plain]\n  \
         search <query> [--project <name>] [--tag <name>] [--kind <kind>]\n                 \
         [--limit <n>] [--archived|--all]\n                 \
         [--format json|table|plain]\n  \
         context [--project <name>] [--kind <kind>] [--limit <n>]\n          \
         [--group kind] [--archived|--all] [--format json|plain]\n  \
         doctor [--repair] [--strict] [--format json|plain]\n  \
         archive <id|prefix|filename>\n          \
         [--resolution implemented|rejected|superseded|stale]\n          \
         [--note <text>] [--format json|plain]\n  \
         unarchive <id|prefix|filename> [--format json|plain]\n  \
         read <id|prefix|filename> [--format json|plain]\n  \
         edit <id|prefix|filename> [--format json|plain]\n  \
         rm <id|prefix|filename> [--format json|plain]\n  \
         import <directory> [--force] [--format json|plain]\n  \
         export <directory> [--force] [--format json|plain]\n  \
         stats [--format json|plain]\n  \
         view [--project <name>] [--tag <name>] [--kind <kind>]\n       \
         [--archived|--all] [--port <n>] [--no-open] [--format json|plain]\n  \
         view-project [--tag <name>] [--kind <kind>] [--archived|--all]\n               \
         [--port <n>] [--no-open] [--format json|plain]\n  \
         --help\n  \
         --version\n"
    );
}

struct ArgReader<'a> {
    args: &'a [String],
    idx: usize,
    command: &'a str,
}

impl<'a> ArgReader<'a> {
    fn new(args: &'a [String], command: &'a str) -> Self {
        Self {
            args,
            idx: 2,
            command,
        }
    }

    fn peek(&self) -> Option<&'a str> {
        self.args.get(self.idx).map(|s| s.as_str())
    }

    fn next_val(&mut self, flag: &str) -> &'a str {
        if self.idx + 1 >= self.args.len() {
            eprintln!("Error: {flag} requires a value");
            process::exit(1);
        }
        self.idx += 1;
        &self.args[self.idx]
    }

    fn parse_format(&mut self, allow_table: bool) -> OutputFormat {
        let val = self.next_val("--format");
        match val.parse::<OutputFormat>() {
            Ok(fmt) => {
                if !allow_table && fmt == OutputFormat::Table {
                    eprintln!(
                        "Error: --format must be json or plain for '{}'",
                        self.command
                    );
                    process::exit(1);
                }
                fmt
            }
            Err(_) => {
                if allow_table {
                    eprintln!(
                        "Error: --format must be json, table, or plain for '{}'",
                        self.command
                    );
                } else {
                    eprintln!(
                        "Error: --format must be json or plain for '{}'",
                        self.command
                    );
                }
                process::exit(1);
            }
        }
    }

    fn parse_selector(&mut self) -> &'a str {
        if self.args.len() < 3 {
            eprintln!(
                "Error: '{}' requires an ID, ID prefix, or filename",
                self.command
            );
            process::exit(1);
        }
        self.idx = 3;
        &self.args[2]
    }
}

fn read_stdin_content() -> Option<String> {
    if io::stdin().is_terminal() {
        return None;
    }
    let mut buffer = String::new();
    if io::stdin().read_to_string(&mut buffer).is_ok() && !buffer.is_empty() {
        Some(buffer)
    } else {
        None
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage();
        process::exit(1);
    }

    let cmd = &args[1];
    if matches!(cmd.as_str(), "--help" | "-h" | "help")
        || (args.len() > 2 && matches!(args[2].as_str(), "--help" | "-h"))
    {
        print_usage();
        return;
    }
    if matches!(cmd.as_str(), "--version" | "-V") {
        println!("pin {VERSION}");
        return;
    }

    let vault_path = resolve_vault_path();
    let mut reader = ArgReader::new(&args, cmd);

    match cmd.as_str() {
        "init" => {
            let (mut local, mut project, mut format) = (false, None, None);
            while let Some(arg) = reader.peek() {
                match arg {
                    "--local" => local = true,
                    "--project" => project = Some(reader.next_val("--project")),
                    "--format" => format = Some(reader.parse_format(false)),
                    _ => {
                        eprintln!("Error: Unknown flag '{arg}'");
                        process::exit(1);
                    }
                }
                reader.idx += 1;
            }

            if !local {
                eprintln!("Error: 'init' currently requires --local");
                process::exit(1);
            }

            let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let root = vault::find_repo_root(&cwd).unwrap_or(cwd);
            let local_vault = root.join(".pin_vault");

            if let Err(e) = fs::create_dir_all(&local_vault) {
                eprintln!("Error: Failed to create vault directory: {e}");
                process::exit(1);
            }
            let _ = fs::write(local_vault.join(".gitkeep"), "");

            let config_path = root.join(".pin-project");
            if !config_path.exists() {
                let name = project.unwrap_or_else(|| {
                    root.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("project")
                });
                let _ = fs::write(&config_path, format!("{name}\n"));
            }

            let fmt = format.unwrap_or_else(|| default_format(OutputFormat::Plain));
            if fmt == OutputFormat::Json {
                println!(
                    "{{\"vault\":\"{}\",\"scope\":\"local\"}}",
                    local_vault.display()
                );
            } else {
                println!("Initialized local vault at {}", local_vault.display());
            }
        }

        "add" => {
            let (mut content, mut project, mut title, mut tags) = (None, None, None, None);
            let (mut kind, mut priority, mut format) = (None, None, None);
            let (mut use_stdin, mut allow_duplicate) = (false, false);

            while let Some(arg) = reader.peek() {
                match arg {
                    "--stdin" => use_stdin = true,
                    "--allow-duplicate" => allow_duplicate = true,
                    "--project" => project = Some(reader.next_val("--project")),
                    "--title" => title = Some(reader.next_val("--title")),
                    "--tags" => tags = Some(reader.next_val("--tags")),
                    "--kind" => {
                        let val = reader.next_val("--kind");
                        match val.parse::<Kind>() {
                            Ok(k) if k != Kind::Unspecified => kind = Some(k),
                            _ => {
                                eprintln!("Error: --kind must be technical, product, business, or project");
                                process::exit(1);
                            }
                        }
                    }
                    "--priority" => {
                        let val = reader.next_val("--priority");
                        match val.parse::<Priority>() {
                            Ok(p) => priority = Some(p),
                            Err(_) => {
                                eprintln!("Error: --priority must be low, medium, or high");
                                process::exit(1);
                            }
                        }
                    }
                    "--format" => format = Some(reader.parse_format(false)),
                    _ if arg.starts_with("--") => {
                        eprintln!("Error: Unknown flag '{arg}'");
                        process::exit(1);
                    }
                    _ => {
                        if content.is_some() {
                            eprintln!("Error: Multiple content arguments provided");
                            process::exit(1);
                        }
                        content = Some(arg.to_string());
                    }
                }
                reader.idx += 1;
            }

            if use_stdin || content.is_none() {
                if let Some(stdin_content) = read_stdin_content() {
                    content = Some(stdin_content);
                }
            }

            let final_content = match content {
                Some(c) if !c.trim().is_empty() => c,
                _ => {
                    eprintln!("Error: Content argument is required for 'add'");
                    process::exit(1);
                }
            };

            let final_kind = match kind {
                Some(k) => k,
                None => {
                    eprintln!(
                        "Error: --kind is required (technical, product, business, or project)"
                    );
                    process::exit(1);
                }
            };

            let proj_name = resolve_project(project);
            let title_val = match title {
                Some(t) => truncate_title(t),
                None => get_default_title(&final_content).unwrap_or_default(),
            };

            if title_val.trim().is_empty() {
                eprintln!("Error: Could not determine a non-empty title");
                process::exit(1);
            }

            if !allow_duplicate {
                if let Ok(existing_ideas) = collect_ideas_filtered(
                    &vault_path,
                    Some(&proj_name),
                    None,
                    None,
                    None,
                    ArchiveFilter::Active,
                ) {
                    for existing in existing_ideas {
                        if existing.title.eq_ignore_ascii_case(&title_val) {
                            eprintln!(
                                "Error: An idea titled '{title_val}' already exists for project '{proj_name}' ({}). Use --allow-duplicate to add it anyway.",
                                existing.id
                            );
                            process::exit(1);
                        }
                    }
                }
            }

            let id = generate_id();
            let now = chrono::Utc::now();
            let filename = format!("{id}.md");
            let file_path = vault_path.join(&filename);

            let idea = IdeaMeta {
                schema: Some(1),
                id: id.clone(),
                project: proj_name,
                kind: final_kind,
                timestamp: now.timestamp(),
                created_at_ns: now.timestamp_nanos_opt(),
                title: title_val,
                tags: tags.map(|s| s.to_string()),
                priority,
                archived_at: None,
                resolution: None,
                resolution_note: None,
                filename,
                body: final_content,
                score: None,
                raw_frontmatter_map: serde_yaml::Mapping::new(),
            };

            if let Err(e) = atomic_write(&file_path, &render_full_document(&idea)) {
                eprintln!("Error: Failed to save idea: {e}");
                process::exit(1);
            }

            let fmt = format.unwrap_or_else(|| default_format(OutputFormat::Plain));
            emit_single_idea(&idea, fmt);
        }

        "list" | "list-project" => {
            let project_scoped = cmd == "list-project";
            let mut filter_project = if project_scoped {
                Some(resolve_project(None))
            } else {
                None
            };
            let (mut filter_tag, mut filter_kind) = (None, None);
            let (mut archive_filter, mut format) = (ArchiveFilter::Active, None);

            while let Some(arg) = reader.peek() {
                match arg {
                    "--archived" => archive_filter = ArchiveFilter::Archived,
                    "--all" => archive_filter = ArchiveFilter::All,
                    "--project" => {
                        if project_scoped {
                            eprintln!("Error: 'list-project' does not accept --project");
                            process::exit(1);
                        }
                        filter_project = Some(reader.next_val("--project").to_string());
                    }
                    "--tag" => filter_tag = Some(reader.next_val("--tag").to_string()),
                    "--kind" => {
                        let val = reader.next_val("--kind");
                        filter_kind = Some(val.parse::<Kind>().unwrap_or_else(|_| {
                            eprintln!("Error: Unknown kind '{val}'");
                            process::exit(1);
                        }));
                    }
                    "--format" => format = Some(reader.parse_format(true)),
                    _ => {
                        eprintln!("Error: Unknown flag '{arg}'");
                        process::exit(1);
                    }
                }
                reader.idx += 1;
            }

            let ideas = collect_ideas_filtered(
                &vault_path,
                filter_project.as_deref(),
                filter_tag.as_deref(),
                filter_kind,
                None,
                archive_filter,
            )
            .unwrap_or_default();

            let fmt = format.unwrap_or_else(|| default_format(OutputFormat::Table));
            emit_ideas(&ideas, fmt);
        }

        "search" => {
            if args.len() < 3 {
                eprintln!("Error: 'search' subcommand requires a query argument");
                process::exit(1);
            }
            let query = &args[2];
            reader.idx = 3;

            let (mut filter_project, mut filter_tag, mut filter_kind) = (None, None, None);
            let (mut archive_filter, mut limit, mut format) = (ArchiveFilter::Active, None, None);

            while let Some(arg) = reader.peek() {
                match arg {
                    "--archived" => archive_filter = ArchiveFilter::Archived,
                    "--all" => archive_filter = ArchiveFilter::All,
                    "--project" => filter_project = Some(reader.next_val("--project").to_string()),
                    "--tag" => filter_tag = Some(reader.next_val("--tag").to_string()),
                    "--kind" => {
                        let val = reader.next_val("--kind");
                        filter_kind = Some(val.parse::<Kind>().unwrap_or_else(|_| {
                            eprintln!("Error: Unknown kind '{val}'");
                            process::exit(1);
                        }));
                    }
                    "--limit" => {
                        let val = reader.next_val("--limit");
                        limit = Some(val.parse::<usize>().unwrap_or_else(|_| {
                            eprintln!("Error: --limit requires a positive integer");
                            process::exit(1);
                        }));
                    }
                    "--format" => format = Some(reader.parse_format(true)),
                    _ => {
                        eprintln!("Error: Unknown flag '{arg}'");
                        process::exit(1);
                    }
                }
                reader.idx += 1;
            }

            let mut ideas = collect_ideas_filtered(
                &vault_path,
                filter_project.as_deref(),
                filter_tag.as_deref(),
                filter_kind,
                Some(query),
                archive_filter,
            )
            .unwrap_or_default();

            sort_search_results(&mut ideas);
            if let Some(lim) = limit {
                ideas.truncate(lim);
            }

            let fmt = format.unwrap_or_else(|| default_format(OutputFormat::Table));
            emit_ideas(&ideas, fmt);
        }

        "context" => {
            let (mut filter_project, mut filter_kind, mut limit) = (None, None, None);
            let (mut archive_filter, mut group_kind, mut format) =
                (ArchiveFilter::Active, false, None);

            while let Some(arg) = reader.peek() {
                match arg {
                    "--archived" => archive_filter = ArchiveFilter::Archived,
                    "--all" => archive_filter = ArchiveFilter::All,
                    "--project" => filter_project = Some(reader.next_val("--project").to_string()),
                    "--kind" => {
                        let val = reader.next_val("--kind");
                        filter_kind = Some(val.parse::<Kind>().unwrap_or_else(|_| {
                            eprintln!("Error: Unknown kind '{val}'");
                            process::exit(1);
                        }));
                    }
                    "--limit" => {
                        let val = reader.next_val("--limit");
                        limit = Some(val.parse::<usize>().unwrap_or_else(|_| {
                            eprintln!("Error: --limit requires a positive integer");
                            process::exit(1);
                        }));
                    }
                    "--group" => {
                        let val = reader.next_val("--group");
                        if val == "kind" {
                            group_kind = true;
                        } else {
                            eprintln!("Error: --group only supports 'kind'");
                            process::exit(1);
                        }
                    }
                    "--format" => format = Some(reader.parse_format(false)),
                    _ => {
                        eprintln!("Error: Unknown flag '{arg}'");
                        process::exit(1);
                    }
                }
                reader.idx += 1;
            }

            let project = resolve_project(filter_project.as_deref());
            let mut ideas = collect_ideas_filtered(
                &vault_path,
                Some(&project),
                None,
                filter_kind,
                None,
                archive_filter,
            )
            .unwrap_or_default();

            ideas.sort_by(|a, b| {
                let p_cmp = b.priority_rank().cmp(&a.priority_rank());
                if p_cmp != std::cmp::Ordering::Equal {
                    p_cmp
                } else {
                    b.timestamp.cmp(&a.timestamp)
                }
            });

            let fmt = format.unwrap_or_else(|| default_format(OutputFormat::Plain));
            emit_context(&ideas, &project, group_kind, archive_filter, limit, fmt);
        }

        "doctor" => {
            let (mut repair, mut strict, mut format) = (false, false, None);
            while let Some(arg) = reader.peek() {
                match arg {
                    "--repair" => repair = true,
                    "--strict" => strict = true,
                    "--format" => format = Some(reader.parse_format(false)),
                    _ => {
                        eprintln!("Error: Unknown flag '{arg}'");
                        process::exit(1);
                    }
                }
                reader.idx += 1;
            }

            let repaired_count = if repair { repair_vault(&vault_path) } else { 0 };
            let scan = scan_vault(&vault_path);
            let fmt = format.unwrap_or_else(|| default_format(OutputFormat::Plain));
            emit_doctor_report(&vault_path, &scan, repaired_count, fmt);

            let error_count = scan
                .issues
                .iter()
                .filter(|i| i.severity == Severity::Error)
                .count();
            let warning_count = scan
                .issues
                .iter()
                .filter(|i| i.severity == Severity::Warning)
                .count();

            if error_count > 0 || (strict && warning_count > 0) {
                process::exit(1);
            }
        }

        "archive" => {
            let selector = reader.parse_selector();
            let (mut resolution, mut note, mut format) = (Resolution::Implemented, None, None);

            while let Some(arg) = reader.peek() {
                match arg {
                    "--resolution" => {
                        let val = reader.next_val("--resolution");
                        resolution = val.parse::<Resolution>().unwrap_or_else(|_| {
                            eprintln!("Error: --resolution must be implemented, rejected, superseded, or stale");
                            process::exit(1);
                        });
                    }
                    "--note" => note = Some(reader.next_val("--note").to_string()),
                    "--format" => format = Some(reader.parse_format(false)),
                    _ => {
                        eprintln!("Error: Unknown flag '{arg}'");
                        process::exit(1);
                    }
                }
                reader.idx += 1;
            }

            let filename = resolve_selector(&vault_path, selector).unwrap_or_else(|e| {
                eprintln!("Error: {e}");
                process::exit(1);
            });
            let path = vault_path.join(&filename);
            let content = fs::read_to_string(&path).unwrap_or_else(|e| {
                eprintln!("Error: Could not read file: {e}");
                process::exit(1);
            });

            let mut issues = Vec::new();
            if let Some(mut meta) = parse_front_matter_detailed(&filename, &content, &mut issues) {
                meta.archived_at = Some(chrono::Utc::now().timestamp());
                meta.resolution = Some(resolution);
                meta.resolution_note = note;

                if let Err(e) = atomic_write(&path, &render_full_document(&meta)) {
                    eprintln!("Error: Failed to save archived idea: {e}");
                    process::exit(1);
                }

                let fmt = format.unwrap_or_else(|| default_format(OutputFormat::Plain));
                if fmt == OutputFormat::Json {
                    #[derive(serde::Serialize)]
                    struct ArchiveJson<'a> {
                        archived: &'a str,
                        filename: &'a str,
                        resolution: &'a str,
                    }
                    let res = ArchiveJson {
                        archived: &meta.id,
                        filename: &filename,
                        resolution: resolution.as_str(),
                    };
                    println!("{}", serde_json::to_string(&res).unwrap_or_default());
                } else {
                    println!("Archived {}  {}", meta.id, filename);
                }
            } else {
                eprintln!("Error: Invalid front matter in file");
                process::exit(1);
            }
        }

        "unarchive" => {
            let selector = reader.parse_selector();
            let mut format = None;

            while let Some(arg) = reader.peek() {
                match arg {
                    "--format" => format = Some(reader.parse_format(false)),
                    _ => {
                        eprintln!("Error: Unknown flag '{arg}'");
                        process::exit(1);
                    }
                }
                reader.idx += 1;
            }

            let filename = resolve_selector(&vault_path, selector).unwrap_or_else(|e| {
                eprintln!("Error: {e}");
                process::exit(1);
            });
            let path = vault_path.join(&filename);
            let content = fs::read_to_string(&path).unwrap_or_else(|e| {
                eprintln!("Error: Could not read file: {e}");
                process::exit(1);
            });

            let mut issues = Vec::new();
            if let Some(mut meta) = parse_front_matter_detailed(&filename, &content, &mut issues) {
                meta.archived_at = None;
                meta.resolution = None;
                meta.resolution_note = None;

                if let Err(e) = atomic_write(&path, &render_full_document(&meta)) {
                    eprintln!("Error: Failed to save unarchived idea: {e}");
                    process::exit(1);
                }

                let fmt = format.unwrap_or_else(|| default_format(OutputFormat::Plain));
                if fmt == OutputFormat::Json {
                    #[derive(serde::Serialize)]
                    struct UnarchiveJson<'a> {
                        unarchived: &'a str,
                        filename: &'a str,
                    }
                    let res = UnarchiveJson {
                        unarchived: &meta.id,
                        filename: &filename,
                    };
                    println!("{}", serde_json::to_string(&res).unwrap_or_default());
                } else {
                    println!("Unarchived {}  {}", meta.id, filename);
                }
            } else {
                eprintln!("Error: Invalid front matter in file");
                process::exit(1);
            }
        }

        "read" => {
            let selector = reader.parse_selector();
            let mut format = None;

            while let Some(arg) = reader.peek() {
                match arg {
                    "--format" => format = Some(reader.parse_format(false)),
                    _ => {
                        eprintln!("Error: Unexpected argument '{arg}'");
                        process::exit(1);
                    }
                }
                reader.idx += 1;
            }

            let filename = resolve_selector(&vault_path, selector).unwrap_or_else(|e| {
                eprintln!("Error: {e}");
                process::exit(1);
            });
            let path = vault_path.join(&filename);
            let content = fs::read_to_string(&path).unwrap_or_else(|e| {
                eprintln!("Error: Could not read file: {e}");
                process::exit(1);
            });

            let fmt = format.unwrap_or(OutputFormat::Plain);
            if fmt == OutputFormat::Json {
                #[derive(serde::Serialize)]
                struct ReadJson<'a> {
                    filename: &'a str,
                    content: &'a str,
                }
                let res = ReadJson {
                    filename: &filename,
                    content: &content,
                };
                println!("{}", serde_json::to_string(&res).unwrap_or_default());
            } else {
                print!("{content}");
                if !content.ends_with('\n') {
                    println!();
                }
            }
        }

        "edit" => {
            let selector = reader.parse_selector();
            let mut format = None;

            while let Some(arg) = reader.peek() {
                match arg {
                    "--format" => format = Some(reader.parse_format(false)),
                    _ => {
                        eprintln!("Error: Unexpected argument '{arg}'");
                        process::exit(1);
                    }
                }
                reader.idx += 1;
            }

            let filename = resolve_selector(&vault_path, selector).unwrap_or_else(|e| {
                eprintln!("Error: {e}");
                process::exit(1);
            });
            let path = vault_path.join(&filename);
            let original_content = fs::read_to_string(&path).unwrap_or_else(|e| {
                eprintln!("Error: Could not read file: {e}");
                process::exit(1);
            });

            let mut temp_issues = Vec::new();
            let original_meta =
                parse_front_matter_detailed(&filename, &original_content, &mut temp_issues);
            let edited_id = original_meta
                .map(|m| m.id)
                .unwrap_or_else(|| filename.clone());

            let temp_edit_file = tempfile::Builder::new()
                .prefix("pin-edit-")
                .suffix(".md")
                .tempfile()
                .unwrap_or_else(|e| {
                    eprintln!("Error: Could not create temp edit file: {e}");
                    process::exit(1);
                });

            if let Err(e) = fs::write(temp_edit_file.path(), &original_content) {
                eprintln!("Error: Could not prepare edit file: {e}");
                process::exit(1);
            }

            let editor = env::var("EDITOR")
                .or_else(|_| env::var("VISUAL"))
                .unwrap_or_else(|_| {
                    if cfg!(target_os = "windows") {
                        "notepad.exe".to_string()
                    } else {
                        "nano".to_string()
                    }
                });

            let edit_path_str = temp_edit_file.path().to_string_lossy().to_string();
            let mut editor_parts = editor.split_whitespace();
            let editor_cmd = editor_parts.next().unwrap_or("nano");
            let mut editor_args: Vec<&str> = editor_parts.collect();
            editor_args.push(&edit_path_str);

            let status = Command::new(editor_cmd).args(&editor_args).status();
            match status {
                Ok(s) if s.success() => {}
                _ => {
                    eprintln!("Error: Editor exited with non-zero status");
                    process::exit(1);
                }
            }

            let edited_content = fs::read_to_string(temp_edit_file.path()).unwrap_or_else(|e| {
                eprintln!("Error: Could not read edited file: {e}");
                process::exit(1);
            });

            let mut issues = Vec::new();
            let parsed = parse_front_matter_detailed(&filename, &edited_content, &mut issues);
            let has_errors = issues.iter().any(|i| i.severity == Severity::Error);

            if parsed.is_none() || has_errors {
                let recovery_filename = format!(".{edited_id}.edit-recovery.tmp");
                let recovery_path = vault_path.join(&recovery_filename);
                let _ = fs::write(&recovery_path, &edited_content);
                let _ = atomic_write(&path, &original_content);
                eprintln!("Error: Front matter is invalid after editing. Saved recovery to '{recovery_filename}'");
                process::exit(1);
            }

            if let Err(e) = atomic_write(&path, &edited_content) {
                eprintln!("Error: Failed to save edited proposal: {e}");
                process::exit(1);
            }

            let fmt = format.unwrap_or_else(|| default_format(OutputFormat::Plain));
            if fmt == OutputFormat::Json {
                #[derive(serde::Serialize)]
                struct EditJson<'a> {
                    edited: &'a str,
                    filename: &'a str,
                }
                let res = EditJson {
                    edited: &edited_id,
                    filename: &filename,
                };
                println!("{}", serde_json::to_string(&res).unwrap_or_default());
            } else {
                println!("Saved changes to {filename}");
            }
        }

        "rm" => {
            let selector = reader.parse_selector();
            let mut format = None;

            while let Some(arg) = reader.peek() {
                match arg {
                    "--format" => format = Some(reader.parse_format(false)),
                    _ => {
                        eprintln!("Error: Unexpected argument '{arg}'");
                        process::exit(1);
                    }
                }
                reader.idx += 1;
            }

            let filename = resolve_selector(&vault_path, selector).unwrap_or_else(|e| {
                eprintln!("Error: {e}");
                process::exit(1);
            });
            let path = vault_path.join(&filename);
            let mut removed_id = filename.clone();
            if let Ok(c) = fs::read_to_string(&path) {
                let mut issues = Vec::new();
                if let Some(meta) = parse_front_matter_detailed(&filename, &c, &mut issues) {
                    removed_id = meta.id;
                }
            }

            if let Err(e) = fs::remove_file(&path) {
                eprintln!("Error: Failed to delete idea: {e}");
                process::exit(1);
            }

            let fmt = format.unwrap_or_else(|| default_format(OutputFormat::Plain));
            if fmt == OutputFormat::Json {
                #[derive(serde::Serialize)]
                struct RmJson<'a> {
                    removed: &'a str,
                    filename: &'a str,
                }
                let res = RmJson {
                    removed: &removed_id,
                    filename: &filename,
                };
                println!("{}", serde_json::to_string(&res).unwrap_or_default());
            } else {
                println!("Removed {removed_id}  {filename}");
            }
        }

        "import" | "export" => {
            if args.len() < 3 {
                eprintln!("Error: '{cmd}' requires a directory");
                process::exit(1);
            }
            let target_path_str = &args[2];
            reader.idx = 3;

            let (mut force, mut format) = (false, None);
            while let Some(arg) = reader.peek() {
                match arg {
                    "--force" => force = true,
                    "--format" => format = Some(reader.parse_format(false)),
                    _ => {
                        eprintln!("Error: Unknown flag '{arg}'");
                        process::exit(1);
                    }
                }
                reader.idx += 1;
            }

            let is_import = cmd == "import";
            let target_dir = Path::new(target_path_str);

            if is_import {
                if !target_dir.is_dir() {
                    eprintln!("Error: Source directory not found");
                    process::exit(1);
                }

                let entries = fs::read_dir(target_dir).unwrap_or_else(|e| {
                    eprintln!("Error: Failed to read source directory: {e}");
                    process::exit(1);
                });

                let mut dir_entries: Vec<_> = entries.flatten().collect();
                dir_entries.sort_by_key(|e| e.file_name());

                let mut valid_entries = Vec::new();
                for entry in dir_entries {
                    let path = entry.path();
                    if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("md") {
                        let filename = path
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or("")
                            .to_string();
                        let content = fs::read_to_string(&path).unwrap_or_else(|_| {
                            eprintln!("Error: '{filename}' is not a valid pin file");
                            process::exit(1);
                        });

                        let mut issues = Vec::new();
                        let parsed = parse_front_matter_detailed(&filename, &content, &mut issues);
                        let has_errors = issues.iter().any(|i| i.severity == Severity::Error);

                        if parsed.is_none() || has_errors {
                            eprintln!("Error: '{filename}' is not a valid pin file");
                            process::exit(1);
                        }
                        valid_entries.push((path, filename));
                    }
                }

                if let Err(e) = fs::create_dir_all(&vault_path) {
                    eprintln!("Error: Failed to create vault directory: {e}");
                    process::exit(1);
                }

                let (mut copied, mut skipped) = (0, 0);
                for (src_path, filename) in valid_entries {
                    let dest_path = vault_path.join(&filename);
                    if dest_path.exists() && !force {
                        skipped += 1;
                        continue;
                    }
                    if fs::copy(&src_path, &dest_path).is_ok() {
                        copied += 1;
                    }
                }

                let fmt = format.unwrap_or_else(|| default_format(OutputFormat::Plain));
                if fmt == OutputFormat::Json {
                    #[derive(serde::Serialize)]
                    struct TransferJson<'a> {
                        operation: &'a str,
                        copied: usize,
                        skipped: usize,
                    }
                    let res = TransferJson {
                        operation: "import",
                        copied,
                        skipped,
                    };
                    println!("{}", serde_json::to_string(&res).unwrap_or_default());
                } else {
                    println!("import: {copied} copied, {skipped} skipped");
                }
            } else {
                if let Err(e) = fs::create_dir_all(target_dir) {
                    eprintln!("Error: Failed to create export directory: {e}");
                    process::exit(1);
                }

                let (mut copied, mut skipped) = (0, 0);
                if let Ok(entries) = fs::read_dir(&vault_path) {
                    for entry in entries.flatten() {
                        let src_path = entry.path();
                        if src_path.is_file()
                            && src_path.extension().and_then(|s| s.to_str()) == Some("md")
                        {
                            if let Some(filename) = src_path.file_name() {
                                let dest_path = target_dir.join(filename);
                                if dest_path.exists() && !force {
                                    skipped += 1;
                                    continue;
                                }
                                if fs::copy(&src_path, &dest_path).is_ok() {
                                    copied += 1;
                                }
                            }
                        }
                    }
                }

                let fmt = format.unwrap_or_else(|| default_format(OutputFormat::Plain));
                if fmt == OutputFormat::Json {
                    #[derive(serde::Serialize)]
                    struct TransferJson<'a> {
                        operation: &'a str,
                        copied: usize,
                        skipped: usize,
                    }
                    let res = TransferJson {
                        operation: "export",
                        copied,
                        skipped,
                    };
                    println!("{}", serde_json::to_string(&res).unwrap_or_default());
                } else {
                    println!("export: {copied} copied, {skipped} skipped");
                }
            }
        }

        "stats" => {
            let mut format = None;
            while let Some(arg) = reader.peek() {
                match arg {
                    "--format" => format = Some(reader.parse_format(false)),
                    _ => {
                        eprintln!("Error: Unexpected argument '{arg}'");
                        process::exit(1);
                    }
                }
                reader.idx += 1;
            }

            let stats = calculate_stats(&vault_path);
            let fmt = format.unwrap_or_else(|| default_format(OutputFormat::Plain));
            emit_stats(&stats, fmt);
        }

        "view" | "view-project" => {
            let project_scoped = cmd == "view-project";
            let mut filter_project = if project_scoped {
                Some(resolve_project(None))
            } else {
                None
            };
            let (mut filter_tag, mut filter_kind) = (None, None);
            let (mut archive_filter, mut port, mut no_open, mut format) =
                (ArchiveFilter::Active, 0, false, None);

            while let Some(arg) = reader.peek() {
                match arg {
                    "--archived" => archive_filter = ArchiveFilter::Archived,
                    "--all" => archive_filter = ArchiveFilter::All,
                    "--no-open" => no_open = true,
                    "--project" => {
                        if project_scoped {
                            eprintln!("Error: 'view-project' does not accept --project");
                            process::exit(1);
                        }
                        filter_project = Some(reader.next_val("--project").to_string());
                    }
                    "--tag" => filter_tag = Some(reader.next_val("--tag").to_string()),
                    "--kind" => {
                        let val = reader.next_val("--kind");
                        filter_kind = Some(val.parse::<Kind>().unwrap_or_else(|_| {
                            eprintln!("Error: Unknown kind '{val}'");
                            process::exit(1);
                        }));
                    }
                    "--port" => {
                        let val = reader.next_val("--port");
                        port = val.parse::<u16>().unwrap_or_else(|_| {
                            eprintln!("Error: --port requires an integer port (0-65535)");
                            process::exit(1);
                        });
                    }
                    "--format" => format = Some(reader.parse_format(false)),
                    _ => {
                        eprintln!("Error: Unknown flag '{arg}'");
                        process::exit(1);
                    }
                }
                reader.idx += 1;
            }

            if !io::stdout().is_terminal() && !no_open && format.is_none() {
                eprintln!("Error: 'view' must be run in an interactive terminal, or with --no-open and an explicit format");
                process::exit(1);
            }

            let scope_label = filter_project.as_deref().unwrap_or("all");
            let ideas = collect_ideas_filtered(
                &vault_path,
                filter_project.as_deref(),
                filter_tag.as_deref(),
                filter_kind,
                None,
                archive_filter,
            )
            .unwrap_or_default();

            let snapshot = create_snapshot(&ideas, scope_label, archive_filter);
            let fmt = format.unwrap_or(OutputFormat::Plain);

            if let Err(e) = serve_view(snapshot, port, no_open, fmt) {
                eprintln!("Error: Failed to serve view: {e}");
                process::exit(1);
            }
        }

        unknown => {
            eprintln!("Error: Unknown command '{unknown}'");
            print_usage();
            process::exit(1);
        }
    }
}
