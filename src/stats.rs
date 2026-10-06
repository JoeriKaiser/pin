use crate::doctor::scan_vault;
use crate::model::{Kind, OutputFormat, Status, WorkType};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Default, Serialize)]
pub struct VaultStats {
    pub ideas: usize,
    pub tasks: usize,
    pub bugs: usize,
    pub decisions: usize,
    pub technical: usize,
    pub product: usize,
    pub business: usize,
    pub project: usize,
    pub unspecified: usize,
    pub active: usize,
    pub archived: usize,
    pub invalid: usize,
    pub created: usize,
    pub planned: usize,
    pub in_progress: usize,
    pub blocked: usize,
    pub done: usize,
    pub closed: usize,
}

pub fn calculate_stats(vault_path: &Path, project: Option<&str>) -> VaultStats {
    let scan = scan_vault(vault_path);
    let mut stats = VaultStats::default();

    if project.is_none() {
        stats.invalid = scan.files_scanned.saturating_sub(scan.valid_files);
    }

    let filtered_metas: Vec<_> = scan
        .metas
        .into_iter()
        .filter(|meta| {
            if let Some(p) = project {
                let trimmed = p.trim();
                if !trimmed.is_empty() && !meta.project.eq_ignore_ascii_case(trimmed) {
                    return false;
                }
            }
            true
        })
        .collect();

    stats.ideas = filtered_metas.len();

    for meta in filtered_metas {
        if meta.is_archived() {
            stats.archived += 1;
        } else {
            stats.active += 1;
        }

        match meta.kind {
            Kind::Technical => stats.technical += 1,
            Kind::Product => stats.product += 1,
            Kind::Business => stats.business += 1,
            Kind::Project => stats.project += 1,
            Kind::Unspecified => stats.unspecified += 1,
        }

        match meta.work_type() {
            WorkType::Task => stats.tasks += 1,
            WorkType::Bug => stats.bugs += 1,
            WorkType::Idea => {}
            WorkType::Decision => stats.decisions += 1,
        }

        match meta.current_status() {
            Status::Created => stats.created += 1,
            Status::Planned => stats.planned += 1,
            Status::InProgress => stats.in_progress += 1,
            Status::Blocked => stats.blocked += 1,
            Status::Done => stats.done += 1,
            Status::Closed => stats.closed += 1,
            Status::Review | Status::Cancelled => {}
        }
    }

    stats
}

pub fn emit_stats(stats: &VaultStats, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string(stats).unwrap_or_default());
        }
        OutputFormat::Table | OutputFormat::Plain => {
            println!("Total ideas: {}", stats.ideas);
            println!("Active:      {}", stats.active);
            println!("Archived:    {}", stats.archived);
            println!("Invalid:     {}", stats.invalid);
            println!("Tasks:       {}", stats.tasks);
            println!("Bugs:        {}", stats.bugs);
            println!("Decisions:   {}", stats.decisions);
            println!("Created:     {}", stats.created);
            println!("Planned:     {}", stats.planned);
            println!("In Progress: {}", stats.in_progress);
            println!("Blocked:     {}", stats.blocked);
            println!("Done:        {}", stats.done);
            println!("Closed:      {}", stats.closed);
            println!("Technical:   {}", stats.technical);
            println!("Product:     {}", stats.product);
            println!("Business:    {}", stats.business);
            println!("Project:     {}", stats.project);
            println!("Unspecified: {}", stats.unspecified);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontmatter::render_full_document;
    use crate::model::IdeaMeta;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_calculate_stats_filtering_and_counts() {
        let dir = tempdir().unwrap();
        let item1 = IdeaMeta::new_work_item(
            "item01".to_string(),
            "projA".to_string(),
            "Task 1".to_string(),
            "Body".to_string(),
            Kind::Technical,
            WorkType::Task,
            Status::Planned,
            None,
            None,
            Some("agent:test".to_string()),
        );
        fs::write(dir.path().join("item01.md"), render_full_document(&item1)).unwrap();

        let item2 = IdeaMeta::new_work_item(
            "item02".to_string(),
            "projB".to_string(),
            "Bug 1".to_string(),
            "Body".to_string(),
            Kind::Product,
            WorkType::Bug,
            Status::Created,
            None,
            None,
            Some("agent:test".to_string()),
        );
        fs::write(dir.path().join("item02.md"), render_full_document(&item2)).unwrap();

        let item3 = IdeaMeta::new_work_item(
            "item03".to_string(),
            "projA".to_string(),
            "Decision 1".to_string(),
            "Body".to_string(),
            Kind::Business,
            WorkType::Decision,
            Status::Done,
            None,
            None,
            Some("agent:test".to_string()),
        );
        fs::write(dir.path().join("item03.md"), render_full_document(&item3)).unwrap();

        let all_stats = calculate_stats(dir.path(), None);
        assert_eq!(all_stats.ideas, 3);
        assert_eq!(all_stats.tasks, 1);
        assert_eq!(all_stats.bugs, 1);
        assert_eq!(all_stats.decisions, 1);
        assert_eq!(all_stats.planned, 1);
        assert_eq!(all_stats.created, 1);
        assert_eq!(all_stats.done, 1);
        assert_eq!(all_stats.technical, 1);
        assert_eq!(all_stats.product, 1);
        assert_eq!(all_stats.business, 1);

        let proj_a_stats = calculate_stats(dir.path(), Some("projA"));
        assert_eq!(proj_a_stats.ideas, 2);
        assert_eq!(proj_a_stats.tasks, 1);
        assert_eq!(proj_a_stats.bugs, 0);
        assert_eq!(proj_a_stats.decisions, 1);
        assert_eq!(proj_a_stats.planned, 1);
        assert_eq!(proj_a_stats.done, 1);
    }
}
