use crate::model::{
    ActivityEvent, ArchiveFilter, Handoff, IdeaMeta, Kind, OutputFormat,
};
use chrono::DateTime;
use serde::Serialize;
use std::io::IsTerminal;


#[derive(Serialize)]
pub struct JsonIdeaOutput<'a> {
    pub id: &'a str,
    pub filename: &'a str,
    pub project: &'a str,
    pub kind: &'a str,
    #[serde(rename = "type")]
    pub item_type: &'a str,
    pub status: &'a str,
    pub title: &'a str,
    pub timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at_ns: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claimed_by: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claim_expires_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<&'a str>,
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    pub depends_on: &'a [String],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    pub related: &'a [String],
    pub tags: Vec<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handoff: Option<&'a Handoff>,
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    pub activity: &'a [ActivityEvent],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution_note: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<&'a str>,
}

impl<'a> From<&'a IdeaMeta> for JsonIdeaOutput<'a> {
    fn from(meta: &'a IdeaMeta) -> Self {
        let tag_slices: Vec<&'a str> = match &meta.tags {
            Some(t) => t
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect(),
            None => Vec::new(),
        };

        JsonIdeaOutput {
            id: &meta.id,
            filename: &meta.filename,
            project: &meta.project,
            kind: meta.kind.as_str(),
            item_type: meta.work_type().as_str(),
            status: meta.current_status().as_str(),
            title: &meta.title,
            timestamp: meta.timestamp,
            created_at_ns: meta.created_at_ns,
            updated_at: meta.updated_at,
            revision: meta.revision,
            created_by: meta.created_by.as_deref(),
            claimed_by: meta.claimed_by.as_deref(),
            claim_expires_at: meta.claim_expires_at,
            parent_id: meta.parent_id.as_deref(),
            depends_on: &meta.depends_on,
            related: &meta.related,
            tags: tag_slices,
            priority: meta.priority.map(|p| p.as_str()),
            handoff: meta.handoff.as_ref(),
            activity: &meta.activity,
            archived_at: meta.archived_at,
            resolution: meta.resolution.map(|r| r.as_str()),
            resolution_note: meta.resolution_note.as_deref(),
            score: meta.score,
            content: None,
        }
    }
}

pub fn default_format(human_format: OutputFormat) -> OutputFormat {
    if std::io::stdout().is_terminal() {
        human_format
    } else {
        OutputFormat::Json
    }
}

pub fn format_date(timestamp: i64) -> String {
    if let Some(dt) = DateTime::from_timestamp(timestamp, 0) {
        dt.format("%Y-%m-%d").to_string()
    } else {
        "          ".to_string()
    }
}

pub fn emit_ideas(ideas: &[IdeaMeta], format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            let json_items: Vec<JsonIdeaOutput> = ideas.iter().map(JsonIdeaOutput::from).collect();
            println!("{}", serde_json::to_string(&json_items).unwrap_or_default());
        }
        OutputFormat::Plain => {
            for idea in ideas {
                emit_idea_plain(idea);
            }
        }
        OutputFormat::Table => {
            emit_table(ideas);
        }
    }
}

pub fn emit_idea_plain(meta: &IdeaMeta) {
    let mut line = format!(
        "{}  {:<8}  {:<11}  {:<11}  {}",
        meta.id,
        meta.work_type().as_str(),
        meta.current_status().as_str(),
        meta.kind.as_str(),
        meta.title
    );
    if let Some(tags) = &meta.tags {
        if !tags.trim().is_empty() {
            line.push_str(&format!("  [{tags}]"));
        }
    }
    if let Some(priority) = meta.priority {
        line.push_str(&format!("  ({})", priority.as_str()));
    }
    if let Some(claimer) = &meta.claimed_by {
        line.push_str(&format!("  @{claimer}"));
    }
    println!("{line}");
}

pub fn emit_single_idea(meta: &IdeaMeta, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            let json_item = JsonIdeaOutput::from(meta);
            println!("{}", serde_json::to_string(&json_item).unwrap_or_default());
        }
        OutputFormat::Plain => {
            println!("Saved {}  [{}]  {}", meta.id, meta.current_status().as_str(), meta.title);
        }
        OutputFormat::Table => {
            emit_table(std::slice::from_ref(meta));
        }
    }
}

fn emit_table(ideas: &[IdeaMeta]) {
    if ideas.is_empty() {
        println!("No items found.");
        return;
    }

    println!("DATE        TYPE     STATUS       ID            TITLE");
    println!("----------  -------  -----------  ------------  ----------------------------------------");

    for idea in ideas {
        let date_str = if idea.timestamp > 0 {
            format_date(idea.timestamp)
        } else {
            "          ".to_string()
        };

        let title_chars: Vec<char> = idea.title.chars().collect();
        let (display_title, ellipsis) = if title_chars.len() > 40 {
            let s: String = title_chars[..40].iter().collect();
            (s, "...")
        } else {
            (idea.title.clone(), "")
        };

        println!(
            "{date_str}  {:<7}  {:<11}  {:<12}  {display_title}{ellipsis}",
            idea.work_type().as_str(),
            idea.current_status().as_str(),
            idea.id,
        );
    }
    println!("\n{} item(s)", ideas.len());
}

pub fn emit_context(
    ideas: &[IdeaMeta],
    project: &str,
    group_kind: bool,
    archive_filter: ArchiveFilter,
    limit: Option<usize>,
    format: OutputFormat,
) {
    let bounded_ideas = match limit {
        Some(lim) => &ideas[..ideas.len().min(lim)],
        None => ideas,
    };

    match format {
        OutputFormat::Json => {
            let json_items: Vec<JsonIdeaOutput> =
                bounded_ideas.iter().map(JsonIdeaOutput::from).collect();
            println!("{}", serde_json::to_string(&json_items).unwrap_or_default());
        }
        OutputFormat::Plain | OutputFormat::Table => {
            if bounded_ideas.is_empty() {
                return;
            }
            let header_prefix = match archive_filter {
                ArchiveFilter::Active => "Active",
                ArchiveFilter::Archived => "Archived",
                ArchiveFilter::All => "All",
            };
            println!("{header_prefix} proposals for {project}:");

            if group_kind {
                let kinds = [
                    Kind::Technical,
                    Kind::Product,
                    Kind::Business,
                    Kind::Project,
                    Kind::Unspecified,
                ];

                for k in kinds {
                    let matching: Vec<&IdeaMeta> =
                        bounded_ideas.iter().filter(|i| i.kind == k).collect();
                    if matching.is_empty() {
                        continue;
                    }
                    println!("\n{}:", k.label());
                    for idea in matching {
                        let mut line = format!(
                            "- [{}] [{}] [{}] {}",
                            idea.id,
                            idea.work_type().as_str(),
                            idea.current_status().as_str(),
                            idea.title
                        );
                        if let Some(tags) = &idea.tags {
                            if !tags.trim().is_empty() {
                                line.push_str(&format!(" [{tags}]"));
                            }
                        }
                        if let Some(priority) = idea.priority {
                            line.push_str(&format!(" ({})", priority.as_str()));
                        }
                        if let Some(claimed_by) = &idea.claimed_by {
                            if !claimed_by.trim().is_empty() {
                                line.push_str(&format!(" @{claimed_by}"));
                            }
                        }
                        println!("{line}");
                    }
                }
            } else {
                for idea in bounded_ideas {
                    let mut line = format!(
                        "- [{}] [{}] [{}] [{}] {}",
                        idea.id,
                        idea.kind.as_str(),
                        idea.work_type().as_str(),
                        idea.current_status().as_str(),
                        idea.title
                    );
                    if let Some(tags) = &idea.tags {
                        if !tags.trim().is_empty() {
                            line.push_str(&format!(" [{tags}]"));
                        }
                    }
                    if let Some(priority) = idea.priority {
                        line.push_str(&format!(" ({})", priority.as_str()));
                    }
                    if let Some(claimed_by) = &idea.claimed_by {
                        if !claimed_by.trim().is_empty() {
                            line.push_str(&format!(" @{claimed_by}"));
                        }
                    }
                    println!("{line}");
                }
        }
    }
}
}
