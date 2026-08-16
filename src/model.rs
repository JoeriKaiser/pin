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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFilter {
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
    pub kind: Kind,
    pub timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at_ns: Option<i64>,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<Priority>,
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
}
