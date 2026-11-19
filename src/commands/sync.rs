use crate::acp::WorktreeManager;
use crate::cli::{resolve_actor, ArgReader, CliError, CliResult};
use crate::model::OutputFormat;
use crate::output::default_format;
use crate::sync::reconcile_and_sync;
use std::path::Path;

pub fn sync(args: &[String], command: &str, vault_path: &Path) -> CliResult<()> {
    let mut reader = ArgReader::new(args, command);
    let mut selector: Option<String> = None;
    let mut project: Option<String> = None;
    let mut dry_run = false;
    let mut force = false;
    let mut format = None;

    while let Some(arg) = reader.peek()? {
        match arg {
            "--dry-run" => dry_run = true,
            "--force" | "-f" => force = true,
            "--project" => project = Some(reader.next_val("--project")?.to_string()),
            "--format" => format = Some(reader.parse_format(false)?),
            flag if flag.starts_with("--") => {
                return Err(CliError::usage(format!("Unknown flag '{flag}'")));
            }
            _ => {
                if selector.is_none() {
                    selector = Some(arg.to_string());
                } else {
                    return Err(CliError::usage(format!("Unexpected argument '{arg}'")));
                }
            }
        }
        reader.idx += 1;
    }

    let repo_path = WorktreeManager::resolve_repo_path(vault_path);
    let actor = resolve_actor(None);
    let filter_project = project.as_deref();
    let target_id = selector.as_deref();

    let results = reconcile_and_sync(
        vault_path,
        &repo_path,
        filter_project,
        target_id,
        dry_run,
        force,
        Some(&actor),
    )
    .map_err(|e| CliError::usage(e.to_string()))?;

    let fmt = format.unwrap_or_else(|| default_format(OutputFormat::Plain));
    if fmt == OutputFormat::Json {
        println!("{}", serde_json::to_string(&results).unwrap_or_default());
    } else if results.is_empty() {
        println!("No merged tickets to sync.");
    } else {
        for res in &results {
            if res.worktree_preserved_dirty {
                println!("Skipped {} ({})", res.id, res.resolution_note);
                println!(
                    "  Worktree has uncommitted changes (preserved; use --force to delete): .pin_worktrees/{}",
                    res.id
                );
            } else {
                let verb = if dry_run { "Would sync" } else { "Synced" };
                println!("{verb} {} ({})", res.id, res.resolution_note);
                if dry_run {
                    if res.worktree_removed {
                        println!("  Would remove worktree: .pin_worktrees/{}", res.id);
                    }
                    if res.branch_deleted {
                        println!("  Would delete branch: pin/{}", res.id);
                    }
                } else {
                    if res.worktree_removed {
                        println!("  Removed worktree: .pin_worktrees/{}", res.id);
                    }
                    if res.branch_deleted {
                        println!("  Deleted branch: pin/{}", res.id);
                    }
                }
            }
        }
    }
    Ok(())
}
