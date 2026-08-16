use crate::frontmatter::parse_front_matter_detailed;
use crate::model::{ArchiveFilter, IdeaMeta, Kind};
use rand::Rng;
use std::env;
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum VaultError {
    #[allow(dead_code)]
    NotFound(PathBuf),
    AmbiguousSelector(String),
    SelectorNotFound(String),
    InvalidSelector(String),
    Io(io::Error),
}

impl fmt::Display for VaultError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VaultError::NotFound(p) => write!(f, "Vault not found at {}", p.display()),
            VaultError::AmbiguousSelector(s) => {
                write!(f, "Selector '{s}' is ambiguous. Use more ID characters.")
            }
            VaultError::SelectorNotFound(s) => write!(f, "No idea matches '{s}'."),
            VaultError::InvalidSelector(s) => write!(f, "Invalid selector '{s}'."),
            VaultError::Io(e) => write!(f, "IO error: {e}"),
        }
    }
}

impl std::error::Error for VaultError {}

impl From<io::Error> for VaultError {
    fn from(e: io::Error) -> Self {
        VaultError::Io(e)
    }
}

pub fn generate_id() -> String {
    let mut bytes = [0u8; 6];
    rand::thread_rng().fill(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn generate_token() -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn validate_selector(selector: &str) -> bool {
    if selector.is_empty()
        || selector.contains('/')
        || selector.contains('\\')
        || selector.contains("..")
    {
        return false;
    }
    selector
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
}

pub fn find_repo_root(start_dir: &Path) -> Option<PathBuf> {
    let mut current = start_dir.to_path_buf();
    loop {
        if current.join(".git").exists()
            || current.join(".pin-project").exists()
            || current.join(".pin_vault").exists()
        {
            return Some(current);
        }
        if !current.pop() {
            break;
        }
    }
    None
}

pub fn resolve_vault_path() -> PathBuf {
    if let Ok(vault_env) = env::var("PIN_VAULT") {
        if !vault_env.trim().is_empty() {
            return PathBuf::from(vault_env.trim());
        }
    }

    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if let Some(root) = find_repo_root(&cwd) {
        let repo_vault = root.join(".pin_vault");
        if repo_vault.is_dir() {
            return repo_vault;
        }
    }

    let home = env::var("HOME")
        .or_else(|_| env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".pin_vault")
}

pub fn resolve_project(override_name: Option<&str>) -> String {
    if let Some(name) = override_name {
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    if let Ok(proj_env) = env::var("PIN_PROJECT") {
        let trimmed = proj_env.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if let Some(root) = find_repo_root(&cwd) {
        let config_file = root.join(".pin-project");
        if let Ok(content) = fs::read_to_string(&config_file) {
            let trimmed = content.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
        if let Some(name) = root.file_name().and_then(|n| n.to_str()) {
            if !name.trim().is_empty() {
                return name.trim().to_string();
            }
        }
    }

    cwd.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("project")
        .to_string()
}

pub fn atomic_write(path: &Path, content: &str) -> io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;

    let mut temp_file = tempfile::NamedTempFile::new_in(parent)?;
    temp_file.write_all(content.as_bytes())?;
    temp_file.flush()?;
    temp_file.persist(path).map_err(|e| e.error)?;
    Ok(())
}

pub fn collect_ideas(vault_path: &Path) -> io::Result<Vec<IdeaMeta>> {
    collect_ideas_filtered(vault_path, None, None, None, None, ArchiveFilter::All)
}

pub fn collect_ideas_filtered(
    vault_path: &Path,
    project: Option<&str>,
    tag: Option<&str>,
    kind: Option<Kind>,
    query: Option<&str>,
    archive_filter: ArchiveFilter,
) -> io::Result<Vec<IdeaMeta>> {
    if !vault_path.is_dir() {
        return Ok(Vec::new());
    }

    let mut ideas = Vec::new();
    let entries = fs::read_dir(vault_path)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("md") {
            let filename = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            if filename.starts_with('.') {
                continue;
            }
            if let Ok(content) = fs::read_to_string(&path) {
                let mut issues = Vec::new();
                if let Some(meta) = parse_front_matter_detailed(&filename, &content, &mut issues) {
                    if !meta.matches_archive_filter(archive_filter) {
                        continue;
                    }
                    if let Some(p) = project {
                        if !meta.project.eq_ignore_ascii_case(p.trim()) {
                            continue;
                        }
                    }
                    if let Some(k) = kind {
                        if meta.kind != k {
                            continue;
                        }
                    }
                    if let Some(t) = tag {
                        let t_norm = t.trim().to_ascii_lowercase();
                        let has_tag = meta
                            .tags_list()
                            .iter()
                            .any(|item| item.to_ascii_lowercase() == t_norm);
                        if !has_tag {
                            continue;
                        }
                    }
                    if let Some(q) = query {
                        if let Some(score) = crate::search::calculate_search_score(&meta, q) {
                            let mut scored_meta = meta;
                            scored_meta.score = Some(score);
                            ideas.push(scored_meta);
                            continue;
                        } else {
                            continue;
                        }
                    }
                    ideas.push(meta);
                }
            }
        }
    }

    // Sort by timestamp descending by default
    ideas.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    Ok(ideas)
}

pub fn resolve_selector(vault_path: &Path, selector: &str) -> Result<String, VaultError> {
    if !validate_selector(selector) {
        return Err(VaultError::InvalidSelector(selector.to_string()));
    }

    // If exact filename ending in .md exists in vault, return it
    if selector.ends_with(".md") {
        let direct_path = vault_path.join(selector);
        if direct_path.is_file() {
            return Ok(selector.to_string());
        }
    }

    let all_ideas = collect_ideas(vault_path).map_err(VaultError::Io)?;
    let mut prefix_match: Option<String> = None;

    for meta in all_ideas {
        if meta.id.eq_ignore_ascii_case(selector) {
            return Ok(meta.filename);
        }
        if meta
            .id
            .to_ascii_lowercase()
            .starts_with(&selector.to_ascii_lowercase())
        {
            if prefix_match.is_some() {
                return Err(VaultError::AmbiguousSelector(selector.to_string()));
            }
            prefix_match = Some(meta.filename);
        }
    }

    prefix_match.ok_or_else(|| VaultError::SelectorNotFound(selector.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selector_validation() {
        assert!(validate_selector("0123456789ab"));
        assert!(validate_selector("prefix-1"));
        assert!(validate_selector("idea.md"));

        assert!(!validate_selector(""));
        assert!(!validate_selector("../secret"));
        assert!(!validate_selector("sub/dir"));
        assert!(!validate_selector("path\\file"));
    }

    #[test]
    fn test_generate_id_format() {
        let id = generate_id();
        assert_eq!(id.len(), 12);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_generate_token_format() {
        let token = generate_token();
        assert_eq!(token.len(), 32);
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
