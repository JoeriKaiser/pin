use crate::cli::{
    read_stdin_content, resolve_actor, split_shell_words, ArgReader, CliError, CliResult,
};
use crate::frontmatter::{
    get_default_title, parse_front_matter_detailed, render_full_document, truncate_title, Severity,
};
use crate::model::{ArchiveFilter, IdeaMeta, Kind, OutputFormat, Priority, Status, WorkType};
use crate::output::{default_format, emit_ideas, emit_single_idea};
use crate::search::sort_search_results;
use crate::vault::{
    atomic_write, collect_ideas_with_filter, generate_id, resolve_project, resolve_selector,
    FilterOptions,
};
use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

pub fn add(args: &[String], command: &str, vault_path: &Path) -> CliResult<()> {
    let mut reader = ArgReader::new(args, command);
    let (mut content, mut project, mut title, mut tags) = (None, None, None, None);
    let (mut kind, mut item_type, mut status, mut actor, mut priority, mut format) =
        (None, None, None, None, None, None);
    let (mut use_stdin, mut allow_duplicate) = (false, false);

    while let Some(arg) = reader.peek()? {
        match arg {
            "--stdin" => use_stdin = true,
            "--allow-duplicate" => allow_duplicate = true,
            "--project" => project = Some(reader.next_val("--project")?),
            "--title" => title = Some(reader.next_val("--title")?),
            "--tags" => tags = Some(reader.next_val("--tags")?),
            "--kind" => {
                let val = reader.next_val("--kind")?;
                match val.parse::<Kind>() {
                    Ok(k) if k != Kind::Unspecified => kind = Some(k),
                    _ => {
                        return Err(CliError::usage(
                            "--kind must be technical, product, business, or project",
                        ))
                    }
                }
            }
            "--type" => {
                let val = reader.next_val("--type")?;
                match val.parse::<WorkType>() {
                    Ok(t) => item_type = Some(t),
                    Err(e) => return Err(CliError::usage(e.to_string())),
                }
            }
            "--status" => {
                let val = reader.next_val("--status")?;
                match val.parse::<Status>() {
                    Ok(s) => status = Some(s),
                    Err(e) => return Err(CliError::usage(e.to_string())),
                }
            }
            "--actor" => actor = Some(reader.next_val("--actor")?),
            "--priority" => {
                let val = reader.next_val("--priority")?;
                match val.parse::<Priority>() {
                    Ok(p) => priority = Some(p),
                    Err(_) => {
                        return Err(CliError::usage("--priority must be low, medium, or high"))
                    }
                }
            }
            "--format" => format = Some(reader.parse_format(false)?),
            _ if arg.starts_with("--") => {
                return Err(CliError::usage(format!("Unknown flag '{arg}'")))
            }
            _ => {
                if content.is_some() {
                    return Err(CliError::usage("Multiple content arguments provided"));
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
        _ => return Err(CliError::usage("Content argument is required for 'add'")),
    };

    let final_kind = match kind {
        Some(k) => k,
        None => {
            if let Some(t) = item_type {
                match t {
                    WorkType::Task | WorkType::Bug => Kind::Technical,
                    WorkType::Idea | WorkType::Decision => {
                        return Err(CliError::usage(
                            "--kind is required (technical, product, business, or project)",
                        ))
                    }
                }
            } else {
                return Err(CliError::usage(
                    "--kind is required (technical, product, business, or project)",
                ));
            }
        }
    };

    let proj_name = resolve_project(project);
    let title_val = match title {
        Some(t) => truncate_title(t),
        None => get_default_title(&final_content).unwrap_or_default(),
    };

    if title_val.trim().is_empty() {
        return Err(CliError::usage("Could not determine a non-empty title"));
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
                    return Err(CliError::usage(format!("An idea titled '{title_val}' already exists for project '{proj_name}' ({}). Use --allow-duplicate to add it anyway.",
                            existing.id)));
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

    atomic_write(&file_path, &render_full_document(&idea))
        .map_err(|e| CliError::usage(format!("Failed to save idea: {e}")))?;

    let fmt = format.unwrap_or_else(|| default_format(OutputFormat::Plain));
    emit_single_idea(&idea, fmt);
    Ok(())
}

pub fn list(args: &[String], command: &str, vault_path: &Path) -> CliResult<()> {
    let mut reader = ArgReader::new(args, command);
    let project_scoped = command == "list-project";
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
    let (mut filter_ready, mut archive_filter, mut format) = (false, ArchiveFilter::Active, None);

    while let Some(arg) = reader.peek()? {
        match arg {
            "--archived" => archive_filter = ArchiveFilter::Archived,
            "--all" => archive_filter = ArchiveFilter::All,
            "--ready" => filter_ready = true,
            "--project" => {
                if project_scoped {
                    return Err(CliError::usage("'list-project' does not accept --project"));
                }
                filter_project = Some(reader.next_val("--project")?.to_string());
            }
            "--tag" => filter_tag = Some(reader.next_val("--tag")?.to_string()),
            "--kind" => {
                let val = reader.next_val("--kind")?;
                filter_kind = Some(
                    val.parse::<Kind>()
                        .map_err(|_| CliError::usage(format!("Unknown kind '{val}'")))?,
                );
            }
            "--type" => {
                let val = reader.next_val("--type")?;
                filter_type = Some(
                    val.parse::<WorkType>()
                        .map_err(|e| CliError::usage(e.to_string()))?,
                );
            }
            "--status" => {
                let val = reader.next_val("--status")?;
                filter_status = Some(
                    val.parse::<Status>()
                        .map_err(|e| CliError::usage(e.to_string()))?,
                );
            }
            "--claimed-by" => {
                filter_claimed_by = Some(reader.next_val("--claimed-by")?.to_string())
            }
            "--format" => format = Some(reader.parse_format(true)?),
            _ => return Err(CliError::usage(format!("Unknown flag '{arg}'"))),
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
    Ok(())
}

pub fn search(args: &[String], command: &str, vault_path: &Path) -> CliResult<()> {
    let mut reader = ArgReader::new(args, command);
    if args.len() < 3 {
        return Err(CliError::usage(
            "'search' subcommand requires a query argument",
        ));
    }
    if args[2] == "--help" || args[2] == "-h" {
        return Err(CliError::Help);
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

    while let Some(arg) = reader.peek()? {
        match arg {
            "--archived" => archive_filter = ArchiveFilter::Archived,
            "--all" => archive_filter = ArchiveFilter::All,
            "--project" => filter_project = Some(reader.next_val("--project")?.to_string()),
            "--tag" => filter_tag = Some(reader.next_val("--tag")?.to_string()),
            "--kind" => {
                let val = reader.next_val("--kind")?;
                filter_kind = Some(
                    val.parse::<Kind>()
                        .map_err(|_| CliError::usage(format!("Unknown kind '{val}'")))?,
                );
            }
            "--type" => {
                let val = reader.next_val("--type")?;
                filter_type = Some(
                    val.parse::<WorkType>()
                        .map_err(|e| CliError::usage(e.to_string()))?,
                );
            }
            "--status" => {
                let val = reader.next_val("--status")?;
                filter_status = Some(
                    val.parse::<Status>()
                        .map_err(|e| CliError::usage(e.to_string()))?,
                );
            }
            "--claimed-by" => {
                filter_claimed_by = Some(reader.next_val("--claimed-by")?.to_string())
            }
            "--limit" => {
                let val = reader.next_val("--limit")?;
                limit = Some(
                    val.parse::<usize>()
                        .map_err(|_| CliError::usage("--limit requires a positive integer"))?,
                );
            }
            "--format" => format = Some(reader.parse_format(true)?),
            _ => return Err(CliError::usage(format!("Unknown flag '{arg}'"))),
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
    Ok(())
}

pub fn read(args: &[String], command: &str, vault_path: &Path) -> CliResult<()> {
    let mut reader = ArgReader::new(args, command);
    let selector = reader.parse_selector()?;
    let mut format = None;

    while let Some(arg) = reader.peek()? {
        match arg {
            "--format" => format = Some(reader.parse_format(false)?),
            _ => return Err(CliError::usage(format!("Unexpected argument '{arg}'"))),
        }
        reader.idx += 1;
    }

    let filename =
        resolve_selector(&vault_path, selector).map_err(|e| CliError::usage(e.to_string()))?;
    let path = vault_path.join(&filename);
    let content = fs::read_to_string(&path)
        .map_err(|e| CliError::usage(format!("Could not read file: {e}")))?;

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
    Ok(())
}

pub fn edit(args: &[String], command: &str, vault_path: &Path) -> CliResult<()> {
    let mut reader = ArgReader::new(args, command);
    let selector = reader.parse_selector()?;
    let mut format = None;

    while let Some(arg) = reader.peek()? {
        match arg {
            "--format" => format = Some(reader.parse_format(false)?),
            _ => return Err(CliError::usage(format!("Unexpected argument '{arg}'"))),
        }
        reader.idx += 1;
    }

    let filename =
        resolve_selector(&vault_path, selector).map_err(|e| CliError::usage(e.to_string()))?;
    let path = vault_path.join(&filename);
    let original_content = fs::read_to_string(&path)
        .map_err(|e| CliError::usage(format!("Could not read file: {e}")))?;

    let mut temp_issues = Vec::new();
    let original_meta = parse_front_matter_detailed(&filename, &original_content, &mut temp_issues);
    let edited_id = original_meta
        .map(|m| m.id)
        .unwrap_or_else(|| filename.clone());

    let temp_edit_file = tempfile::Builder::new()
        .prefix("pin-edit-")
        .suffix(".md")
        .tempfile()
        .map_err(|e| CliError::usage(format!("Could not create temp edit file: {e}")))?;

    fs::write(temp_edit_file.path(), &original_content)
        .map_err(|e| CliError::usage(format!("Could not prepare edit file: {e}")))?;

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
        _ => return Err(CliError::usage("Editor exited with non-zero status")),
    }

    let edited_content = fs::read_to_string(temp_edit_file.path())
        .map_err(|e| CliError::usage(format!("Could not read edited file: {e}")))?;

    // Check if file on disk was modified concurrently
    let current_on_disk = fs::read_to_string(&path).unwrap_or_default();
    if current_on_disk != original_content {
        let recovery_filename = format!(".{edited_id}.edit-recovery.tmp");
        let recovery_path = vault_path.join(&recovery_filename);
        let _ = fs::write(&recovery_path, &edited_content);
        return Err(CliError::usage(format!(
            "File changed while editing. Saved recovery to '{recovery_filename}'"
        )));
    }

    let mut issues = Vec::new();
    let parsed = parse_front_matter_detailed(&filename, &edited_content, &mut issues);
    let has_errors = issues.iter().any(|i| i.severity == Severity::Error);

    if parsed.is_none() || has_errors {
        let recovery_filename = format!(".{edited_id}.edit-recovery.tmp");
        let recovery_path = vault_path.join(&recovery_filename);
        let _ = fs::write(&recovery_path, &edited_content);
        let _ = atomic_write(&path, &original_content);
        return Err(CliError::usage(format!(
            "Front matter is invalid after editing. Saved recovery to '{recovery_filename}'"
        )));
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
            return Err(CliError::usage(format!(
                "Completion requires non-empty evidence. Saved recovery to '{recovery_filename}'"
            )));
        }
    }

    parsed_meta.updated_at = Some(chrono::Utc::now().timestamp());
    parsed_meta.revision = Some(parsed_meta.current_revision() + 1);

    let new_doc = render_full_document(&parsed_meta);
    atomic_write(&path, &new_doc)
        .map_err(|e| CliError::usage(format!("Failed to save edited proposal: {e}")))?;

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
    Ok(())
}

pub fn rm(args: &[String], command: &str, vault_path: &Path) -> CliResult<()> {
    let mut reader = ArgReader::new(args, command);
    let selector = reader.parse_selector()?;
    let mut format = None;

    while let Some(arg) = reader.peek()? {
        match arg {
            "--format" => format = Some(reader.parse_format(false)?),
            _ => return Err(CliError::usage(format!("Unexpected argument '{arg}'"))),
        }
        reader.idx += 1;
    }

    let filename =
        resolve_selector(&vault_path, selector).map_err(|e| CliError::usage(e.to_string()))?;
    let path = vault_path.join(&filename);
    let mut removed_id = filename.clone();
    if let Ok(c) = fs::read_to_string(&path) {
        let mut issues = Vec::new();
        if let Some(meta) = parse_front_matter_detailed(&filename, &c, &mut issues) {
            removed_id = meta.id;
        }
    }

    fs::remove_file(&path).map_err(|e| CliError::usage(format!("Failed to delete idea: {e}")))?;

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
    Ok(())
}
