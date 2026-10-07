use crate::cli::{ArgReader, CliError, CliResult};
use crate::model::{ArchiveFilter, Kind, OutputFormat, Status, WorkType};
use crate::vault::{collect_ideas_with_filter, resolve_project, FilterOptions};
use crate::viewer::{create_snapshot, serve_view};
use std::io::{self, IsTerminal};
use std::path::Path;

pub fn view(args: &[String], command: &str, vault_path: &Path) -> CliResult<()> {
    let mut reader = ArgReader::new(args, command);
    let project_scoped = command == "view-project";
    let mut filter_project = if project_scoped {
        Some(resolve_project(None))
    } else {
        None
    };
    let (mut filter_tag, mut filter_kind, mut filter_type, mut filter_status) =
        (None, None, None, None);
    let (mut archive_filter, mut port, mut no_open, mut format) =
        (ArchiveFilter::Active, 0, false, None);
    let mut acp_command: Option<String> = None;
    while let Some(arg) = reader.peek()? {
        match arg {
            "--archived" => archive_filter = ArchiveFilter::Archived,
            "--all" => archive_filter = ArchiveFilter::All,
            "--no-open" => no_open = true,
            "--acp-command" => {
                acp_command = Some(reader.next_val("--acp-command")?.to_string());
            }
            "--project" => {
                if project_scoped {
                    return Err(CliError::usage("'view-project' does not accept --project"));
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
            "--port" => {
                let val = reader.next_val("--port")?;
                port = val
                    .parse::<u16>()
                    .map_err(|_| CliError::usage("--port requires an integer port (0-65535)"))?;
            }
            "--format" => format = Some(reader.parse_format(false)?),
            _ => return Err(CliError::usage(format!("Unknown flag '{arg}'"))),
        }
        reader.idx += 1;
    }

    if !io::stdout().is_terminal() && !no_open && format.is_none() {
        return Err(CliError::usage("'view' must be run in an interactive terminal, or with --no-open and an explicit format"));
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
    let snapshot = create_snapshot(
        &ideas,
        vault_path.to_path_buf(),
        scope_label,
        archive_filter,
    );
    let fmt = format.unwrap_or(OutputFormat::Plain);

    serve_view(snapshot, port, no_open, fmt, acp_command)
        .map_err(|e| CliError::usage(format!("Failed to serve view: {e}")))?;
    Ok(())
}
