use crate::model::OutputFormat;
use std::env;
use std::io::{self, IsTerminal, Read};

/// Why a command stopped early. `main` turns these into the process exit
/// status, so no command needs to terminate the process itself.
#[derive(Debug)]
pub enum CliError {
    /// `--help` was requested: print usage on stdout and exit successfully.
    Help,
    /// A user-facing failure: print `Error: <message>` on stderr and exit 1.
    Usage(String),
    /// Same as `Usage`, followed by the command list on stdout.
    UsageWithHelp(String),
    /// Exit with a status and no message, used by `doctor --strict`.
    Exit(i32),
}

pub type CliResult<T> = Result<T, CliError>;

impl CliError {
    pub fn usage(message: impl Into<String>) -> Self {
        CliError::Usage(message.into())
    }
}

pub fn print_usage() {
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
         [--note <text>] [--expect-revision <n>] [--worktree] [--format json|plain]\n  \
         claim <id|prefix|filename> [--actor <name>] [--lease <seconds>]\n        \
         [--expect-revision <n>] [--worktree] [--format json|plain]\n  \
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
         [--status <status>] [--archived|--all] [--port <n>] [--no-open] [--acp-command <cmd>] [--format json|plain]\n  \
         view-project [--tag <name>] [--kind <kind>] [--type <type>] [--status <status>]\n               \
         [--archived|--all] [--port <n>] [--no-open] [--acp-command <cmd>] [--format json|plain]\n  \
         --help\n  \
         --version\n"
    );
}

pub struct ArgReader<'a> {
    args: &'a [String],
    pub idx: usize,
    command: &'a str,
}

impl<'a> ArgReader<'a> {
    pub fn new(args: &'a [String], command: &'a str) -> Self {
        Self {
            args,
            idx: 2,
            command,
        }
    }

    pub fn peek(&self) -> CliResult<Option<&'a str>> {
        let Some(arg) = self.args.get(self.idx).map(|s| s.as_str()) else {
            return Ok(None);
        };
        if arg == "--help" || arg == "-h" {
            return Err(CliError::Help);
        }
        Ok(Some(arg))
    }

    pub fn next_val(&mut self, flag: &str) -> CliResult<&'a str> {
        if self.idx + 1 >= self.args.len() {
            return Err(CliError::usage(format!("{flag} requires a value")));
        }
        self.idx += 1;
        Ok(&self.args[self.idx])
    }

    pub fn parse_format(&mut self, allow_table: bool) -> CliResult<OutputFormat> {
        let val = self.next_val("--format")?;
        match val.parse::<OutputFormat>() {
            Ok(fmt) => {
                if !allow_table && fmt == OutputFormat::Table {
                    return Err(CliError::usage(format!(
                        "--format must be json or plain for '{}'",
                        self.command
                    )));
                }
                Ok(fmt)
            }
            Err(_) if allow_table => Err(CliError::usage(format!(
                "--format must be json, table, or plain for '{}'",
                self.command
            ))),
            Err(_) => Err(CliError::usage(format!(
                "--format must be json or plain for '{}'",
                self.command
            ))),
        }
    }

    pub fn parse_selector(&mut self) -> CliResult<&'a str> {
        if self.args.len() < 3 {
            return Err(CliError::usage(format!(
                "'{}' requires an ID, ID prefix, or filename",
                self.command
            )));
        }
        if self.args[2] == "--help" || self.args[2] == "-h" {
            return Err(CliError::Help);
        }
        self.idx = 3;
        Ok(&self.args[2])
    }
}

pub fn read_stdin_content() -> Option<String> {
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

pub fn resolve_actor(explicit: Option<&str>) -> String {
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

pub fn split_shell_words(cmd: &str) -> Vec<String> {
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
