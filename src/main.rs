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

use doctor::{emit_doctor_report, repair_vault, scan_vault};
use frontmatter::{
    get_default_title, parse_front_matter_detailed, render_full_document, truncate_title, Severity,
};
use model::{
    ArchiveFilter, Handoff, IdeaMeta, Kind, OutputFormat, Priority, Resolution, Status, WorkType,
};
use output::{default_format, emit_context, emit_ideas, emit_mutation, emit_single_idea};
use search::sort_search_results;
use stats::{calculate_stats, emit_stats};
use vault::{
    atomic_write, collect_ideas_filtered, generate_id, lock_item, lock_vault, resolve_project,
    resolve_selector, resolve_vault_path, WorkItemFilter,
};
use viewer::{create_snapshot, serve_view, ViewConfig};
use workflow::{actor_from_env, append_activity};

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
         add <markdown> --kind technical|product|business|project\n                 \
         [--stdin] [--project <name>] [--title <title>]\n                 \
         [--tags <csv>] [--priority low|medium|high] [--type idea|task|bug|decision]\n                 \
         [--allow-duplicate] [--format json|plain]\n  \
         list [--project <name>] [--tag <name>] [--kind <kind>] [--type <type>]\n       \
          [--status <status>] [--claimed-by <actor>] [--ready] [--archived|--all]\n       \
         [--format json|table|plain]\n  \
         list-project [--tag <name>] [--kind <kind>] [--type <type>]\n               \
          [--status <status>] [--claimed-by <actor>] [--ready] [--archived|--all]\n               \
         [--format json|table|plain]\n  \
         search <query> [--project <name>] [--tag <name>] [--kind <kind>]\n                 \
         [--type <type>] [--status <status>] [--claimed-by <actor>]\n                 \
         [--limit <n>] [--archived|--all]\n                 \
         [--format json|table|plain]\n  \
         context [--project <name>] [--kind <kind>] [--type <type>]\n          \
         [--status <status>] [--limit <n>] [--group kind]\n          \
         [--archived|--all] [--format json|plain]\n  \
         next [--project <name>] [--limit <n>] [--format json|plain]\n  \
         transition <id|id-prefix|filename> --to <status> [--actor <name>]\n          \
         [--note <text>] [--expect-revision <n>] [--format json|plain]\n  \
         claim <id|id-prefix|filename> [--actor <name>] [--lease <seconds>]\n          \
         [--expect-revision <n>] [--format json|plain]\n  \
         release <id|id-prefix|filename> [--actor <name>] [--force]\n          \
         [--expect-revision <n>] [--format json|plain]\n  \
         handoff <id|id-prefix|filename> [--actor <name>] [--progress <text>]\n          \
         [--next <text>] [--blocker <text>] [--verification <text>]\n          \
         [--expect-revision <n>] [--format json|plain]\n  \
         complete <id|id-prefix|filename> --evidence <text> [--actor <name>]\n          \
         [--expect-revision <n>] [--format json|plain]\n  \
         close <id|id-prefix|filename> [--actor <name>] [--note <text>]\n          \
         [--expect-revision <n>] [--format json|plain]\n  \
         depend <id|id-prefix|filename> <dependency-id> [--actor <name>]\n          \
         [--expect-revision <n>] [--format json|plain]\n  \
         parent <id|id-prefix|filename> <parent-id> [--actor <name>]\n          \
         [--expect-revision <n>] [--format json|plain]\n  \
         relate <id|id-prefix|filename> <related-id> [--actor <name>]\n          \
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
         stats [--format json|plain]\n  \
          view [--project <name>] [--tag <name>] [--kind <kind>] [--type <type>] [--status <status>]\n       \
         [--archived|--all] [--port <n>] [--no-open] [--format json|plain]\n  \
          view-project [--tag <name>] [--kind <kind>] [--type <type>] [--status <status>] [--archived|--all]\n               \
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

fn parse_status_value(value: &str) -> Status {
    value.parse::<Status>().unwrap_or_else(|_| {
        eprintln!("Error: Unknown status '{value}'");
        process::exit(1);
    })
}

fn parse_work_type_value(value: &str) -> WorkType {
    value.parse::<WorkType>().unwrap_or_else(|_| {
        eprintln!("Error: Unknown work type '{value}'");
        process::exit(1);
    })
}

fn parse_revision_value(value: &str) -> u64 {
    value.parse::<u64>().unwrap_or_else(|_| {
        eprintln!("Error: --expect-revision requires a non-negative integer");
        process::exit(1);
    })
}

fn emit_workflow_error(error: workflow::WorkflowError) -> ! {
    eprintln!("Error: {error}");
    process::exit(1);
}

#[derive(Default)]
struct FilterFlags {
    project: Option<String>,
    tag: Option<String>,
    kind: Option<Kind>,
    item_type: Option<WorkType>,
    status: Option<Status>,
    claimed_by: Option<String>,
    archive_filter: ArchiveFilter,
    format: Option<OutputFormat>,
}

impl FilterFlags {
    fn parse_flag(&mut self, arg: &str, reader: &mut ArgReader, allow_table: bool) -> bool {
        match arg {
            "--archived" => {
                self.archive_filter = ArchiveFilter::Archived;
                true
            }
            "--all" => {
                self.archive_filter = ArchiveFilter::All;
                true
            }
            "--project" => {
                self.project = Some(reader.next_val("--project").to_string());
                true
            }
            "--tag" => {
                self.tag = Some(reader.next_val("--tag").to_string());
                true
            }
            "--kind" => {
                let val = reader.next_val("--kind");
                self.kind = Some(val.parse::<Kind>().unwrap_or_else(|_| {
                    eprintln!("Error: Unknown kind '{val}'");
                    process::exit(1);
                }));
                true
            }
            "--type" => {
                let val = reader.next_val("--type");
                self.item_type = Some(parse_work_type_value(val));
                true
            }
            "--status" => {
                let val = reader.next_val("--status");
                self.status = Some(parse_status_value(val));
                true
            }
            "--claimed-by" => {
                self.claimed_by = Some(reader.next_val("--claimed-by").to_string());
                true
            }
            "--format" => {
                self.format = Some(reader.parse_format(allow_table));
                true
            }
            _ => false,
        }
    }
}

#[derive(Default)]
struct MutationFlags {
    actor: Option<String>,
    expect_revision: Option<u64>,
    format: Option<OutputFormat>,
}

impl MutationFlags {
    fn parse_flag(&mut self, arg: &str, reader: &mut ArgReader) -> bool {
        match arg {
            "--actor" => {
                self.actor = Some(reader.next_val("--actor").to_string());
                true
            }
            "--expect-revision" => {
                self.expect_revision =
                    Some(parse_revision_value(reader.next_val("--expect-revision")));
                true
            }
            "--format" => {
                self.format = Some(reader.parse_format(false));
                true
            }
            _ => false,
        }
    }
}

fn run_mutation(
    vault_path: &Path,
    selector: &str,
    expected_revision: Option<u64>,
    action_label: &str,
    format: Option<OutputFormat>,
    action: workflow::MutationAction,
) {
    let meta = workflow::execute_mutation(vault_path, selector, expected_revision, action)
        .unwrap_or_else(|error| emit_workflow_error(error));
    let fmt = format.unwrap_or_else(|| default_format(OutputFormat::Plain));
    emit_mutation(&meta, action_label, fmt);
}

fn is_ready(item: &IdeaMeta, all_items: &[IdeaMeta], now: i64) -> bool {
    if item.current_status() != Status::Planned || item.has_active_claim(now) {
        return false;
    }

    item.depends_on.iter().all(|dependency| {
        all_items.iter().any(|candidate| {
            candidate.id == *dependency
                && matches!(candidate.current_status(), Status::Done | Status::Closed)
        })
    })
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
            let _ = fs::write(local_vault.join(".gitignore"), "*.lock\n.*.tmp\n");

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
            let (mut kind, mut priority, mut item_type, mut format) = (None, None, None, None);
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
                    "--type" => {
                        let val = reader.next_val("--type");
                        item_type = Some(parse_work_type_value(val));
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

            let _vault_lock = lock_vault(&vault_path).unwrap_or_else(|e| {
                eprintln!("Error: Could not lock vault: {e}");
                process::exit(1);
            });

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

            let actor = actor_from_env();
            let mut idea = IdeaMeta {
                schema: Some(2),
                id: id.clone(),
                project: proj_name,
                kind: final_kind,
                item_type: Some(item_type.unwrap_or(WorkType::Idea)),
                status: Some(Status::Captured),
                timestamp: now.timestamp(),
                created_at_ns: now.timestamp_nanos_opt(),
                title: title_val,
                tags: tags.map(|s| s.to_string()),
                priority,
                updated_at: Some(now.timestamp()),
                revision: Some(0),
                created_by: Some(actor.clone()),
                claimed_by: None,
                claim_expires_at: None,
                parent_id: None,
                depends_on: Vec::new(),
                related: Vec::new(),
                handoff: None,
                activity: Vec::new(),
                archived_at: None,
                resolution: None,
                resolution_note: None,
                filename,
                body: final_content,
                score: None,
                raw_frontmatter_map: serde_yaml::Mapping::new(),
            };
            append_activity(
                &mut idea,
                &actor,
                "created",
                None,
                Some(Status::Captured),
                None,
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
            let mut flags = FilterFlags::default();
            if project_scoped {
                flags.project = Some(resolve_project(None));
            }
            let mut ready_only = false;

            while let Some(arg) = reader.peek() {
                if arg == "--project" && project_scoped {
                    eprintln!("Error: 'list-project' does not accept --project");
                    process::exit(1);
                }
                if flags.parse_flag(arg, &mut reader, true) {
                    // handled
                } else if arg == "--ready" {
                    ready_only = true;
                } else {
                    eprintln!("Error: Unknown flag '{arg}'");
                    process::exit(1);
                }
                reader.idx += 1;
            }

            let mut ideas = vault::collect_work_items_filtered(
                &vault_path,
                &WorkItemFilter {
                    project: flags.project.as_deref(),
                    tag: flags.tag.as_deref(),
                    kind: flags.kind,
                    item_type: flags.item_type,
                    status: flags.status,
                    claimed_by: flags.claimed_by.as_deref(),
                    archive_filter: flags.archive_filter,
                    ..WorkItemFilter::default()
                },
            )
            .unwrap_or_default();

            if ready_only {
                let all_items = vault::collect_work_items_filtered(
                    &vault_path,
                    &WorkItemFilter {
                        archive_filter: ArchiveFilter::All,
                        ..WorkItemFilter::default()
                    },
                )
                .unwrap_or_default();
                let now = chrono::Utc::now().timestamp();
                ideas.retain(|item| is_ready(item, &all_items, now));
            }

            let fmt = flags
                .format
                .unwrap_or_else(|| default_format(OutputFormat::Table));
            emit_ideas(&ideas, fmt);
        }

        "search" => {
            if args.len() < 3 {
                eprintln!("Error: 'search' subcommand requires a query argument");
                process::exit(1);
            }
            let query = &args[2];
            reader.idx = 3;

            let mut flags = FilterFlags::default();
            let mut limit = None;

            while let Some(arg) = reader.peek() {
                if flags.parse_flag(arg, &mut reader, true) {
                    // handled
                } else if arg == "--limit" {
                    let val = reader.next_val("--limit");
                    limit = Some(val.parse::<usize>().unwrap_or_else(|_| {
                        eprintln!("Error: --limit requires a positive integer");
                        process::exit(1);
                    }));
                } else {
                    eprintln!("Error: Unknown flag '{arg}'");
                    process::exit(1);
                }
                reader.idx += 1;
            }

            let mut ideas = vault::collect_work_items_filtered(
                &vault_path,
                &WorkItemFilter {
                    project: flags.project.as_deref(),
                    tag: flags.tag.as_deref(),
                    kind: flags.kind,
                    item_type: flags.item_type,
                    status: flags.status,
                    claimed_by: flags.claimed_by.as_deref(),
                    query: Some(query),
                    archive_filter: flags.archive_filter,
                },
            )
            .unwrap_or_default();

            sort_search_results(&mut ideas);
            if let Some(lim) = limit {
                ideas.truncate(lim);
            }

            let fmt = flags
                .format
                .unwrap_or_else(|| default_format(OutputFormat::Table));
            emit_ideas(&ideas, fmt);
        }

        "context" => {
            let mut flags = FilterFlags::default();
            let mut limit = None;
            let mut group_kind = false;

            while let Some(arg) = reader.peek() {
                if flags.parse_flag(arg, &mut reader, false) {
                    // handled
                } else if arg == "--limit" {
                    let val = reader.next_val("--limit");
                    limit = Some(val.parse::<usize>().unwrap_or_else(|_| {
                        eprintln!("Error: --limit requires a positive integer");
                        process::exit(1);
                    }));
                } else if arg == "--group" {
                    let val = reader.next_val("--group");
                    if val == "kind" {
                        group_kind = true;
                    } else {
                        eprintln!("Error: --group only supports 'kind'");
                        process::exit(1);
                    }
                } else {
                    eprintln!("Error: Unknown flag '{arg}'");
                    process::exit(1);
                }
                reader.idx += 1;
            }

            let project = resolve_project(flags.project.as_deref());
            let mut ideas = vault::collect_work_items_filtered(
                &vault_path,
                &WorkItemFilter {
                    project: Some(&project),
                    kind: flags.kind,
                    item_type: flags.item_type,
                    status: flags.status,
                    archive_filter: flags.archive_filter,
                    ..WorkItemFilter::default()
                },
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

            let fmt = flags
                .format
                .unwrap_or_else(|| default_format(OutputFormat::Plain));
            emit_context(
                &ideas,
                &project,
                group_kind,
                flags.archive_filter,
                limit,
                fmt,
            );
        }

        "next" => {
            let mut filter_project = None;
            let mut limit = None;
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

            let project = resolve_project(filter_project.as_deref());
            let mut items = vault::collect_work_items_filtered(
                &vault_path,
                &WorkItemFilter {
                    project: Some(&project),
                    status: Some(Status::Planned),
                    ..WorkItemFilter::default()
                },
            )
            .unwrap_or_default();
            let all_items = vault::collect_work_items_filtered(
                &vault_path,
                &WorkItemFilter {
                    archive_filter: ArchiveFilter::All,
                    ..WorkItemFilter::default()
                },
            )
            .unwrap_or_default();
            let now = chrono::Utc::now().timestamp();
            items.retain(|item| is_ready(item, &all_items, now));
            items.sort_by(|a, b| {
                let priority = b.priority_rank().cmp(&a.priority_rank());
                if priority == std::cmp::Ordering::Equal {
                    a.timestamp.cmp(&b.timestamp)
                } else {
                    priority
                }
            });
            if let Some(limit) = limit {
                items.truncate(limit);
            }
            let fmt = format.unwrap_or_else(|| default_format(OutputFormat::Plain));
            emit_ideas(&items, fmt);
        }

        "transition" => {
            let selector = reader.parse_selector();
            let mut target = None;
            let mut note = None;
            let mut flags = MutationFlags::default();
            while let Some(arg) = reader.peek() {
                if flags.parse_flag(arg, &mut reader) {
                    // handled
                } else if arg == "--to" {
                    target = Some(parse_status_value(reader.next_val("--to")));
                } else if arg == "--note" {
                    note = Some(reader.next_val("--note").to_string());
                } else {
                    eprintln!("Error: Unknown flag '{arg}'");
                    process::exit(1);
                }
                reader.idx += 1;
            }
            let target = target.unwrap_or_else(|| {
                eprintln!("Error: '--to' is required for 'transition'");
                process::exit(1);
            });
            let actor = flags.actor.unwrap_or_else(actor_from_env);
            run_mutation(
                &vault_path,
                selector,
                flags.expect_revision,
                "transitioned",
                flags.format,
                workflow::MutationAction::Transition {
                    target,
                    actor,
                    note,
                },
            );
        }

        "claim" => {
            let selector = reader.parse_selector();
            let mut lease = workflow::DEFAULT_CLAIM_SECONDS;
            let mut flags = MutationFlags::default();
            while let Some(arg) = reader.peek() {
                if flags.parse_flag(arg, &mut reader) {
                    // handled
                } else if arg == "--lease" {
                    let value = reader.next_val("--lease");
                    lease = value.parse::<i64>().unwrap_or_else(|_| {
                        eprintln!("Error: --lease requires a positive integer");
                        process::exit(1);
                    });
                } else {
                    eprintln!("Error: Unknown flag '{arg}'");
                    process::exit(1);
                }
                reader.idx += 1;
            }
            let actor = flags.actor.unwrap_or_else(actor_from_env);
            run_mutation(
                &vault_path,
                selector,
                flags.expect_revision,
                "claimed",
                flags.format,
                workflow::MutationAction::Claim { actor, lease },
            );
        }

        "release" => {
            let selector = reader.parse_selector();
            let mut force = false;
            let mut flags = MutationFlags::default();
            while let Some(arg) = reader.peek() {
                if flags.parse_flag(arg, &mut reader) {
                    // handled
                } else if arg == "--force" {
                    force = true;
                } else {
                    eprintln!("Error: Unknown flag '{arg}'");
                    process::exit(1);
                }
                reader.idx += 1;
            }
            let actor = flags.actor.unwrap_or_else(actor_from_env);
            run_mutation(
                &vault_path,
                selector,
                flags.expect_revision,
                "released",
                flags.format,
                workflow::MutationAction::Release { actor, force },
            );
        }

        "handoff" => {
            let selector = reader.parse_selector();
            let (mut progress, mut next, mut blocker, mut verification) = (None, None, None, None);
            let mut flags = MutationFlags::default();
            while let Some(arg) = reader.peek() {
                if flags.parse_flag(arg, &mut reader) {
                    // handled
                } else {
                    match arg {
                        "--progress" => progress = Some(reader.next_val("--progress").to_string()),
                        "--next" => next = Some(reader.next_val("--next").to_string()),
                        "--blocker" => blocker = Some(reader.next_val("--blocker").to_string()),
                        "--verification" => {
                            verification = Some(reader.next_val("--verification").to_string())
                        }
                        _ => {
                            eprintln!("Error: Unknown flag '{arg}'");
                            process::exit(1);
                        }
                    }
                }
                reader.idx += 1;
            }
            let actor = flags.actor.unwrap_or_else(actor_from_env);
            let handoff = Handoff {
                progress,
                next,
                blocker,
                verification,
            };
            run_mutation(
                &vault_path,
                selector,
                flags.expect_revision,
                "handoff_updated",
                flags.format,
                workflow::MutationAction::Handoff { actor, handoff },
            );
        }

        "complete" => {
            let selector = reader.parse_selector();
            let mut evidence = None;
            let mut flags = MutationFlags::default();
            while let Some(arg) = reader.peek() {
                if flags.parse_flag(arg, &mut reader) {
                    // handled
                } else if arg == "--evidence" {
                    evidence = Some(reader.next_val("--evidence").to_string());
                } else {
                    eprintln!("Error: Unknown flag '{arg}'");
                    process::exit(1);
                }
                reader.idx += 1;
            }
            let evidence = evidence.unwrap_or_else(|| {
                eprintln!("Error: '--evidence' is required for 'complete'");
                process::exit(1);
            });
            let actor = flags.actor.unwrap_or_else(actor_from_env);
            run_mutation(
                &vault_path,
                selector,
                flags.expect_revision,
                "completed",
                flags.format,
                workflow::MutationAction::Complete { actor, evidence },
            );
        }

        "close" => {
            let selector = reader.parse_selector();
            let mut note = None;
            let mut flags = MutationFlags::default();
            while let Some(arg) = reader.peek() {
                if flags.parse_flag(arg, &mut reader) {
                    // handled
                } else if arg == "--note" {
                    note = Some(reader.next_val("--note").to_string());
                } else {
                    eprintln!("Error: Unknown flag '{arg}'");
                    process::exit(1);
                }
                reader.idx += 1;
            }
            let actor = flags.actor.unwrap_or_else(actor_from_env);
            run_mutation(
                &vault_path,
                selector,
                flags.expect_revision,
                "closed",
                flags.format,
                workflow::MutationAction::Close { actor, note },
            );
        }

        "depend" | "parent" | "relate" => {
            let selector = reader.parse_selector();
            if args.len() < 4 {
                eprintln!("Error: '{cmd}' requires a target item ID or selector");
                process::exit(1);
            }
            let target_selector = args[3].clone();
            reader.idx = 4;
            let mut flags = MutationFlags::default();
            while let Some(arg) = reader.peek() {
                if !flags.parse_flag(arg, &mut reader) {
                    eprintln!("Error: Unknown flag '{arg}'");
                    process::exit(1);
                }
                reader.idx += 1;
            }
            let actor = flags.actor.unwrap_or_else(actor_from_env);
            let (label, action) = match cmd.as_str() {
                "depend" => (
                    "dependency_added",
                    workflow::MutationAction::Depend {
                        actor,
                        dependency_selector: target_selector,
                    },
                ),
                "parent" => (
                    "parent_set",
                    workflow::MutationAction::Parent {
                        actor,
                        parent_selector: target_selector,
                    },
                ),
                _ => (
                    "related_item_added",
                    workflow::MutationAction::Relate {
                        actor,
                        related_selector: target_selector,
                    },
                ),
            };
            run_mutation(
                &vault_path,
                selector,
                flags.expect_revision,
                label,
                flags.format,
                action,
            );
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
            let _vault_lock = lock_vault(&vault_path).unwrap_or_else(|e| {
                eprintln!("Error: Could not lock vault: {e}");
                process::exit(1);
            });

            let repaired_count = if repair || upgrade {
                repair_vault(&vault_path, upgrade)
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

            let actor = actor_from_env();
            let meta = workflow::execute_mutation(
                &vault_path,
                selector,
                None,
                workflow::MutationAction::Archive {
                    actor,
                    resolution,
                    note,
                },
            )
            .unwrap_or_else(|error| emit_workflow_error(error));

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
                    filename: &meta.filename,
                    resolution: resolution.as_str(),
                };
                println!("{}", serde_json::to_string(&res).unwrap_or_default());
            } else {
                println!("Archived {}  {}", meta.id, meta.filename);
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

            let actor = actor_from_env();
            let meta = workflow::execute_mutation(
                &vault_path,
                selector,
                None,
                workflow::MutationAction::Unarchive { actor },
            )
            .unwrap_or_else(|error| emit_workflow_error(error));

            let fmt = format.unwrap_or_else(|| default_format(OutputFormat::Plain));
            if fmt == OutputFormat::Json {
                #[derive(serde::Serialize)]
                struct UnarchiveJson<'a> {
                    unarchived: &'a str,
                    filename: &'a str,
                }
                let res = UnarchiveJson {
                    unarchived: &meta.id,
                    filename: &meta.filename,
                };
                println!("{}", serde_json::to_string(&res).unwrap_or_default());
            } else {
                println!("Unarchived {}  {}", meta.id, meta.filename);
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
                .as_ref()
                .map(|m| m.id.clone())
                .unwrap_or_else(|| filename.clone());
            let original_revision = original_meta
                .as_ref()
                .map(IdeaMeta::current_revision)
                .unwrap_or(0);

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
            let default_editor = if cfg!(target_os = "windows") {
                "notepad.exe"
            } else {
                "nano"
            };
            let mut editor_parts = parse_command_line(&editor);
            let editor_cmd = if editor_parts.is_empty() {
                default_editor.to_string()
            } else {
                editor_parts.remove(0)
            };
            let mut editor_args = editor_parts;
            editor_args.push(edit_path_str);

            let status = Command::new(&editor_cmd).args(&editor_args).status();
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
                eprintln!("Error: Front matter is invalid after editing. Saved recovery to '{recovery_filename}'");
                process::exit(1);
            }

            let _lock = lock_item(&path).unwrap_or_else(|e| {
                eprintln!("Error: Could not lock file for editing: {e}");
                process::exit(1);
            });
            let current_content = fs::read_to_string(&path).unwrap_or_else(|e| {
                eprintln!("Error: Could not re-read file after editing: {e}");
                process::exit(1);
            });
            let mut current_issues = Vec::new();
            let current_meta =
                parse_front_matter_detailed(&filename, &current_content, &mut current_issues);
            let current_revision = current_meta
                .as_ref()
                .map(IdeaMeta::current_revision)
                .unwrap_or(0);
            if current_content != original_content
                || current_revision != original_revision
                || current_meta.is_none()
            {
                let recovery_filename = format!(".{edited_id}.edit-recovery.tmp");
                let recovery_path = vault_path.join(&recovery_filename);
                let _ = fs::write(&recovery_path, &edited_content);
                eprintln!(
                    "Error: File changed while editing. Saved recovery to '{recovery_filename}'"
                );
                process::exit(1);
            }

            let current_meta = current_meta.unwrap();
            let mut edited_meta = parsed.unwrap();
            if edited_meta.id != current_meta.id || edited_meta.project != current_meta.project {
                let recovery_filename = format!(".{edited_id}.edit-recovery.tmp");
                let recovery_path = vault_path.join(&recovery_filename);
                let _ = fs::write(&recovery_path, &edited_content);
                eprintln!(
                    "Error: Editing cannot change the item ID or project. Saved recovery to '{recovery_filename}'"
                );
                process::exit(1);
            }

            let previous_status = current_meta.current_status();
            let edited_status = edited_meta.status.unwrap_or(previous_status);
            if edited_status != previous_status
                && !workflow::can_transition(previous_status, edited_status)
            {
                let recovery_filename = format!(".{edited_id}.edit-recovery.tmp");
                let recovery_path = vault_path.join(&recovery_filename);
                let _ = fs::write(&recovery_path, &edited_content);
                eprintln!(
                    "Error: Invalid status transition from '{previous_status}' to '{edited_status}'. Saved recovery to '{recovery_filename}'"
                );
                process::exit(1);
            }

            edited_meta.status = Some(previous_status);
            edited_meta.item_type = edited_meta.item_type.or(current_meta.item_type);
            edited_meta.revision = Some(current_meta.current_revision());
            edited_meta.updated_at = current_meta.updated_at;
            edited_meta.created_by = current_meta.created_by.clone();
            edited_meta.claimed_by = current_meta.claimed_by.clone();
            edited_meta.claim_expires_at = current_meta.claim_expires_at;
            edited_meta.activity = current_meta.activity.clone();
            let actor = actor_from_env();
            if edited_status != previous_status {
                if let Err(error) = workflow::transition(
                    &mut edited_meta,
                    edited_status,
                    &actor,
                    Some("Status changed while editing".to_string()),
                ) {
                    let recovery_filename = format!(".{edited_id}.edit-recovery.tmp");
                    let recovery_path = vault_path.join(&recovery_filename);
                    let _ = fs::write(&recovery_path, &edited_content);
                    eprintln!("Error: {error}. Saved recovery to '{recovery_filename}'");
                    process::exit(1);
                }
            } else {
                workflow::append_activity(&mut edited_meta, &actor, "edited", None, None, None);
            }
            workflow::prepare_v2(&mut edited_meta);
            edited_meta.revision = Some(current_meta.current_revision().saturating_add(1));
            edited_meta.updated_at = Some(chrono::Utc::now().timestamp());

            if let Err(e) = atomic_write(&path, &render_full_document(&edited_meta)) {
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
            let _vault_lock = lock_vault(&vault_path).unwrap_or_else(|e| {
                eprintln!("Error: Could not lock vault: {e}");
                process::exit(1);
            });

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

            let lock_path = path.with_extension("md.lock");
            {
                let _item_lock = lock_item(&path).unwrap_or_else(|e| {
                    eprintln!("Error: Could not lock item for deletion: {e}");
                    process::exit(1);
                });

                if let Err(e) = fs::remove_file(&path) {
                    eprintln!("Error: Failed to delete idea: {e}");
                    process::exit(1);
                }
            }
            let _ = fs::remove_file(&lock_path);
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

                let _vault_lock = lock_vault(&vault_path).unwrap_or_else(|e| {
                    eprintln!("Error: Could not lock vault: {e}");
                    process::exit(1);
                });

                let (mut copied, mut skipped) = (0, 0);
                for (src_path, filename) in valid_entries {
                    let dest_path = vault_path.join(&filename);
                    if dest_path.exists() && !force {
                        skipped += 1;
                        continue;
                    }
                    let _item_lock = lock_item(&dest_path).ok();
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
                let _vault_lock = lock_vault(&vault_path).unwrap_or_else(|e| {
                    eprintln!("Error: Could not lock vault: {e}");
                    process::exit(1);
                });

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
            let mut flags = FilterFlags::default();
            if project_scoped {
                flags.project = Some(resolve_project(None));
            }
            let (mut port, mut no_open) = (0, false);

            while let Some(arg) = reader.peek() {
                if arg == "--project" && project_scoped {
                    eprintln!("Error: 'view-project' does not accept --project");
                    process::exit(1);
                }
                if flags.parse_flag(arg, &mut reader, false) {
                    // handled
                } else if arg == "--no-open" {
                    no_open = true;
                } else if arg == "--port" {
                    let val = reader.next_val("--port");
                    port = val.parse::<u16>().unwrap_or_else(|_| {
                        eprintln!("Error: --port requires an integer port (0-65535)");
                        process::exit(1);
                    });
                } else {
                    eprintln!("Error: Unknown flag '{arg}'");
                    process::exit(1);
                }
                reader.idx += 1;
            }

            if !io::stdout().is_terminal() && !no_open && flags.format.is_none() {
                eprintln!("Error: 'view' must be run in an interactive terminal, or with --no-open and an explicit format");
                process::exit(1);
            }

            let scope_label = flags.project.as_deref().unwrap_or("all");
            let snapshot = create_snapshot(ViewConfig {
                vault_path: vault_path.clone(),
                scope_label: scope_label.to_string(),
                project: flags.project,
                tag: flags.tag,
                kind: flags.kind,
                item_type: flags.item_type,
                status: flags.status,
                archive_filter: flags.archive_filter,
            });
            let fmt = flags.format.unwrap_or(OutputFormat::Plain);

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

pub fn parse_command_line(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut in_token = false;

    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if in_single_quote {
            if c == '\'' {
                in_single_quote = false;
            } else {
                current.push(c);
            }
            in_token = true;
        } else if in_double_quote {
            if c == '"' {
                in_double_quote = false;
            } else if c == '\\' {
                if i + 1 < chars.len() && (chars[i + 1] == '"' || chars[i + 1] == '\\') {
                    i += 1;
                    current.push(chars[i]);
                } else {
                    current.push('\\');
                }
            } else {
                current.push(c);
            }
            in_token = true;
        } else if c == '\'' {
            in_single_quote = true;
            in_token = true;
        } else if c == '"' {
            in_double_quote = true;
            in_token = true;
        } else if c == '\\' {
            if i + 1 < chars.len() {
                i += 1;
                current.push(chars[i]);
            } else {
                current.push('\\');
            }
            in_token = true;
        } else if c.is_whitespace() {
            if in_token {
                tokens.push(current);
                current = String::new();
                in_token = false;
            }
        } else {
            current.push(c);
            in_token = true;
        }
        i += 1;
    }

    if in_token {
        tokens.push(current);
    }

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_command_line_simple() {
        assert_eq!(parse_command_line("nano"), vec!["nano"]);
        assert_eq!(parse_command_line("code --wait"), vec!["code", "--wait"]);
    }

    #[test]
    fn test_parse_command_line_quoted_spaces() {
        assert_eq!(
            parse_command_line(r#""/path with spaces/editor" --wait"#),
            vec!["/path with spaces/editor", "--wait"]
        );
        assert_eq!(
            parse_command_line(r#"'/path with spaces/editor' --wait"#),
            vec!["/path with spaces/editor", "--wait"]
        );
    }

    #[test]
    fn test_parse_command_line_windows_paths() {
        assert_eq!(
            parse_command_line(r#""C:\Program Files\Editor\editor.exe" --wait"#),
            vec![r#"C:\Program Files\Editor\editor.exe"#, "--wait"]
        );
        assert_eq!(
            parse_command_line(r#""C:\\Program Files\\Editor\\editor.exe" --wait"#),
            vec![r#"C:\Program Files\Editor\editor.exe"#, "--wait"]
        );
        assert_eq!(
            parse_command_line(r#"'C:\Program Files\Editor\editor.exe' --wait"#),
            vec![r#"C:\Program Files\Editor\editor.exe"#, "--wait"]
        );
    }

    #[test]
    fn test_parse_command_line_escaped_spaces() {
        assert_eq!(
            parse_command_line(r#"path\ with\ spaces --wait"#),
            vec!["path with spaces", "--wait"]
        );
    }

    #[test]
    fn test_parse_command_line_empty_and_whitespace() {
        assert_eq!(parse_command_line(""), Vec::<String>::new());
        assert_eq!(parse_command_line("   "), Vec::<String>::new());
    }
}
