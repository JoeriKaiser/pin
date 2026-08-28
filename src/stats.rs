use crate::doctor::scan_vault;
use crate::model::{Kind, OutputFormat, Status};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Default, Serialize)]
pub struct VaultStats {
    pub ideas: usize,
    pub technical: usize,
    pub product: usize,
    pub business: usize,
    pub project: usize,
    pub unspecified: usize,
    pub active: usize,
    pub archived: usize,
    pub invalid: usize,
    pub captured: usize,
    pub planned: usize,
    pub in_progress: usize,
    pub blocked: usize,
    pub review: usize,
    pub done: usize,
    pub closed: usize,
    pub cancelled: usize,
}

pub fn calculate_stats(vault_path: &Path) -> VaultStats {
    let scan = scan_vault(vault_path);
    let mut stats = VaultStats {
        ideas: scan.metas.len(),
        invalid: scan.files_scanned.saturating_sub(scan.valid_files),
        ..VaultStats::default()
    };

    for meta in scan.metas {
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

        match meta.current_status() {
            Status::Captured => stats.captured += 1,
            Status::Planned => stats.planned += 1,
            Status::InProgress => stats.in_progress += 1,
            Status::Blocked => stats.blocked += 1,
            Status::Review => stats.review += 1,
            Status::Done => stats.done += 1,
            Status::Closed => stats.closed += 1,
            Status::Cancelled => stats.cancelled += 1,
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
            println!("Technical:   {}", stats.technical);
            println!("Product:     {}", stats.product);
            println!("Business:    {}", stats.business);
            println!("Project:     {}", stats.project);
            println!("Unspecified: {}", stats.unspecified);
            println!("Captured:    {}", stats.captured);
            println!("Planned:     {}", stats.planned);
            println!("In progress: {}", stats.in_progress);
            println!("Blocked:     {}", stats.blocked);
            println!("Review:      {}", stats.review);
            println!("Done:        {}", stats.done);
            println!("Closed:      {}", stats.closed);
            println!("Cancelled:   {}", stats.cancelled);
        }
    }
}
