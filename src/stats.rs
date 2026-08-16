use crate::doctor::scan_vault;
use crate::model::{Kind, OutputFormat};
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
}

pub fn calculate_stats(vault_path: &Path) -> VaultStats {
    let scan = scan_vault(vault_path);
    let mut stats = VaultStats::default();

    stats.ideas = scan.metas.len();
    stats.invalid = scan.files_scanned.saturating_sub(scan.valid_files);

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
        }
    }
}
