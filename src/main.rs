mod assets;
mod doctor;
mod frontmatter;
mod model;
mod output;
mod search;
mod stats;
mod vault;
mod viewer;
mod workflow;

use doctor::{emit_doctor_report, repair_vault, scan_vault, upgrade_vault};
use frontmatter::{
    get_default_title, parse_front_matter_detailed, render_full_document, truncate_title, Severity,
};
use model::{ArchiveFilter, IdeaMeta, Kind, OutputFormat, Priority, Resolution, Status, WorkType};
use output::{default_format, emit_context, emit_ideas, emit_single_idea};
use search::sort_search_results;
use stats::{calculate_stats, emit_stats};
use vault::{
    atomic_write, collect_ideas_with_filter, generate_id, resolve_project, resolve_selector,
    resolve_vault_path, FilterOptions,
};
use viewer::{create_snapshot, serve_view};

use std::env;
use std::fs;
use std::io::{self, IsTerminal, Read};
use std::path::{Path, PathBuf};
use std::process::{self, Command};

const VERSION: &str = "2.0.0";

fn print_usage() {
    print!(
        "Usage: pin <command> [options]\n\n\
         Commands:\n  \
         init --local [--project <name>] [--format json|plain]\n  \
         add <markdown> [--kind <kind>] [--type task|bug|idea|decision]\n                 \
         [--status <status>] [--actor <name>] [--stdin] [--project <name>]\n                 \
         [--title <title>] [--tags <csv>] [--priority low|medium|high]\n                 \
         [--allow-duplicate] [--format json|plain]\n  \
         list [--project <name>] [--tag <name>] [--kind <kind>] [--type <type>]\n       \
         [--status <status>] [--claimed-by <actor>] [--ready]\n       \
         [--archived|--all] [--format json|table|plain]\n  \
         list-project [--tag <name>] [--kind <kind>] [--type <type>]\n               \
         [--status <status>] [--claimed-by <actor>] [--ready] [--archived|--all]\n               \
         [--format json|table|plain]\n  \
         search <query> [--project <name>] [--tag <name>] [--kind <kind>]\n                 \
         [--type <type>] [--status <status>] [--claimed-by <actor>]\n                 \
         [--limit <n>] [--archived|--all] [--format json|table|plain]\n  \
         context [--project <name>] [--kind <kind>] [--type <type>] [--status <status>]\n          \
         [--limit <n>] [--group kind] [--archived|--all] [--format json|plain]\n  \
         next [--project <name>] [--limit <n>] [--format json|plain]\n  \
         transition <id|prefix|filename> --to <status> [--actor <name>]\n              \
         [--note <text>] [--expect-revision <n>] [--format json|plain]\n  \
         claim <id|prefix|filename> [--actor <name>] [--lease <seconds>]\n        \
         [--expect-revision <n>] [--format json|plain]\n  \
         release <id|prefix|filename> [--actor <name>] [--force]\n          \
         [--expect-revision <n>] [--format json|plain]\n  \
         handoff <id|prefix|filename> [--actor <name>] [--progress <text>]\n          \
         [--next <text>] [--blocker <text>] [--verification <text>]\n          \
         [--expect-revision <n>] [--format json|plain]\n  \
         complete <id|prefix|filename> --evidence <text> [--actor <name>]\n           \
         [--expect-revision <n>] [--format json|plain]\n  \
         close <id|prefix|filename> [--actor <name>] [--note <text>]\n        \
         [--expect-revision <n>] [--format json|plain]\n  \
         depend <id|prefix|filename> <dependency-id> [--actor <name>]\n         \
         [--expect-revision <n>] [--format json|plain]\n  \
         parent <id|prefix|filename> <parent-id> [--actor <name>]\n         \
         [--expect-revision <n>] [--format json|plain]\n  \
         relate <id|prefix|filename> <related-id> [--actor <name>]\n         \
         [--expect-revision <n>] [--format json|plain]\n  \
         doctor [--repair] [--upgrade] [--strict] [--format json|plain]\n  \
         archive <id|prefix|filename>\n          \
         [--resolution implemented|rejected|superseded|stale]\n          \
         [--note <text>] [--format json|plain]\n  \
         unarchive <id|prefix|filename> [--format json|plain]\n  \
         read <id|prefix|filename> [--format json|plain]\n  \
         edit <id|prefix|filename> [--format json|plain]\n  \
         rm <id|prefix|filename> [--format json|plain]\n  \
         import <directory> [--force] [--format json|plain]\n  \
         export <directory> [--force] [--format json|plain]\n  \
         stats [--project <name>] [--format json|plain]\n  \
         view [--project <name>] [--tag <name>] [--kind <kind>] [--type <type>]\n       \
         [--status <status>] [--archived|--all] [--port <n>] [--no-open] [--format json|plain]\n  \
         view-project [--tag <name>] [--kind <kind>] [--type <type>] [--status <status>]\n               \
         [--archived|--all] [--port <n>] [--no-open] [--format json|plain]\n  \
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
        let arg = self.args.get(self.idx).map(|s| s.as_str())?;
        if arg == "--help" || arg == "-h" {
            print_usage();
            process::exit(0);
        }
        Some(arg)
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
        if self.args[2] == "--help" || self.args[2] == "-h" {
            print_usage();
            process::exit(0);
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

fn resolve_actor(explicit: Option<&str>) -> String {
    if let Some(a) = explicit {
        let trimmed = a.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    if let Ok(val) = env::var("PIN_ACTOR") {
        let trimmed = val.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    if io::stdout().is_terminal() {
        "human:cli".to_string()
    } else {
        "agent:default".to_string()
    }
}

fn split_shell_words(cmd: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut quote_char = '"';

    for c in cmd.chars() {
        match c {
            '"' | '\'' if !in_quotes => {
                in_quotes = true;
                quote_char = c;
            }
            c if in_quotes && c == quote_char => {
                in_quotes = false;
            }
            c if c.is_whitespace() && !in_quotes => {
                if !cur.is_empty() {
                    words.push(cur.clone());
                    cur.clear();
                }
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    words
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage();
        process::exit(0);
    }

    let cmd = &args[1];
    if cmd == "--help" || cmd == "-h" {
        print_usage();
        process::exit(0);
    }
    if cmd == "--version" || cmd == "-v" {
        println!("pin {VERSION}");
        process::exit(0);
    }

    let vault_path = resolve_vault_path();
    let mut reader = ArgReader::new(&args, cmd);

    match cmd.as_str() {
        "init" => {
            let (mut is_local, mut project, mut format) = (false, None, None);
            while let Some(arg) = reader.peek() {
                match arg {
                    "--local" => is_local = true,
                    "--project" => project = Some(reader.next_val("--project").to_string()),
                    "--format" => format = Some(reader.parse_format(false)),
                    _ => {
                        eprintln!("Error: Unknown flag '{arg}'");
                        process::exit(1);
                    }
                }
                reader.idx += 1;
            }

            if !is_local {
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

            let gitignore_path = local_vault.join(".gitignore");
            if !gitignore_path.exists() {
                let _ = fs::write(&gitignore_path, "*.lock\n.*.lock\n.*.edit-recovery.tmp\n");
            }

            let config_path = root.join(".pin-project");
            if !config_path.exists() {
                let name = project.unwrap_or_else(|| {
                    root.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("project")
                        .to_string()
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
            let (mut kind, mut item_type, mut status, mut actor, mut priority, mut format) =
                (None, None, None, None, None, None);
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
                    "--type" => {
                        let val = reader.next_val("--type");
                        match val.parse::<WorkType>() {
                            Ok(t) => item_type = Some(t),
                            Err(e) => {
                                eprintln!("Error: {e}");
                                process::exit(1);
                            }
                        }
                    }
                    "--status" => {
                        let val = reader.next_val("--status");
                        match val.parse::<Status>() {
                            Ok(s) => status = Some(s),
                            Err(e) => {
                                eprintln!("Error: {e}");
                                process::exit(1);
                            }
                        }
                    }
                    "--actor" => actor = Some(reader.next_val("--actor")),
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
                    if let Some(t) = item_type {
                        match t {
                            WorkType::Task | WorkType::Bug => Kind::Technical,
                            WorkType::Idea | WorkType::Decision => {
                                eprintln!("Error: --kind is required (technical, product, business, or project)");
                                process::exit(1);
                            }
                        }
                    } else {
                        eprintln!(
                            "Error: --kind is required (technical, product, business, or project)"
                        );
                        process::exit(1);
                    }
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
                let filter = FilterOptions {
                    project: Some(&proj_name),
                    archive_filter: ArchiveFilter::Active,
                    ..Default::default()
                };
                if let Ok(existing_ideas) = collect_ideas_with_filter(&vault_path, &filter) {
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
            let filename = format!("{id}.md");
            let file_path = vault_path.join(&filename);
            let creator = resolve_actor(actor);

            let resolved_type = item_type.unwrap_or(WorkType::Task);
            let final_status = status.unwrap_or(Status::Created);

            let idea = IdeaMeta::new_work_item(
                id.clone(),
                proj_name,
                title_val,
                final_content,
                final_kind,
                resolved_type,
                final_status,
                priority,
                tags.map(|s| s.to_string()),
                Some(creator),
            );

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
            let (
                mut filter_tag,
                mut filter_kind,
                mut filter_type,
                mut filter_status,
                mut filter_claimed_by,
            ) = (None, None, None, None, None);
            let (mut filter_ready, mut archive_filter, mut format) =
                (false, ArchiveFilter::Active, None);

            while let Some(arg) = reader.peek() {
                match arg {
                    "--archived" => archive_filter = ArchiveFilter::Archived,
                    "--all" => archive_filter = ArchiveFilter::All,
                    "--ready" => filter_ready = true,
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
                    "--type" => {
                        let val = reader.next_val("--type");
                        filter_type = Some(val.parse::<WorkType>().unwrap_or_else(|e| {
                            eprintln!("Error: {e}");
                            process::exit(1);
                        }));
                    }
                    "--status" => {
                        let val = reader.next_val("--status");
                        filter_status = Some(val.parse::<Status>().unwrap_or_else(|e| {
                            eprintln!("Error: {e}");
                            process::exit(1);
                        }));
                    }
                    "--claimed-by" => {
                        filter_claimed_by = Some(reader.next_val("--claimed-by").to_string())
                    }
                    "--format" => format = Some(reader.parse_format(true)),
                    _ => {
                        eprintln!("Error: Unknown flag '{arg}'");
                        process::exit(1);
                    }
                }
                reader.idx += 1;
            }

            let filter = FilterOptions {
                project: filter_project.as_deref(),
                tag: filter_tag.as_deref(),
                kind: filter_kind,
                item_type: filter_type,
                status: filter_status,
                claimed_by: filter_claimed_by.as_deref(),
                ready: filter_ready,
                query: None,
                archive_filter,
            };

            let ideas = collect_ideas_with_filter(&vault_path, &filter).unwrap_or_default();
            let fmt = format.unwrap_or_else(|| default_format(OutputFormat::Table));
            emit_ideas(&ideas, fmt);
        }

        "search" => {
            if args.len() < 3 {
                eprintln!("Error: 'search' subcommand requires a query argument");
                process::exit(1);
            }
            if args[2] == "--help" || args[2] == "-h" {
                print_usage();
                process::exit(0);
            }
            let query = &args[2];
            reader.idx = 3;

            let (
                mut filter_project,
                mut filter_tag,
                mut filter_kind,
                mut filter_type,
                mut filter_status,
                mut filter_claimed_by,
            ) = (None, None, None, None, None, None);
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
                    "--type" => {
                        let val = reader.next_val("--type");
                        filter_type = Some(val.parse::<WorkType>().unwrap_or_else(|e| {
                            eprintln!("Error: {e}");
                            process::exit(1);
                        }));
                    }
                    "--status" => {
                        let val = reader.next_val("--status");
                        filter_status = Some(val.parse::<Status>().unwrap_or_else(|e| {
                            eprintln!("Error: {e}");
                            process::exit(1);
                        }));
                    }
                    "--claimed-by" => {
                        filter_claimed_by = Some(reader.next_val("--claimed-by").to_string())
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

            let filter = FilterOptions {
                project: filter_project.as_deref(),
                tag: filter_tag.as_deref(),
                kind: filter_kind,
                item_type: filter_type,
                status: filter_status,
                claimed_by: filter_claimed_by.as_deref(),
                ready: false,
                query: Some(query),
                archive_filter,
            };

            let mut ideas = collect_ideas_with_filter(&vault_path, &filter).unwrap_or_default();
            sort_search_results(&mut ideas);
            if let Some(lim) = limit {
                ideas.truncate(lim);
            }

            let fmt = format.unwrap_or_else(|| default_format(OutputFormat::Table));
            emit_ideas(&ideas, fmt);
        }

        "context" => {
            let (
                mut filter_project,
                mut filter_kind,
                mut filter_type,
                mut filter_status,
                mut limit,
            ) = (None, None, None, None, None);
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
                    "--type" => {
                        let val = reader.next_val("--type");
                        filter_type = Some(val.parse::<WorkType>().unwrap_or_else(|e| {
                            eprintln!("Error: {e}");
                            process::exit(1);
                        }));
                    }
                    "--status" => {
                        let val = reader.next_val("--status");
                        filter_status = Some(val.parse::<Status>().unwrap_or_else(|e| {
                            eprintln!("Error: {e}");
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
            let filter = FilterOptions {
                project: Some(&project),
                kind: filter_kind,
                item_type: filter_type,
                status: filter_status,
                archive_filter,
                ..Default::default()
            };

            let mut ideas = collect_ideas_with_filter(&vault_path, &filter).unwrap_or_default();
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

        "next" => {
            let mut filter_project = None;
            let mut limit = Some(1);
            let mut format = None;

            while let Some(arg) = reader.peek() {
                match arg {
                    "--project" => filter_project = Some(reader.next_val("--project").to_string()),
                    "--limit" => {
                        let val = reader.next_val("--limit");
                        limit = Some(val.parse::<usize>().unwrap_or_else(|_| {
                            eprintln!("Error: --limit requires a positive integer");
                            process::exit(1);
                        }));
                    }
                    "--format" => format = Some(reader.parse_format(false)),
                    _ => {
                        eprintln!("Error: Unknown flag '{arg}'");
                        process::exit(1);
                    }
                }
                reader.idx += 1;
            }

            let proj = resolve_project(filter_project.as_deref());
            let all_items = vault::collect_ideas(&vault_path).unwrap_or_default();
            let ready = workflow::next_ready_items(&all_items, Some(&proj), limit.unwrap_or(1));

            let fmt = format.unwrap_or_else(|| default_format(OutputFormat::Plain));
            emit_ideas(&ready, fmt);
        }

        "transition" => {
            let selector = reader.parse_selector();
            let mut to_status = None;
            let mut actor = None;
            let mut note = None;
            let mut expect_revision = None;
            let mut format = None;

            while let Some(arg) = reader.peek() {
                match arg {
                    "--to" => {
                        let val = reader.next_val("--to");
                        match val.parse::<Status>() {
                            Ok(st) => to_status = Some(st),
                            Err(e) => {
                                eprintln!("Error: {e}");
                                process::exit(1);
                            }
                        }
                    }
                    "--actor" => actor = Some(reader.next_val("--actor")),
                    "--note" => note = Some(reader.next_val("--note")),
                    "--expect-revision" => {
                        let val = reader.next_val("--expect-revision");
                        expect_revision = Some(val.parse::<u64>().unwrap_or_else(|_| {
                            eprintln!("Error: --expect-revision requires an integer");
                            process::exit(1);
                        }));
                    }
                    "--format" => format = Some(reader.parse_format(false)),
                    _ => {
                        eprintln!("Error: Unknown flag '{arg}'");
                        process::exit(1);
                    }
                }
                reader.idx += 1;
            }

            let target_status = match to_status {
                Some(st) => st,
                None => {
                    eprintln!("Error: --to <status> is required for 'transition'");
                    process::exit(1);
                }
            };

            let filename = resolve_selector(&vault_path, selector).unwrap_or_else(|e| {
                eprintln!("Error: {e}");
                process::exit(1);
            });

            let resolved_actor = resolve_actor(actor);
            match workflow::transition_item(
                &vault_path,
                &filename,
                target_status,
                Some(&resolved_actor),
                note,
                expect_revision,
            ) {
                Ok(meta) => {
                    let fmt = format.unwrap_or_else(|| default_format(OutputFormat::Plain));
                    emit_single_idea(&meta, fmt);
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    process::exit(1);
                }
            }
        }

        "claim" => {
            let selector = reader.parse_selector();
            let mut actor = None;
            let mut lease = 3600;
            let mut expect_revision = None;
            let mut format = None;

            while let Some(arg) = reader.peek() {
                match arg {
                    "--actor" => actor = Some(reader.next_val("--actor")),
                    "--lease" => {
                        let val = reader.next_val("--lease");
                        lease = val.parse::<u64>().unwrap_or_else(|_| {
                            eprintln!("Error: --lease requires a positive integer seconds");
                            process::exit(1);
                        });
                    }
                    "--expect-revision" => {
                        let val = reader.next_val("--expect-revision");
                        expect_revision = Some(val.parse::<u64>().unwrap_or_else(|_| {
                            eprintln!("Error: --expect-revision requires an integer");
                            process::exit(1);
                        }));
                    }
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

            let resolved_actor = resolve_actor(actor);
            match workflow::claim_item(
                &vault_path,
                &filename,
                &resolved_actor,
                lease,
                expect_revision,
            ) {
                Ok(meta) => {
                    let fmt = format.unwrap_or_else(|| default_format(OutputFormat::Plain));
                    emit_single_idea(&meta, fmt);
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    process::exit(1);
                }
            }
        }

        "release" => {
            let selector = reader.parse_selector();
            let mut actor = None;
            let mut force = false;
            let mut expect_revision = None;
            let mut format = None;

            while let Some(arg) = reader.peek() {
                match arg {
                    "--actor" => actor = Some(reader.next_val("--actor")),
                    "--force" => force = true,
                    "--expect-revision" => {
                        let val = reader.next_val("--expect-revision");
                        expect_revision = Some(val.parse::<u64>().unwrap_or_else(|_| {
                            eprintln!("Error: --expect-revision requires an integer");
                            process::exit(1);
                        }));
                    }
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

            let resolved_actor = actor
                .map(|a| a.to_string())
                .unwrap_or_else(|| resolve_actor(None));
            match workflow::release_item(
                &vault_path,
                &filename,
                Some(&resolved_actor),
                force,
                expect_revision,
            ) {
                Ok(meta) => {
                    let fmt = format.unwrap_or_else(|| default_format(OutputFormat::Plain));
                    emit_single_idea(&meta, fmt);
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    process::exit(1);
                }
            }
        }

        "handoff" => {
            let selector = reader.parse_selector();
            let mut actor = None;
            let mut progress = None;
            let mut next = None;
            let mut blocker = None;
            let mut verification = None;
            let mut expect_revision = None;
            let mut format = None;

            while let Some(arg) = reader.peek() {
                match arg {
                    "--actor" => actor = Some(reader.next_val("--actor")),
                    "--progress" => progress = Some(reader.next_val("--progress")),
                    "--next" => next = Some(reader.next_val("--next")),
                    "--blocker" => blocker = Some(reader.next_val("--blocker")),
                    "--verification" => verification = Some(reader.next_val("--verification")),
                    "--expect-revision" => {
                        let val = reader.next_val("--expect-revision");
                        expect_revision = Some(val.parse::<u64>().unwrap_or_else(|_| {
                            eprintln!("Error: --expect-revision requires an integer");
                            process::exit(1);
                        }));
                    }
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

            let resolved_actor = resolve_actor(actor);
            match workflow::handoff_item(
                &vault_path,
                &filename,
                Some(&resolved_actor),
                progress,
                next,
                blocker,
                verification,
                expect_revision,
            ) {
                Ok(meta) => {
                    let fmt = format.unwrap_or_else(|| default_format(OutputFormat::Plain));
                    if fmt == OutputFormat::Json {
                        println!(
                            "{{\"id\":\"{}\",\"action\":\"handoff_updated\",\"status\":\"{}\"}}",
                            meta.id,
                            meta.current_status().as_str()
                        );
                    } else {
                        emit_single_idea(&meta, fmt);
                    }
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    process::exit(1);
                }
            }
        }

        "complete" => {
            let selector = reader.parse_selector();
            let mut actor = None;
            let mut evidence = None;
            let mut expect_revision = None;
            let mut format = None;

            while let Some(arg) = reader.peek() {
                match arg {
                    "--actor" => actor = Some(reader.next_val("--actor")),
                    "--evidence" => evidence = Some(reader.next_val("--evidence")),
                    "--expect-revision" => {
                        let val = reader.next_val("--expect-revision");
                        expect_revision = Some(val.parse::<u64>().unwrap_or_else(|_| {
                            eprintln!("Error: --expect-revision requires an integer");
                            process::exit(1);
                        }));
                    }
                    "--format" => format = Some(reader.parse_format(false)),
                    _ => {
                        eprintln!("Error: Unknown flag '{arg}'");
                        process::exit(1);
                    }
                }
                reader.idx += 1;
            }

            let evidence_str = match evidence {
                Some(ev) if !ev.trim().is_empty() => ev,
                _ => {
                    eprintln!("Error: --evidence is required for 'complete'");
                    process::exit(1);
                }
            };

            let filename = resolve_selector(&vault_path, selector).unwrap_or_else(|e| {
                eprintln!("Error: {e}");
                process::exit(1);
            });

            let resolved_actor = resolve_actor(actor);
            match workflow::complete_item(
                &vault_path,
                &filename,
                Some(&resolved_actor),
                evidence_str,
                expect_revision,
            ) {
                Ok(meta) => {
                    let fmt = format.unwrap_or_else(|| default_format(OutputFormat::Plain));
                    emit_single_idea(&meta, fmt);
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    process::exit(1);
                }
            }
        }

        "close" => {
            let selector = reader.parse_selector();
            let mut actor = None;
            let mut note = None;
            let mut expect_revision = None;
            let mut format = None;

            while let Some(arg) = reader.peek() {
                match arg {
                    "--actor" => actor = Some(reader.next_val("--actor")),
                    "--note" => note = Some(reader.next_val("--note")),
                    "--expect-revision" => {
                        let val = reader.next_val("--expect-revision");
                        expect_revision = Some(val.parse::<u64>().unwrap_or_else(|_| {
                            eprintln!("Error: --expect-revision requires an integer");
                            process::exit(1);
                        }));
                    }
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

            let resolved_actor = resolve_actor(actor);
            match workflow::close_item(
                &vault_path,
                &filename,
                Some(&resolved_actor),
                note,
                expect_revision,
            ) {
                Ok(meta) => {
                    let fmt = format.unwrap_or_else(|| default_format(OutputFormat::Plain));
                    emit_single_idea(&meta, fmt);
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    process::exit(1);
                }
            }
        }

        "depend" => {
            if args.len() >= 3 && (args[2] == "--help" || args[2] == "-h") {
                print_usage();
                process::exit(0);
            }
            if args.len() < 4 {
                eprintln!("Error: 'depend' requires an item selector and a dependency selector");
                process::exit(1);
            }
            if args[3] == "--help" || args[3] == "-h" {
                print_usage();
                process::exit(0);
            }
            let selector = &args[2];
            let dep_selector = &args[3];
            reader.idx = 4;

            let mut actor = None;
            let mut expect_revision = None;
            let mut format = None;

            while let Some(arg) = reader.peek() {
                match arg {
                    "--actor" => actor = Some(reader.next_val("--actor")),
                    "--expect-revision" => {
                        let val = reader.next_val("--expect-revision");
                        expect_revision = Some(val.parse::<u64>().unwrap_or_else(|_| {
                            eprintln!("Error: --expect-revision requires an integer");
                            process::exit(1);
                        }));
                    }
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
            let dep_filename = resolve_selector(&vault_path, dep_selector).unwrap_or_else(|e| {
                eprintln!("Error: {e}");
                process::exit(1);
            });
            let dep_content =
                fs::read_to_string(vault_path.join(&dep_filename)).unwrap_or_else(|e| {
                    eprintln!("Error: Could not read dependency file: {e}");
                    process::exit(1);
                });
            let mut issues = Vec::new();
            let dep_meta = parse_front_matter_detailed(&dep_filename, &dep_content, &mut issues)
                .unwrap_or_else(|| {
                    eprintln!("Error: Invalid front matter in dependency file");
                    process::exit(1);
                });

            let resolved_actor = resolve_actor(actor);
            match workflow::depend_item(
                &vault_path,
                &filename,
                &dep_meta.id,
                Some(&resolved_actor),
                expect_revision,
            ) {
                Ok(meta) => {
                    let fmt = format.unwrap_or_else(|| default_format(OutputFormat::Plain));
                    if fmt == OutputFormat::Json {
                        println!(
                            "{{\"id\":\"{}\",\"action\":\"dependency_added\",\"depends_on\":{:?}}}",
                            meta.id, meta.depends_on
                        );
                    } else {
                        println!("Added dependency on {} to {}", dep_meta.id, meta.id);
                    }
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    process::exit(1);
                }
            }
        }

        "parent" => {
            if args.len() >= 3 && (args[2] == "--help" || args[2] == "-h") {
                print_usage();
                process::exit(0);
            }
            if args.len() < 4 {
                eprintln!("Error: 'parent' requires an item selector and a parent selector");
                process::exit(1);
            }
            if args[3] == "--help" || args[3] == "-h" {
                print_usage();
                process::exit(0);
            }
            let selector = &args[2];
            let parent_selector = &args[3];
            reader.idx = 4;

            let mut actor = None;
            let mut expect_revision = None;
            let mut format = None;

            while let Some(arg) = reader.peek() {
                match arg {
                    "--actor" => actor = Some(reader.next_val("--actor")),
                    "--expect-revision" => {
                        let val = reader.next_val("--expect-revision");
                        expect_revision = Some(val.parse::<u64>().unwrap_or_else(|_| {
                            eprintln!("Error: --expect-revision requires an integer");
                            process::exit(1);
                        }));
                    }
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
            let parent_filename =
                resolve_selector(&vault_path, parent_selector).unwrap_or_else(|e| {
                    eprintln!("Error: {e}");
                    process::exit(1);
                });
            let parent_content = fs::read_to_string(vault_path.join(&parent_filename))
                .unwrap_or_else(|e| {
                    eprintln!("Error: Could not read parent file: {e}");
                    process::exit(1);
                });
            let mut issues = Vec::new();
            let parent_meta =
                parse_front_matter_detailed(&parent_filename, &parent_content, &mut issues)
                    .unwrap_or_else(|| {
                        eprintln!("Error: Invalid front matter in parent file");
                        process::exit(1);
                    });

            let resolved_actor = resolve_actor(actor);
            match workflow::parent_item(
                &vault_path,
                &filename,
                &parent_meta.id,
                Some(&resolved_actor),
                expect_revision,
            ) {
                Ok(meta) => {
                    let fmt = format.unwrap_or_else(|| default_format(OutputFormat::Plain));
                    if fmt == OutputFormat::Json {
                        println!(
                            "{{\"id\":\"{}\",\"parent_id\":\"{}\"}}",
                            meta.id, parent_meta.id
                        );
                    } else {
                        println!("Set parent of {} to {}", meta.id, parent_meta.id);
                    }
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    process::exit(1);
                }
            }
        }

        "relate" => {
            if args.len() >= 3 && (args[2] == "--help" || args[2] == "-h") {
                print_usage();
                process::exit(0);
            }
            if args.len() < 4 {
                eprintln!("Error: 'relate' requires an item selector and a related selector");
                process::exit(1);
            }
            if args[3] == "--help" || args[3] == "-h" {
                print_usage();
                process::exit(0);
            }
            let selector = &args[2];
            let rel_selector = &args[3];
            reader.idx = 4;

            let mut actor = None;
            let mut expect_revision = None;
            let mut format = None;

            while let Some(arg) = reader.peek() {
                match arg {
                    "--actor" => actor = Some(reader.next_val("--actor")),
                    "--expect-revision" => {
                        let val = reader.next_val("--expect-revision");
                        expect_revision = Some(val.parse::<u64>().unwrap_or_else(|_| {
                            eprintln!("Error: --expect-revision requires an integer");
                            process::exit(1);
                        }));
                    }
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
            let rel_filename = resolve_selector(&vault_path, rel_selector).unwrap_or_else(|e| {
                eprintln!("Error: {e}");
                process::exit(1);
            });
            let rel_content =
                fs::read_to_string(vault_path.join(&rel_filename)).unwrap_or_else(|e| {
                    eprintln!("Error: Could not read related file: {e}");
                    process::exit(1);
                });
            let mut issues = Vec::new();
            let rel_meta = parse_front_matter_detailed(&rel_filename, &rel_content, &mut issues)
                .unwrap_or_else(|| {
                    eprintln!("Error: Invalid front matter in related file");
                    process::exit(1);
                });

            let resolved_actor = resolve_actor(actor);
            match workflow::relate_item(
                &vault_path,
                &filename,
                &rel_meta.id,
                Some(&resolved_actor),
                expect_revision,
            ) {
                Ok(meta) => {
                    let fmt = format.unwrap_or_else(|| default_format(OutputFormat::Plain));
                    if fmt == OutputFormat::Json {
                        println!("{{\"id\":\"{}\",\"related\":{:?}}}", meta.id, meta.related);
                    } else {
                        println!("Related {} to {}", meta.id, rel_meta.id);
                    }
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    process::exit(1);
                }
            }
        }

        "doctor" => {
            let (mut repair, mut upgrade, mut strict, mut format) = (false, false, false, None);
            while let Some(arg) = reader.peek() {
                match arg {
                    "--repair" => repair = true,
                    "--upgrade" => upgrade = true,
                    "--strict" => strict = true,
                    "--format" => format = Some(reader.parse_format(false)),
                    _ => {
                        eprintln!("Error: Unknown flag '{arg}'");
                        process::exit(1);
                    }
                }
                reader.idx += 1;
            }

            let repaired_count = if upgrade {
                upgrade_vault(&vault_path)
            } else if repair {
                repair_vault(&vault_path)
            } else {
                0
            };

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
            let mut actor = None;
            let mut expect_revision = None;

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
                    "--actor" => actor = Some(reader.next_val("--actor").to_string()),
                    "--expect-revision" => {
                        let val = reader.next_val("--expect-revision");
                        expect_revision = Some(val.parse::<u64>().unwrap_or_else(|_| {
                            eprintln!("Error: --expect-revision requires an integer");
                            process::exit(1);
                        }));
                    }
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

            let actor_name = actor.or_else(|| env::var("PIN_ACTOR").ok());
            let meta = workflow::archive_item(
                &vault_path,
                &filename,
                resolution,
                actor_name.as_deref(),
                note.as_deref(),
                expect_revision,
            )
            .unwrap_or_else(|e| {
                eprintln!("Error: {e}");
                process::exit(1);
            });

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
        }

        "unarchive" => {
            let selector = reader.parse_selector();
            let mut format = None;
            let mut actor = None;
            let mut expect_revision = None;

            while let Some(arg) = reader.peek() {
                match arg {
                    "--actor" => actor = Some(reader.next_val("--actor").to_string()),
                    "--expect-revision" => {
                        let val = reader.next_val("--expect-revision");
                        expect_revision = Some(val.parse::<u64>().unwrap_or_else(|_| {
                            eprintln!("Error: --expect-revision requires an integer");
                            process::exit(1);
                        }));
                    }
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

            let actor_name = actor.or_else(|| env::var("PIN_ACTOR").ok());
            let meta = workflow::unarchive_item(
                &vault_path,
                &filename,
                actor_name.as_deref(),
                expect_revision,
            )
            .unwrap_or_else(|e| {
                eprintln!("Error: {e}");
                process::exit(1);
            });

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
            let mut editor_parts = split_shell_words(&editor);
            if editor_parts.is_empty() {
                editor_parts.push("nano".to_string());
            }
            let editor_cmd = editor_parts.remove(0);
            editor_parts.push(edit_path_str);

            let status = Command::new(&editor_cmd).args(&editor_parts).status();
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

            // Check if file on disk was modified concurrently
            let current_on_disk = fs::read_to_string(&path).unwrap_or_default();
            if current_on_disk != original_content {
                let recovery_filename = format!(".{edited_id}.edit-recovery.tmp");
                let recovery_path = vault_path.join(&recovery_filename);
                let _ = fs::write(&recovery_path, &edited_content);
                eprintln!(
                    "Error: File changed while editing. Saved recovery to '{recovery_filename}'"
                );
                process::exit(1);
            }

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

            let mut parsed_meta = parsed.unwrap();
            // Check if status transitioned to done without evidence
            if parsed_meta.current_status() == Status::Done {
                let has_evidence = parsed_meta
                    .handoff
                    .as_ref()
                    .and_then(|h| h.verification.as_deref())
                    .is_some_and(|v| !v.trim().is_empty());
                if !has_evidence {
                    let recovery_filename = format!(".{edited_id}.edit-recovery.tmp");
                    let recovery_path = vault_path.join(&recovery_filename);
                    let _ = fs::write(&recovery_path, &edited_content);
                    let _ = atomic_write(&path, &original_content);
                    eprintln!("Error: Completion requires non-empty evidence. Saved recovery to '{recovery_filename}'");
                    process::exit(1);
                }
            }

            parsed_meta.updated_at = Some(chrono::Utc::now().timestamp());
            parsed_meta.revision = Some(parsed_meta.current_revision() + 1);

            let new_doc = render_full_document(&parsed_meta);
            if let Err(e) = atomic_write(&path, &new_doc) {
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
            if args[2] == "--help" || args[2] == "-h" {
                print_usage();
                process::exit(0);
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
            let mut filter_project = None;
            while let Some(arg) = reader.peek() {
                match arg {
                    "--project" => filter_project = Some(reader.next_val("--project").to_string()),
                    "--format" => format = Some(reader.parse_format(false)),
                    _ => {
                        eprintln!("Error: Unexpected argument '{arg}'");
                        process::exit(1);
                    }
                }
                reader.idx += 1;
            }

            let stats = calculate_stats(&vault_path, filter_project.as_deref());
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
            let (mut filter_tag, mut filter_kind, mut filter_type, mut filter_status) =
                (None, None, None, None);
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
                    "--type" => {
                        let val = reader.next_val("--type");
                        filter_type = Some(val.parse::<WorkType>().unwrap_or_else(|e| {
                            eprintln!("Error: {e}");
                            process::exit(1);
                        }));
                    }
                    "--status" => {
                        let val = reader.next_val("--status");
                        filter_status = Some(val.parse::<Status>().unwrap_or_else(|e| {
                            eprintln!("Error: {e}");
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
            let filter = FilterOptions {
                project: filter_project.as_deref(),
                tag: filter_tag.as_deref(),
                kind: filter_kind,
                item_type: filter_type,
                status: filter_status,
                archive_filter,
                ..Default::default()
            };

            let ideas = collect_ideas_with_filter(&vault_path, &filter).unwrap_or_default();
            let snapshot = create_snapshot(&ideas, vault_path, scope_label, archive_filter);
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
