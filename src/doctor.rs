use crate::frontmatter::{
    derive_deterministic_id, parse_front_matter_detailed, render_full_document, Issue, Severity,
};
use crate::model::{IdeaMeta, OutputFormat};
use crate::vault::atomic_write;
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Default)]
pub struct VaultScan {
    pub files_scanned: usize,
    pub valid_files: usize,
    pub issues: Vec<Issue>,
    pub metas: Vec<IdeaMeta>,
}

#[derive(Serialize)]
struct JsonDoctorIssue<'a> {
    code: &'a str,
    severity: &'a str,
    filename: &'a str,
    line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    field: Option<&'a str>,
    message: &'a str,
    repairable: bool,
}

#[derive(Serialize)]
struct JsonDoctorSummary {
    errors: usize,
    warnings: usize,
    info: usize,
    repairable: usize,
}

#[derive(Serialize)]
struct JsonDoctorReport<'a> {
    healthy: bool,
    vault: &'a str,
    files_scanned: usize,
    repaired: usize,
    issues: Vec<JsonDoctorIssue<'a>>,
    summary: JsonDoctorSummary,
}

pub fn scan_vault(vault_path: &Path) -> VaultScan {
    let mut scan = VaultScan::default();
    if !vault_path.is_dir() {
        return scan;
    }

    let mut id_map: HashMap<String, String> = HashMap::new();

    let entries = match fs::read_dir(vault_path) {
        Ok(e) => e,
        Err(_) => return scan,
    };

    let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();

    for path in paths {
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("md") {
            let filename = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            if filename.starts_with('.') {
                continue;
            }
            scan.files_scanned += 1;

            let content = match fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    scan.issues.push(Issue {
                        severity: Severity::Error,
                        code: "unreadable_file".to_string(),
                        filename: filename.clone(),
                        message: format!("Could not read file: {e}"),
                        field: None,
                    });
                    continue;
                }
            };

            let file_issues_before = scan.issues.len();
            let parsed = parse_front_matter_detailed(&filename, &content, &mut scan.issues);

            if let Some(meta) = parsed {
                // Check duplicate ID
                if !meta.id.is_empty() {
                    if let Some(existing_file) = id_map.get(&meta.id) {
                        scan.issues.push(Issue {
                            severity: Severity::Error,
                            code: "duplicate_id".to_string(),
                            filename: filename.clone(),
                            message: format!(
                                "Duplicate ID '{}' already used in '{}'",
                                meta.id, existing_file
                            ),
                            field: Some("id".to_string()),
                        });
                    } else {
                        id_map.insert(meta.id.clone(), filename.clone());
                    }
                }

                let has_errors = scan.issues[file_issues_before..]
                    .iter()
                    .any(|i| i.severity == Severity::Error);

                if !has_errors {
                    scan.valid_files += 1;
                }
                scan.metas.push(meta);
            }
        }
    }

    scan
}

pub fn repair_vault(vault_path: &Path) -> usize {
    if !vault_path.is_dir() {
        return 0;
    }

    let mut repaired_count = 0;
    let entries = match fs::read_dir(vault_path) {
        Ok(e) => e,
        Err(_) => return 0,
    };

    let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();

    for path in paths {
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("md") {
            let filename = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            if filename.starts_with('.') {
                continue;
            }
            let content = match fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let mut issues = Vec::new();
            if let Some(mut meta) = parse_front_matter_detailed(&filename, &content, &mut issues) {
                let mut needs_repair = false;

                if meta.schema != Some(1) {
                    meta.schema = Some(1);
                    needs_repair = true;
                }

                let is_valid_hex_id =
                    meta.id.len() == 12 && meta.id.chars().all(|c| c.is_ascii_hexdigit());
                if !is_valid_hex_id || issues.iter().any(|i| i.code == "missing_id") {
                    meta.id = derive_deterministic_id(&filename);
                    needs_repair = true;
                }

                if needs_repair {
                    let new_doc = render_full_document(&meta);
                    if atomic_write(&path, &new_doc).is_ok() {
                        repaired_count += 1;
                    }
                }
            }
        }
    }

    repaired_count
}

pub fn emit_doctor_report(
    vault_path: &Path,
    scan: &VaultScan,
    repaired: usize,
    format: OutputFormat,
) {
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
    let info_count = scan
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Info)
        .count();
    let repairable_count = scan
        .issues
        .iter()
        .filter(|i| {
            i.code == "missing_schema" || i.code == "missing_id" || i.code == "invalid_schema"
        })
        .count();

    match format {
        OutputFormat::Json => {
            let json_issues: Vec<JsonDoctorIssue> = scan
                .issues
                .iter()
                .map(|i| {
                    let is_repairable = i.code == "missing_schema"
                        || i.code == "missing_id"
                        || i.code == "invalid_schema";
                    JsonDoctorIssue {
                        code: &i.code,
                        severity: match i.severity {
                            Severity::Error => "error",
                            Severity::Warning => "warning",
                            Severity::Info => "info",
                        },
                        filename: &i.filename,
                        line: 1,
                        field: i.field.as_deref(),
                        message: &i.message,
                        repairable: is_repairable,
                    }
                })
                .collect();

            let report = JsonDoctorReport {
                healthy: error_count == 0,
                vault: &vault_path.to_string_lossy(),
                files_scanned: scan.files_scanned,
                repaired,
                issues: json_issues,
                summary: JsonDoctorSummary {
                    errors: error_count,
                    warnings: warning_count,
                    info: info_count,
                    repairable: repairable_count,
                },
            };

            println!("{}", serde_json::to_string(&report).unwrap_or_default());
        }
        OutputFormat::Table | OutputFormat::Plain => {
            println!("Vault: {}", vault_path.display());
            println!("Scanned: {} Markdown file(s)", scan.files_scanned);
            if repaired > 0 {
                println!("Repaired: {repaired} file(s)");
            }
            for issue in &scan.issues {
                let sev_str = match issue.severity {
                    Severity::Error => "error",
                    Severity::Warning => "warning",
                    Severity::Info => "info",
                };
                let rep_str = if issue.code == "missing_schema" || issue.code == "missing_id" {
                    " (repairable)"
                } else {
                    ""
                };
                println!(
                    "{sev_str}: {}: {}: {} [{}]{rep_str}",
                    issue.filename, 1, issue.message, issue.code
                );
            }
            if scan.issues.is_empty() {
                println!("No integrity issues found.");
            }
            println!(
                "Summary: {error_count} error(s), {warning_count} warning(s), {info_count} info"
            );
        }
    }
}
