use crate::cli::{resolve_actor, ArgReader, CliError, CliResult};
use crate::frontmatter::parse_front_matter_detailed;
use crate::model::{OutputFormat, Status};
use crate::output::{default_format, emit_single_idea};
use crate::vault::resolve_selector;
use crate::workflow;
use std::fs;
use std::path::Path;

pub fn transition(args: &[String], command: &str, vault_path: &Path) -> CliResult<()> {
    let mut reader = ArgReader::new(args, command);
    let selector = reader.parse_selector()?;
    let mut to_status = None;
    let mut actor = None;
    let mut note = None;
    let mut expect_revision = None;
    let mut format = None;

    while let Some(arg) = reader.peek()? {
        match arg {
            "--to" => {
                let val = reader.next_val("--to")?;
                match val.parse::<Status>() {
                    Ok(st) => to_status = Some(st),
                    Err(e) => return Err(CliError::usage(e.to_string())),
                }
            }
            "--actor" => actor = Some(reader.next_val("--actor")?),
            "--note" => note = Some(reader.next_val("--note")?),
            "--expect-revision" => {
                let val = reader.next_val("--expect-revision")?;
                expect_revision = Some(
                    val.parse::<u64>()
                        .map_err(|_| CliError::usage("--expect-revision requires an integer"))?,
                );
            }
            "--format" => format = Some(reader.parse_format(false)?),
            _ => return Err(CliError::usage(format!("Unknown flag '{arg}'"))),
        }
        reader.idx += 1;
    }

    let target_status = match to_status {
        Some(st) => st,
        None => {
            return Err(CliError::usage(
                "--to <status> is required for 'transition'",
            ))
        }
    };

    let filename =
        resolve_selector(&vault_path, selector).map_err(|e| CliError::usage(e.to_string()))?;

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
        Err(e) => return Err(CliError::usage(e.to_string())),
    }
    Ok(())
}

pub fn claim(args: &[String], command: &str, vault_path: &Path) -> CliResult<()> {
    let mut reader = ArgReader::new(args, command);
    let selector = reader.parse_selector()?;
    let mut actor = None;
    let mut lease = 3600;
    let mut expect_revision = None;
    let mut format = None;

    while let Some(arg) = reader.peek()? {
        match arg {
            "--actor" => actor = Some(reader.next_val("--actor")?),
            "--lease" => {
                let val = reader.next_val("--lease")?;
                lease = val
                    .parse::<u64>()
                    .map_err(|_| CliError::usage("--lease requires a positive integer seconds"))?;
            }
            "--expect-revision" => {
                let val = reader.next_val("--expect-revision")?;
                expect_revision = Some(
                    val.parse::<u64>()
                        .map_err(|_| CliError::usage("--expect-revision requires an integer"))?,
                );
            }
            "--format" => format = Some(reader.parse_format(false)?),
            _ => return Err(CliError::usage(format!("Unknown flag '{arg}'"))),
        }
        reader.idx += 1;
    }

    let filename =
        resolve_selector(&vault_path, selector).map_err(|e| CliError::usage(e.to_string()))?;

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
        Err(e) => return Err(CliError::usage(e.to_string())),
    }
    Ok(())
}

pub fn release(args: &[String], command: &str, vault_path: &Path) -> CliResult<()> {
    let mut reader = ArgReader::new(args, command);
    let selector = reader.parse_selector()?;
    let mut actor = None;
    let mut force = false;
    let mut expect_revision = None;
    let mut format = None;

    while let Some(arg) = reader.peek()? {
        match arg {
            "--actor" => actor = Some(reader.next_val("--actor")?),
            "--force" => force = true,
            "--expect-revision" => {
                let val = reader.next_val("--expect-revision")?;
                expect_revision = Some(
                    val.parse::<u64>()
                        .map_err(|_| CliError::usage("--expect-revision requires an integer"))?,
                );
            }
            "--format" => format = Some(reader.parse_format(false)?),
            _ => return Err(CliError::usage(format!("Unknown flag '{arg}'"))),
        }
        reader.idx += 1;
    }

    let filename =
        resolve_selector(&vault_path, selector).map_err(|e| CliError::usage(e.to_string()))?;

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
        Err(e) => return Err(CliError::usage(e.to_string())),
    }
    Ok(())
}

pub fn handoff(args: &[String], command: &str, vault_path: &Path) -> CliResult<()> {
    let mut reader = ArgReader::new(args, command);
    let selector = reader.parse_selector()?;
    let mut actor = None;
    let mut progress = None;
    let mut next = None;
    let mut blocker = None;
    let mut verification = None;
    let mut expect_revision = None;
    let mut format = None;

    while let Some(arg) = reader.peek()? {
        match arg {
            "--actor" => actor = Some(reader.next_val("--actor")?),
            "--progress" => progress = Some(reader.next_val("--progress")?),
            "--next" => next = Some(reader.next_val("--next")?),
            "--blocker" => blocker = Some(reader.next_val("--blocker")?),
            "--verification" => verification = Some(reader.next_val("--verification")?),
            "--expect-revision" => {
                let val = reader.next_val("--expect-revision")?;
                expect_revision = Some(
                    val.parse::<u64>()
                        .map_err(|_| CliError::usage("--expect-revision requires an integer"))?,
                );
            }
            "--format" => format = Some(reader.parse_format(false)?),
            _ => return Err(CliError::usage(format!("Unknown flag '{arg}'"))),
        }
        reader.idx += 1;
    }

    let filename =
        resolve_selector(&vault_path, selector).map_err(|e| CliError::usage(e.to_string()))?;

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
        Err(e) => return Err(CliError::usage(e.to_string())),
    }
    Ok(())
}

pub fn complete(args: &[String], command: &str, vault_path: &Path) -> CliResult<()> {
    let mut reader = ArgReader::new(args, command);
    let selector = reader.parse_selector()?;
    let mut actor = None;
    let mut evidence = None;
    let mut expect_revision = None;
    let mut format = None;

    while let Some(arg) = reader.peek()? {
        match arg {
            "--actor" => actor = Some(reader.next_val("--actor")?),
            "--evidence" => evidence = Some(reader.next_val("--evidence")?),
            "--expect-revision" => {
                let val = reader.next_val("--expect-revision")?;
                expect_revision = Some(
                    val.parse::<u64>()
                        .map_err(|_| CliError::usage("--expect-revision requires an integer"))?,
                );
            }
            "--format" => format = Some(reader.parse_format(false)?),
            _ => return Err(CliError::usage(format!("Unknown flag '{arg}'"))),
        }
        reader.idx += 1;
    }

    let evidence_str = match evidence {
        Some(ev) if !ev.trim().is_empty() => ev,
        _ => return Err(CliError::usage("--evidence is required for 'complete'")),
    };

    let filename =
        resolve_selector(&vault_path, selector).map_err(|e| CliError::usage(e.to_string()))?;

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
        Err(e) => return Err(CliError::usage(e.to_string())),
    }
    Ok(())
}

pub fn close(args: &[String], command: &str, vault_path: &Path) -> CliResult<()> {
    let mut reader = ArgReader::new(args, command);
    let selector = reader.parse_selector()?;
    let mut actor = None;
    let mut note = None;
    let mut expect_revision = None;
    let mut format = None;

    while let Some(arg) = reader.peek()? {
        match arg {
            "--actor" => actor = Some(reader.next_val("--actor")?),
            "--note" => note = Some(reader.next_val("--note")?),
            "--expect-revision" => {
                let val = reader.next_val("--expect-revision")?;
                expect_revision = Some(
                    val.parse::<u64>()
                        .map_err(|_| CliError::usage("--expect-revision requires an integer"))?,
                );
            }
            "--format" => format = Some(reader.parse_format(false)?),
            _ => return Err(CliError::usage(format!("Unknown flag '{arg}'"))),
        }
        reader.idx += 1;
    }

    let filename =
        resolve_selector(&vault_path, selector).map_err(|e| CliError::usage(e.to_string()))?;

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
        Err(e) => return Err(CliError::usage(e.to_string())),
    }
    Ok(())
}

pub fn depend(args: &[String], command: &str, vault_path: &Path) -> CliResult<()> {
    let mut reader = ArgReader::new(args, command);
    if args.len() >= 3 && (args[2] == "--help" || args[2] == "-h") {
        return Err(CliError::Help);
    }
    if args.len() < 4 {
        return Err(CliError::usage(
            "'depend' requires an item selector and a dependency selector",
        ));
    }
    if args[3] == "--help" || args[3] == "-h" {
        return Err(CliError::Help);
    }
    let selector = &args[2];
    let dep_selector = &args[3];
    reader.idx = 4;

    let mut actor = None;
    let mut expect_revision = None;
    let mut format = None;

    while let Some(arg) = reader.peek()? {
        match arg {
            "--actor" => actor = Some(reader.next_val("--actor")?),
            "--expect-revision" => {
                let val = reader.next_val("--expect-revision")?;
                expect_revision = Some(
                    val.parse::<u64>()
                        .map_err(|_| CliError::usage("--expect-revision requires an integer"))?,
                );
            }
            "--format" => format = Some(reader.parse_format(false)?),
            _ => return Err(CliError::usage(format!("Unknown flag '{arg}'"))),
        }
        reader.idx += 1;
    }

    let filename =
        resolve_selector(&vault_path, selector).map_err(|e| CliError::usage(e.to_string()))?;
    let dep_filename =
        resolve_selector(&vault_path, dep_selector).map_err(|e| CliError::usage(e.to_string()))?;
    let dep_content = fs::read_to_string(vault_path.join(&dep_filename))
        .map_err(|e| CliError::usage(format!("Could not read dependency file: {e}")))?;
    let mut issues = Vec::new();
    let dep_meta = parse_front_matter_detailed(&dep_filename, &dep_content, &mut issues)
        .ok_or_else(|| CliError::usage("Invalid front matter in dependency file"))?;

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
        Err(e) => return Err(CliError::usage(e.to_string())),
    }
    Ok(())
}

pub fn parent(args: &[String], command: &str, vault_path: &Path) -> CliResult<()> {
    let mut reader = ArgReader::new(args, command);
    if args.len() >= 3 && (args[2] == "--help" || args[2] == "-h") {
        return Err(CliError::Help);
    }
    if args.len() < 4 {
        return Err(CliError::usage(
            "'parent' requires an item selector and a parent selector",
        ));
    }
    if args[3] == "--help" || args[3] == "-h" {
        return Err(CliError::Help);
    }
    let selector = &args[2];
    let parent_selector = &args[3];
    reader.idx = 4;

    let mut actor = None;
    let mut expect_revision = None;
    let mut format = None;

    while let Some(arg) = reader.peek()? {
        match arg {
            "--actor" => actor = Some(reader.next_val("--actor")?),
            "--expect-revision" => {
                let val = reader.next_val("--expect-revision")?;
                expect_revision = Some(
                    val.parse::<u64>()
                        .map_err(|_| CliError::usage("--expect-revision requires an integer"))?,
                );
            }
            "--format" => format = Some(reader.parse_format(false)?),
            _ => return Err(CliError::usage(format!("Unknown flag '{arg}'"))),
        }
        reader.idx += 1;
    }

    let filename =
        resolve_selector(&vault_path, selector).map_err(|e| CliError::usage(e.to_string()))?;
    let parent_filename = resolve_selector(&vault_path, parent_selector)
        .map_err(|e| CliError::usage(e.to_string()))?;
    let parent_content = fs::read_to_string(vault_path.join(&parent_filename))
        .map_err(|e| CliError::usage(format!("Could not read parent file: {e}")))?;
    let mut issues = Vec::new();
    let parent_meta = parse_front_matter_detailed(&parent_filename, &parent_content, &mut issues)
        .ok_or_else(|| CliError::usage("Invalid front matter in parent file"))?;

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
        Err(e) => return Err(CliError::usage(e.to_string())),
    }
    Ok(())
}

pub fn relate(args: &[String], command: &str, vault_path: &Path) -> CliResult<()> {
    let mut reader = ArgReader::new(args, command);
    if args.len() >= 3 && (args[2] == "--help" || args[2] == "-h") {
        return Err(CliError::Help);
    }
    if args.len() < 4 {
        return Err(CliError::usage(
            "'relate' requires an item selector and a related selector",
        ));
    }
    if args[3] == "--help" || args[3] == "-h" {
        return Err(CliError::Help);
    }
    let selector = &args[2];
    let rel_selector = &args[3];
    reader.idx = 4;

    let mut actor = None;
    let mut expect_revision = None;
    let mut format = None;

    while let Some(arg) = reader.peek()? {
        match arg {
            "--actor" => actor = Some(reader.next_val("--actor")?),
            "--expect-revision" => {
                let val = reader.next_val("--expect-revision")?;
                expect_revision = Some(
                    val.parse::<u64>()
                        .map_err(|_| CliError::usage("--expect-revision requires an integer"))?,
                );
            }
            "--format" => format = Some(reader.parse_format(false)?),
            _ => return Err(CliError::usage(format!("Unknown flag '{arg}'"))),
        }
        reader.idx += 1;
    }

    let filename =
        resolve_selector(&vault_path, selector).map_err(|e| CliError::usage(e.to_string()))?;
    let rel_filename =
        resolve_selector(&vault_path, rel_selector).map_err(|e| CliError::usage(e.to_string()))?;
    let rel_content = fs::read_to_string(vault_path.join(&rel_filename))
        .map_err(|e| CliError::usage(format!("Could not read related file: {e}")))?;
    let mut issues = Vec::new();
    let rel_meta = parse_front_matter_detailed(&rel_filename, &rel_content, &mut issues)
        .ok_or_else(|| CliError::usage("Invalid front matter in related file"))?;

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
        Err(e) => return Err(CliError::usage(e.to_string())),
    }
    Ok(())
}
