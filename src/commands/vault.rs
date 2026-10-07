use crate::cli::{ArgReader, CliError, CliResult};
use crate::doctor::{emit_doctor_report, repair_vault, scan_vault, upgrade_vault};
use crate::frontmatter::{parse_front_matter_detailed, Severity};
use crate::model::{OutputFormat, Resolution};
use crate::output::default_format;
use crate::stats::{calculate_stats, emit_stats};
use crate::vault::{find_repo_root, resolve_selector};
use crate::workflow;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub fn init(args: &[String], command: &str, _vault_path: &Path) -> CliResult<()> {
    let mut reader = ArgReader::new(args, command);
    let (mut is_local, mut project, mut format) = (false, None, None);
    while let Some(arg) = reader.peek()? {
        match arg {
            "--local" => is_local = true,
            "--project" => project = Some(reader.next_val("--project")?.to_string()),
            "--format" => format = Some(reader.parse_format(false)?),
            _ => return Err(CliError::usage(format!("Unknown flag '{arg}'"))),
        }
        reader.idx += 1;
    }

    if !is_local {
        return Err(CliError::usage("'init' currently requires --local"));
    }

    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let root = find_repo_root(&cwd).unwrap_or(cwd);
    let local_vault = root.join(".pin_vault");

    fs::create_dir_all(&local_vault)
        .map_err(|e| CliError::usage(format!("Failed to create vault directory: {e}")))?;
    let _ = fs::write(local_vault.join(".gitkeep"), "");

    let gitignore_path = local_vault.join(".gitignore");
    if !gitignore_path.exists() {
        let _ = fs::write(&gitignore_path, "*.lock\n.*.lock\n.*.edit-recovery.tmp\nruns/\n");
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
    Ok(())
}

pub fn doctor(args: &[String], command: &str, vault_path: &Path) -> CliResult<()> {
    let mut reader = ArgReader::new(args, command);
    let (mut repair, mut upgrade, mut strict, mut format) = (false, false, false, None);
    while let Some(arg) = reader.peek()? {
        match arg {
            "--repair" => repair = true,
            "--upgrade" => upgrade = true,
            "--strict" => strict = true,
            "--format" => format = Some(reader.parse_format(false)?),
            _ => return Err(CliError::usage(format!("Unknown flag '{arg}'"))),
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
        return Err(CliError::Exit(1));
    }
    Ok(())
}

pub fn archive(args: &[String], command: &str, vault_path: &Path) -> CliResult<()> {
    let mut reader = ArgReader::new(args, command);
    let selector = reader.parse_selector()?;
    let (mut resolution, mut note, mut format) = (Resolution::Implemented, None, None);
    let mut actor = None;
    let mut expect_revision = None;

    while let Some(arg) = reader.peek()? {
        match arg {
            "--resolution" => {
                let val = reader.next_val("--resolution")?;
                resolution = val.parse::<Resolution>().map_err(|_| {
                    CliError::usage(
                        "--resolution must be implemented, rejected, superseded, or stale",
                    )
                })?;
            }
            "--note" => note = Some(reader.next_val("--note")?.to_string()),
            "--actor" => actor = Some(reader.next_val("--actor")?.to_string()),
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

    let actor_name = actor.or_else(|| env::var("PIN_ACTOR").ok());
    let meta = workflow::archive_item(
        &vault_path,
        &filename,
        resolution,
        actor_name.as_deref(),
        note.as_deref(),
        expect_revision,
    )
    .map_err(|e| CliError::usage(e.to_string()))?;

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
    Ok(())
}

pub fn unarchive(args: &[String], command: &str, vault_path: &Path) -> CliResult<()> {
    let mut reader = ArgReader::new(args, command);
    let selector = reader.parse_selector()?;
    let mut format = None;
    let mut actor = None;
    let mut expect_revision = None;

    while let Some(arg) = reader.peek()? {
        match arg {
            "--actor" => actor = Some(reader.next_val("--actor")?.to_string()),
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

    let actor_name = actor.or_else(|| env::var("PIN_ACTOR").ok());
    let meta = workflow::unarchive_item(
        &vault_path,
        &filename,
        actor_name.as_deref(),
        expect_revision,
    )
    .map_err(|e| CliError::usage(e.to_string()))?;

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
    Ok(())
}

pub fn transfer(args: &[String], command: &str, vault_path: &Path) -> CliResult<()> {
    let mut reader = ArgReader::new(args, command);
    if args.len() < 3 {
        return Err(CliError::usage(format!("'{command}' requires a directory")));
    }
    if args[2] == "--help" || args[2] == "-h" {
        return Err(CliError::Help);
    }
    let target_path_str = &args[2];
    reader.idx = 3;

    let (mut force, mut format) = (false, None);
    while let Some(arg) = reader.peek()? {
        match arg {
            "--force" => force = true,
            "--format" => format = Some(reader.parse_format(false)?),
            _ => return Err(CliError::usage(format!("Unknown flag '{arg}'"))),
        }
        reader.idx += 1;
    }

    let is_import = command == "import";
    let target_dir = Path::new(target_path_str);

    if is_import {
        if !target_dir.is_dir() {
            return Err(CliError::usage("Source directory not found"));
        }

        let entries = fs::read_dir(target_dir)
            .map_err(|e| CliError::usage(format!("Failed to read source directory: {e}")))?;

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
                let content = fs::read_to_string(&path).map_err(|_| {
                    CliError::usage(format!("'{filename}' is not a valid pin file"))
                })?;

                let mut issues = Vec::new();
                let parsed = parse_front_matter_detailed(&filename, &content, &mut issues);
                let has_errors = issues.iter().any(|i| i.severity == Severity::Error);

                if parsed.is_none() || has_errors {
                    return Err(CliError::usage(format!(
                        "'{filename}' is not a valid pin file"
                    )));
                }
                valid_entries.push((path, filename));
            }
        }

        fs::create_dir_all(&vault_path)
            .map_err(|e| CliError::usage(format!("Failed to create vault directory: {e}")))?;

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
        fs::create_dir_all(target_dir)
            .map_err(|e| CliError::usage(format!("Failed to create export directory: {e}")))?;

        let (mut copied, mut skipped) = (0, 0);
        if let Ok(entries) = fs::read_dir(&vault_path) {
            for entry in entries.flatten() {
                let src_path = entry.path();
                if src_path.is_file() && src_path.extension().and_then(|s| s.to_str()) == Some("md")
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
    Ok(())
}

pub fn stats(args: &[String], command: &str, vault_path: &Path) -> CliResult<()> {
    let mut reader = ArgReader::new(args, command);
    let mut format = None;
    let mut filter_project = None;
    while let Some(arg) = reader.peek()? {
        match arg {
            "--project" => filter_project = Some(reader.next_val("--project")?.to_string()),
            "--format" => format = Some(reader.parse_format(false)?),
            _ => return Err(CliError::usage(format!("Unexpected argument '{arg}'"))),
        }
        reader.idx += 1;
    }

    let stats = calculate_stats(&vault_path, filter_project.as_deref());
    let fmt = format.unwrap_or_else(|| default_format(OutputFormat::Plain));
    emit_stats(&stats, fmt);
    Ok(())
}
