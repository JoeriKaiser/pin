use crate::cli::{ArgReader, CliError, CliResult};
use crate::model::OutputFormat;
use crate::output::default_format;
use crate::update::{check_for_updates, install_update, CURRENT_VERSION};
use std::path::Path;

pub fn update(args: &[String], command: &str, _vault_path: &Path) -> CliResult<()> {
    let mut reader = ArgReader::new(args, command);
    let mut check_only = false;
    let mut force = false;
    let mut check_cache = false;
    let mut format = None;

    while let Some(arg) = reader.peek()? {
        match arg {
            "--check" => check_only = true,
            "--force" => force = true,
            "--check-cache" => check_cache = true,
            "--format" => format = Some(reader.parse_format(false)?),
            _ => return Err(CliError::usage(format!("Unknown flag '{arg}'"))),
        }
        reader.idx += 1;
    }

    if check_cache {
        let _ = check_for_updates(false);
        return Ok(());
    }

    let fmt = format.unwrap_or_else(|| default_format(OutputFormat::Plain));

    if check_only {
        let check = check_for_updates(false).map_err(CliError::usage)?;
        if fmt == OutputFormat::Json {
            println!("{}", serde_json::to_string(&check).unwrap_or_default());
        } else if check.update_available {
            println!(
                "Update available: v{} (current: v{})",
                check.latest_version, check.current_version
            );
            println!("Run 'pin update' to install the update.");
        } else {
            println!("pin is up to date (version {})", check.current_version);
        }
        return Ok(());
    }

    if fmt != OutputFormat::Json {
        println!("Checking for updates...");
    }

    let check = check_for_updates(false).map_err(CliError::usage)?;
    if !check.update_available && !force {
        if fmt == OutputFormat::Json {
            #[derive(serde::Serialize)]
            struct UpToDateJson<'a> {
                current_version: &'a str,
                latest_version: &'a str,
                update_available: bool,
                installed: bool,
                message: &'a str,
            }
            let res = UpToDateJson {
                current_version: CURRENT_VERSION,
                latest_version: &check.latest_version,
                update_available: false,
                installed: false,
                message: "pin is already up to date",
            };
            println!("{}", serde_json::to_string(&res).unwrap_or_default());
        } else {
            println!("pin is already up to date (version {CURRENT_VERSION})");
        }
        return Ok(());
    }

    if fmt != OutputFormat::Json {
        println!(
            "Found new release: v{} (current: v{})",
            check.latest_version, CURRENT_VERSION
        );
        println!("Downloading and installing update...");
    }

    let outcome = install_update(force).map_err(CliError::usage)?;

    if fmt == OutputFormat::Json {
        println!("{}", serde_json::to_string(&outcome).unwrap_or_default());
    } else {
        println!(
            "Successfully updated pin to version {} at {}",
            outcome.updated_version, outcome.target_path
        );
    }

    Ok(())
}
