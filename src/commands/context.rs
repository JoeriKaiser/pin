use crate::cli::{ArgReader, CliError, CliResult};
use crate::model::{ArchiveFilter, Kind, OutputFormat, Status, WorkType};
use crate::output::{default_format, emit_context, emit_ideas};
use crate::vault::{collect_ideas, collect_ideas_with_filter, resolve_project, FilterOptions};
use crate::workflow;
use std::path::Path;

pub fn context(args: &[String], command: &str, vault_path: &Path) -> CliResult<()> {
    let mut reader = ArgReader::new(args, command);
    let (mut filter_project, mut filter_kind, mut filter_type, mut filter_status, mut limit) =
        (None, None, None, None, None);
    let (mut archive_filter, mut group_kind, mut format) = (ArchiveFilter::Active, false, None);

    while let Some(arg) = reader.peek()? {
        match arg {
            "--archived" => archive_filter = ArchiveFilter::Archived,
            "--all" => archive_filter = ArchiveFilter::All,
            "--project" => filter_project = Some(reader.next_val("--project")?.to_string()),
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
            "--limit" => {
                let val = reader.next_val("--limit")?;
                limit = Some(
                    val.parse::<usize>()
                        .map_err(|_| CliError::usage("--limit requires a positive integer"))?,
                );
            }
            "--group" => {
                let val = reader.next_val("--group")?;
                if val == "kind" {
                    group_kind = true;
                } else {
                    return Err(CliError::usage("--group only supports 'kind'"));
                }
            }
            "--format" => format = Some(reader.parse_format(false)?),
            _ => return Err(CliError::usage(format!("Unknown flag '{arg}'"))),
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
    Ok(())
}

pub fn next(args: &[String], command: &str, vault_path: &Path) -> CliResult<()> {
    let mut reader = ArgReader::new(args, command);
    let mut filter_project = None;
    let mut limit = Some(1);
    let mut format = None;

    while let Some(arg) = reader.peek()? {
        match arg {
            "--project" => filter_project = Some(reader.next_val("--project")?.to_string()),
            "--limit" => {
                let val = reader.next_val("--limit")?;
                limit = Some(
                    val.parse::<usize>()
                        .map_err(|_| CliError::usage("--limit requires a positive integer"))?,
                );
            }
            "--format" => format = Some(reader.parse_format(false)?),
            _ => return Err(CliError::usage(format!("Unknown flag '{arg}'"))),
        }
        reader.idx += 1;
    }

    let proj = resolve_project(filter_project.as_deref());
    let all_items = collect_ideas(&vault_path).unwrap_or_default();
    let ready = workflow::next_ready_items(&all_items, Some(&proj), limit.unwrap_or(1));

    let fmt = format.unwrap_or_else(|| default_format(OutputFormat::Plain));
    emit_ideas(&ready, fmt);
    Ok(())
}
