mod assets;
mod cli;
mod commands;
mod doctor;
mod frontmatter;
mod model;
mod output;
mod search;
mod stats;
mod vault;
mod viewer;
mod workflow;

use cli::{print_usage, CliError, CliResult};
use std::env;
use std::process;
use vault::resolve_vault_path;

const VERSION: &str = "2.2.0";

fn run() -> CliResult<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage();
        return Ok(());
    }

    let cmd = &args[1];
    if cmd == "--help" || cmd == "-h" {
        print_usage();
        return Ok(());
    }
    if cmd == "--version" || cmd == "-v" {
        println!("pin {VERSION}");
        return Ok(());
    }

    let vault_path = resolve_vault_path();

    match cmd.as_str() {
        "init" => commands::vault::init(&args, cmd, &vault_path)?,
        "add" => commands::items::add(&args, cmd, &vault_path)?,
        "list" | "list-project" => commands::items::list(&args, cmd, &vault_path)?,
        "search" => commands::items::search(&args, cmd, &vault_path)?,
        "read" => commands::items::read(&args, cmd, &vault_path)?,
        "edit" => commands::items::edit(&args, cmd, &vault_path)?,
        "rm" => commands::items::rm(&args, cmd, &vault_path)?,
        "context" => commands::context::context(&args, cmd, &vault_path)?,
        "next" => commands::context::next(&args, cmd, &vault_path)?,
        "transition" => commands::work::transition(&args, cmd, &vault_path)?,
        "claim" => commands::work::claim(&args, cmd, &vault_path)?,
        "release" => commands::work::release(&args, cmd, &vault_path)?,
        "handoff" => commands::work::handoff(&args, cmd, &vault_path)?,
        "complete" => commands::work::complete(&args, cmd, &vault_path)?,
        "close" => commands::work::close(&args, cmd, &vault_path)?,
        "depend" => commands::work::depend(&args, cmd, &vault_path)?,
        "parent" => commands::work::parent(&args, cmd, &vault_path)?,
        "relate" => commands::work::relate(&args, cmd, &vault_path)?,
        "doctor" => commands::vault::doctor(&args, cmd, &vault_path)?,
        "archive" => commands::vault::archive(&args, cmd, &vault_path)?,
        "unarchive" => commands::vault::unarchive(&args, cmd, &vault_path)?,
        "import" | "export" => commands::vault::transfer(&args, cmd, &vault_path)?,
        "stats" => commands::vault::stats(&args, cmd, &vault_path)?,
        "view" | "view-project" => commands::view::view(&args, cmd, &vault_path)?,
        unknown => {
            return Err(CliError::UsageWithHelp(format!(
                "Unknown command '{unknown}'"
            )))
        }
    }

    Ok(())
}

fn main() {
    match run() {
        Ok(()) => {}
        Err(CliError::Help) => print_usage(),
        Err(CliError::Usage(message)) => {
            eprintln!("Error: {message}");
            process::exit(1);
        }
        Err(CliError::UsageWithHelp(message)) => {
            eprintln!("Error: {message}");
            print_usage();
            process::exit(1);
        }
        Err(CliError::Exit(code)) => process::exit(code),
    }
}
