use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    Json,
    Table,
    Plain,
}

impl FromStr for OutputFormat {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "json" => Ok(OutputFormat::Json),
            "table" => Ok(OutputFormat::Table),
            "plain" => Ok(OutputFormat::Plain),
            _ => Err(format!("Unknown format: {s}")),
        }
    }
}

impl fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OutputFormat::Json => write!(f, "json"),
            OutputFormat::Table => write!(f, "table"),
            OutputFormat::Plain => write!(f, "plain"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Technical,
    Product,
    Business,
    Project,
    Unspecified,
}

impl Default for Kind {
    fn default() -> Self {
        Kind::Technical
    }
}

impl Kind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Kind::Technical => "technical",
            Kind::Product => "product",
            Kind::Business => "business",
            Kind::Project => "project",
            Kind::Unspecified => "unspecified",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Kind::Technical => "Technical",
            Kind::Product => "Product",
            Kind::Business => "Business",
            Kind::Project => "Project",
            Kind::Unspecified => "Unspecified",
        }
    }

    #[allow(dead_code)]
    pub fn rank_index(&self) -> usize {
        match self {
            Kind::Technical => 0,
            Kind::Product => 1,
            Kind::Business => 2,
            Kind::Project => 3,
            Kind::Unspecified => 4,
        }
    }
}

impl FromStr for Kind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "technical" => Ok(Kind::Technical),
            "product" => Ok(Kind::Product),
            "business" => Ok(Kind::Business),
            "project" => Ok(Kind::Project),
            "unspecified" => Ok(Kind::Unspecified),
            _ => Err(format!("Invalid kind: {s}")),
        }
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkType {
    Task,
    Bug,
    Idea,
    Decision,
}

impl Default for WorkType {
    fn default() -> Self {
        WorkType::Task
    }
}

impl WorkType {
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkType::Task => "task",
            WorkType::Bug => "bug",
            WorkType::Idea => "idea",
            WorkType::Decision => "decision",
        }
    }
}

impl FromStr for WorkType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "task" => Ok(WorkType::Task),
            "bug" => Ok(WorkType::Bug),
            "idea" => Ok(WorkType::Idea),
            "decision" => Ok(WorkType::Decision),
            _ => Err(format!(
                "Invalid work type: {s} (expected task, bug, idea, decision)"
            )),
        }
    }
}

impl fmt::Display for WorkType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Created,
    Planned,
    InProgress,
    Blocked,
    Review,
    Done,
    Closed,
    Cancelled,
}

impl Default for Status {
    fn default() -> Self {
        Status::Created
    }
}

impl Status {
    pub fn as_str(&self) -> &'static str {
        match self {
            Status::Created => "created",
            Status::Planned => "planned",
            Status::InProgress => "in_progress",
            Status::Blocked => "blocked",
            Status::Review => "review",
            Status::Done => "done",
            Status::Closed => "closed",
            Status::Cancelled => "cancelled",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Status::Closed | Status::Cancelled)
    }

    /// True once the work is over: verified (`done`) or withdrawn (`closed`,
    /// `cancelled`). Distinct from `is_terminal`, which excludes `done` so that
    /// an accepted outcome can still be re-verified.
    pub fn is_finished(&self) -> bool {
        matches!(self, Status::Done | Status::Closed | Status::Cancelled)
    }
}

impl FromStr for Status {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "created" | "captured" => Ok(Status::Created),
            "planned" => Ok(Status::Planned),
            "in_progress" | "in-progress" => Ok(Status::InProgress),
            "blocked" => Ok(Status::Blocked),
            "review" => Ok(Status::Review),
            "done" => Ok(Status::Done),
            "closed" => Ok(Status::Closed),
            "cancelled" | "canceled" => Ok(Status::Cancelled),
            _ => Err(format!(
                "Invalid status: {s} (expected created, planned, in_progress, blocked, review, done, closed, cancelled)"
            )),
        }
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Handoff {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocker: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityEvent {
    pub at: i64,
    pub actor: String,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<Status>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<Status>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Low,
    Medium,
    High,
}

impl Priority {
    pub fn as_str(&self) -> &'static str {
        match self {
            Priority::Low => "low",
            Priority::Medium => "medium",
            Priority::High => "high",
        }
    }

    pub fn rank(&self) -> u8 {
        match self {
            Priority::High => 3,
            Priority::Medium => 2,
            Priority::Low => 1,
        }
    }
}

impl FromStr for Priority {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "low" => Ok(Priority::Low),
            "medium" => Ok(Priority::Medium),
            "high" => Ok(Priority::High),
            _ => Err(format!("Invalid priority: {s}")),
        }
    }
}

impl fmt::Display for Priority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Resolution {
    Implemented,
    Rejected,
    Superseded,
    Stale,
}

impl Resolution {
    pub fn as_str(&self) -> &'static str {
        match self {
            Resolution::Implemented => "implemented",
            Resolution::Rejected => "rejected",
            Resolution::Superseded => "superseded",
            Resolution::Stale => "stale",
        }
    }
}

impl FromStr for Resolution {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "implemented" => Ok(Resolution::Implemented),
            "rejected" => Ok(Resolution::Rejected),
            "superseded" => Ok(Resolution::Superseded),
            "stale" => Ok(Resolution::Stale),
            _ => Err(format!("Invalid resolution: {s}")),
        }
    }
}

impl fmt::Display for Resolution {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ArchiveFilter {
    #[default]
    Active,
    Archived,
    All,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdeaMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<u32>,
    pub id: String,
    pub project: String,
    #[serde(default)]
    pub kind: Kind,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub item_type: Option<WorkType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<Status>,
    pub timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at_ns: Option<i64>,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<Priority>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claimed_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claim_expires_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handoff: Option<Handoff>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub activity: Vec<ActivityEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution: Option<Resolution>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution_note: Option<String>,

    #[serde(default)]
    pub filename: String,
    #[serde(default)]
    pub body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<usize>,

    #[serde(skip_serializing, skip_deserializing)]
    pub raw_frontmatter_map: serde_yaml::Mapping,
}

impl IdeaMeta {
    pub fn work_type(&self) -> WorkType {
        self.item_type.unwrap_or_else(|| {
            if self.schema.unwrap_or(1) == 1 {
                WorkType::Idea
            } else {
                WorkType::Task
            }
        })
    }

    pub fn current_status(&self) -> Status {
        self.status.unwrap_or_else(|| {
            if self.is_archived() {
                match self.resolution {
                    Some(Resolution::Rejected | Resolution::Superseded | Resolution::Stale) => {
                        Status::Cancelled
                    }
                    _ => Status::Closed,
                }
            } else {
                Status::Created
            }
        })
    }

    pub fn current_revision(&self) -> u64 {
        self.revision.unwrap_or(0)
    }

    pub fn has_active_claim(&self, now: i64) -> bool {
        self.claimed_by.as_ref().is_some_and(|_| {
            self.claim_expires_at
                .map(|expires| expires > now)
                .unwrap_or(true)
        })
    }

    pub fn is_archived(&self) -> bool {
        self.archived_at.is_some() || self.resolution.is_some()
    }

    pub fn matches_archive_filter(&self, filter: ArchiveFilter) -> bool {
        match filter {
            ArchiveFilter::Active => !self.is_archived(),
            ArchiveFilter::Archived => self.is_archived(),
            ArchiveFilter::All => true,
        }
    }

    pub fn tags_list(&self) -> Vec<String> {
        match &self.tags {
            Some(tags_str) => tags_str
                .split(',')
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect(),
            None => Vec::new(),
        }
    }

    pub fn priority_rank(&self) -> u8 {
        self.priority.map_or(0, |p| p.rank())
    }

    pub fn new_work_item(
        id: String,
        project: String,
        title: String,
        body: String,
        kind: Kind,
        item_type: WorkType,
        status: Status,
        priority: Option<Priority>,
        tags: Option<String>,
        created_by: Option<String>,
    ) -> Self {
        let now = chrono::Utc::now();
        let timestamp = now.timestamp();
        let created_at_ns = now.timestamp_nanos_opt();
        let filename = format!("{id}.md");

        let mut activity = Vec::new();
        if let Some(actor) = &created_by {
            activity.push(ActivityEvent {
                at: timestamp,
                actor: actor.clone(),
                action: "created".to_string(),
                from: None,
                to: Some(status),
                note: None,
            });
        }

        Self {
            schema: Some(2),
            id,
            project,
            kind,
            item_type: Some(item_type),
            status: Some(status),
            timestamp,
            created_at_ns,
            title,
            tags,
            priority,
            updated_at: Some(timestamp),
            revision: Some(1),
            created_by,
            claimed_by: None,
            claim_expires_at: None,
            parent_id: None,
            depends_on: Vec::new(),
            related: Vec::new(),
            handoff: None,
            activity,
            archived_at: None,
            resolution: None,
            resolution_note: None,
            filename,
            body,
            score: None,
            raw_frontmatter_map: serde_yaml::Mapping::new(),
        }
    }
}
