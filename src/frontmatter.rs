use crate::model::{
    ActivityEvent, Handoff, IdeaMeta, Kind, Priority, Resolution, Status, WorkType,
};
use serde::Serialize;
use serde_yaml::{Mapping, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Severity {
    #[allow(dead_code)]
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub severity: Severity,
    pub code: String,
    pub filename: String,
    pub message: String,
    pub field: Option<String>,
}

pub fn truncate_title(title: &str) -> String {
    let trimmed = title.trim();
    if trimmed.chars().count() > 120 {
        trimmed.chars().take(120).collect()
    } else {
        trimmed.to_string()
    }
}

pub fn get_default_title(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(heading) = trimmed.strip_prefix('#') {
            let title = heading.trim_start_matches('#').trim();
            if !title.is_empty() {
                return Some(truncate_title(title));
            }
        }
    }
    None
}

pub fn split_front_matter(content: &str) -> Option<(&str, &str)> {
    let trimmed_start = content.trim_start_matches('\u{feff}'); // Handle UTF-8 BOM
    if !trimmed_start.starts_with("---") {
        return None;
    }

    let rest = &trimmed_start[3..];
    let rest = rest
        .strip_prefix("\r\n")
        .or_else(|| rest.strip_prefix('\n'))?;

    // Find ending "---"
    let mut offset = 0;
    while let Some(pos) = rest[offset..].find("---") {
        let actual_pos = offset + pos;
        let before = &rest[..actual_pos];
        let after = &rest[actual_pos + 3..];

        let ends_with_newline = before.is_empty() || before.ends_with('\n');
        let starts_with_newline_or_end =
            after.is_empty() || after.starts_with('\n') || after.starts_with("\r\n");

        if ends_with_newline && starts_with_newline_or_end {
            let front_matter = before
                .strip_suffix("\r\n")
                .or_else(|| before.strip_suffix('\n'))
                .unwrap_or(before);
            let body = if let Some(stripped) = after.strip_prefix("\r\n") {
                stripped
            } else if let Some(stripped) = after.strip_prefix('\n') {
                stripped
            } else {
                after
            };
            return Some((front_matter, body));
        }
        offset = actual_pos + 3;
    }

    None
}

pub fn parse_front_matter_detailed(
    filename: &str,
    content: &str,
    issues: &mut Vec<Issue>,
) -> Option<IdeaMeta> {
    let (fm_str, body) = match split_front_matter(content) {
        Some((fm, b)) => (fm, b),
        None => {
            issues.push(Issue {
                severity: Severity::Error,
                code: "missing_front_matter".to_string(),
                filename: filename.to_string(),
                message: "Missing front-matter delimiters (---)".to_string(),
                field: None,
            });
            return None;
        }
    };

    let mapping: Mapping = match serde_yaml::from_str(fm_str) {
        Ok(Value::Mapping(map)) => map,
        Ok(_) | Err(_) => {
            issues.push(Issue {
                severity: Severity::Error,
                code: "invalid_yaml".to_string(),
                filename: filename.to_string(),
                message: "Front matter contains malformed YAML".to_string(),
                field: None,
            });
            return None;
        }
    };

    let mut schema: Option<u32> = None;
    let mut id: Option<String> = None;
    let mut project: Option<String> = None;
    let mut kind: Option<Kind> = None;
    let mut item_type: Option<WorkType> = None;
    let mut status: Option<Status> = None;
    let mut timestamp: Option<i64> = None;
    let mut created_at_ns: Option<i64> = None;
    let mut title: Option<String> = None;
    let mut tags: Option<String> = None;
    let mut priority: Option<Priority> = None;
    let mut updated_at: Option<i64> = None;
    let mut revision: Option<u64> = None;
    let mut created_by: Option<String> = None;
    let mut claimed_by: Option<String> = None;
    let mut claim_expires_at: Option<i64> = None;
    let mut parent_id: Option<String> = None;
    let mut depends_on: Vec<String> = Vec::new();
    let mut related: Vec<String> = Vec::new();
    let mut handoff: Option<Handoff> = None;
    let mut activity: Vec<ActivityEvent> = Vec::new();
    let mut archived_at: Option<i64> = None;
    let mut resolution: Option<Resolution> = None;
    let mut resolution_note: Option<String> = None;

    for (k, v) in &mapping {
        let key = match k.as_str() {
            Some(s) => s,
            None => continue,
        };

        match key {
            "schema" => {
                if let Some(s) = v.as_u64() {
                    schema = Some(s as u32);
                    if s != 1 && s != 2 {
                        issues.push(Issue {
                            severity: Severity::Warning,
                            code: "invalid_schema".to_string(),
                            filename: filename.to_string(),
                            message: format!("Unrecognized schema version '{s}' (expected 1 or 2)"),
                            field: Some("schema".to_string()),
                        });
                    }
                } else {
                    issues.push(Issue {
                        severity: Severity::Warning,
                        code: "invalid_schema".to_string(),
                        filename: filename.to_string(),
                        message: "Invalid schema value (expected integer 1 or 2)".to_string(),
                        field: Some("schema".to_string()),
                    });
                }
            }
            "id" => {
                if let Some(s) = v.as_str() {
                    if s.len() == 12 && s.chars().all(|c| c.is_ascii_hexdigit()) {
                        id = Some(s.to_string());
                    } else {
                        id = Some(s.to_string());
                        issues.push(Issue {
                            severity: Severity::Warning,
                            code: "invalid_id".to_string(),
                            filename: filename.to_string(),
                            message: format!("ID '{s}' is not a 12-character hex string"),
                            field: Some("id".to_string()),
                        });
                    }
                } else {
                    issues.push(Issue {
                        severity: Severity::Warning,
                        code: "invalid_id".to_string(),
                        filename: filename.to_string(),
                        message: "Invalid non-string ID".to_string(),
                        field: Some("id".to_string()),
                    });
                }
            }
            "project" => {
                if let Some(s) = v.as_str() {
                    let trimmed = s.trim();
                    if !trimmed.is_empty() {
                        project = Some(trimmed.to_string());
                    } else {
                        issues.push(Issue {
                            severity: Severity::Error,
                            code: "missing_project".to_string(),
                            filename: filename.to_string(),
                            message: "Project is empty".to_string(),
                            field: Some("project".to_string()),
                        });
                    }
                } else {
                    issues.push(Issue {
                        severity: Severity::Error,
                        code: "missing_project".to_string(),
                        filename: filename.to_string(),
                        message: "Project must be a string".to_string(),
                        field: Some("project".to_string()),
                    });
                }
            }
            "kind" => {
                if let Some(s) = v.as_str() {
                    if let Ok(k) = s.parse::<Kind>() {
                        kind = Some(k);
                    } else {
                        issues.push(Issue {
                            severity: Severity::Warning,
                            code: "invalid_kind".to_string(),
                            filename: filename.to_string(),
                            message: format!("Invalid kind '{s}' (expected technical, product, business, or project)"),
                            field: Some("kind".to_string()),
                        });
                    }
                } else {
                    issues.push(Issue {
                        severity: Severity::Warning,
                        code: "invalid_kind".to_string(),
                        filename: filename.to_string(),
                        message: "Kind must be a string".to_string(),
                        field: Some("kind".to_string()),
                    });
                }
            }
            "type" => {
                if let Some(s) = v.as_str() {
                    if let Ok(t) = s.parse::<WorkType>() {
                        item_type = Some(t);
                    } else {
                        issues.push(Issue {
                            severity: Severity::Warning,
                            code: "invalid_type".to_string(),
                            filename: filename.to_string(),
                            message: format!(
                                "Invalid work type '{s}' (expected idea, task, bug, or decision)"
                            ),
                            field: Some("type".to_string()),
                        });
                    }
                } else {
                    issues.push(Issue {
                        severity: Severity::Warning,
                        code: "invalid_type".to_string(),
                        filename: filename.to_string(),
                        message: "Work type must be a string".to_string(),
                        field: Some("type".to_string()),
                    });
                }
            }
            "status" => {
                if let Some(s) = v.as_str() {
                    if let Ok(value) = s.parse::<Status>() {
                        status = Some(value);
                    } else {
                        issues.push(Issue {
                            severity: Severity::Warning,
                            code: "invalid_status".to_string(),
                            filename: filename.to_string(),
                            message: format!("Invalid status '{s}'"),
                            field: Some("status".to_string()),
                        });
                    }
                } else {
                    issues.push(Issue {
                        severity: Severity::Warning,
                        code: "invalid_status".to_string(),
                        filename: filename.to_string(),
                        message: "Status must be a string".to_string(),
                        field: Some("status".to_string()),
                    });
                }
            }
            "timestamp" => {
                if let Some(t) = v.as_i64() {
                    timestamp = Some(t);
                } else {
                    issues.push(Issue {
                        severity: Severity::Error,
                        code: "missing_timestamp".to_string(),
                        filename: filename.to_string(),
                        message: "Timestamp must be an integer".to_string(),
                        field: Some("timestamp".to_string()),
                    });
                }
            }
            "created_at_ns" => {
                if let Some(t) = v.as_i64() {
                    created_at_ns = Some(t);
                }
            }
            "title" => {
                if let Some(s) = v.as_str() {
                    let trimmed = s.trim();
                    if !trimmed.is_empty() {
                        title = Some(truncate_title(trimmed));
                    }
                }
            }
            "tags" => {
                if let Some(s) = v.as_str() {
                    tags = Some(s.to_string());
                } else if let Some(seq) = v.as_sequence() {
                    let tag_list: Vec<String> = seq
                        .iter()
                        .filter_map(|item| item.as_str().map(|s| s.trim().to_string()))
                        .filter(|s| !s.is_empty())
                        .collect();
                    tags = Some(tag_list.join(", "));
                }
            }
            "priority" => {
                if let Some(s) = v.as_str() {
                    if let Ok(p) = s.parse::<Priority>() {
                        priority = Some(p);
                    } else {
                        issues.push(Issue {
                            severity: Severity::Warning,
                            code: "invalid_priority".to_string(),
                            filename: filename.to_string(),
                            message: format!(
                                "Invalid priority '{s}' (expected low, medium, or high)"
                            ),
                            field: Some("priority".to_string()),
                        });
                    }
                } else {
                    issues.push(Issue {
                        severity: Severity::Warning,
                        code: "invalid_priority".to_string(),
                        filename: filename.to_string(),
                        message: "Priority must be a string".to_string(),
                        field: Some("priority".to_string()),
                    });
                }
            }
            "updated_at" => {
                if let Some(t) = v.as_i64() {
                    updated_at = Some(t);
                } else {
                    issues.push(Issue {
                        severity: Severity::Warning,
                        code: "invalid_updated_at".to_string(),
                        filename: filename.to_string(),
                        message: "Updated timestamp must be an integer".to_string(),
                        field: Some("updated_at".to_string()),
                    });
                }
            }
            "revision" => {
                if let Some(r) = v.as_u64() {
                    revision = Some(r);
                } else {
                    issues.push(Issue {
                        severity: Severity::Warning,
                        code: "invalid_revision".to_string(),
                        filename: filename.to_string(),
                        message: "Revision must be a non-negative integer".to_string(),
                        field: Some("revision".to_string()),
                    });
                }
            }
            "created_by" => {
                created_by = parse_optional_string(v, "created_by", filename, issues);
            }
            "claimed_by" => {
                claimed_by = parse_optional_string(v, "claimed_by", filename, issues);
            }
            "claim_expires_at" => {
                if let Some(t) = v.as_i64() {
                    claim_expires_at = Some(t);
                } else {
                    issues.push(Issue {
                        severity: Severity::Warning,
                        code: "invalid_claim_expires_at".to_string(),
                        filename: filename.to_string(),
                        message: "Claim expiry must be an integer".to_string(),
                        field: Some("claim_expires_at".to_string()),
                    });
                }
            }
            "parent_id" => {
                parent_id = parse_optional_string(v, "parent_id", filename, issues);
            }
            "depends_on" => {
                depends_on = parse_string_list(v, "depends_on", filename, issues);
            }
            "related" => {
                related = parse_string_list(v, "related", filename, issues);
            }
            "handoff" => match serde_yaml::from_value::<Handoff>(v.clone()) {
                Ok(value) => handoff = Some(value),
                Err(_) => issues.push(Issue {
                    severity: Severity::Warning,
                    code: "invalid_handoff".to_string(),
                    filename: filename.to_string(),
                    message: "Handoff must be a mapping".to_string(),
                    field: Some("handoff".to_string()),
                }),
            },
            "activity" => match serde_yaml::from_value::<Vec<ActivityEvent>>(v.clone()) {
                Ok(value) => activity = value,
                Err(_) => issues.push(Issue {
                    severity: Severity::Warning,
                    code: "invalid_activity".to_string(),
                    filename: filename.to_string(),
                    message: "Activity must be a sequence of events".to_string(),
                    field: Some("activity".to_string()),
                }),
            },
            "archived_at" => {
                if let Some(t) = v.as_i64() {
                    archived_at = Some(t);
                }
            }
            "resolution" => {
                if let Some(s) = v.as_str() {
                    if let Ok(r) = s.parse::<Resolution>() {
                        resolution = Some(r);
                    } else {
                        issues.push(Issue {
                            severity: Severity::Warning,
                            code: "invalid_resolution".to_string(),
                            filename: filename.to_string(),
                            message: format!("Invalid resolution '{s}'"),
                            field: Some("resolution".to_string()),
                        });
                    }
                } else {
                    issues.push(Issue {
                        severity: Severity::Warning,
                        code: "invalid_resolution".to_string(),
                        filename: filename.to_string(),
                        message: "Resolution must be a string".to_string(),
                        field: Some("resolution".to_string()),
                    });
                }
            }
            "resolution_note" => {
                if let Some(s) = v.as_str() {
                    resolution_note = Some(s.to_string());
                }
            }
            unknown_key => {
                issues.push(Issue {
                    severity: Severity::Warning,
                    code: "unknown_field".to_string(),
                    filename: filename.to_string(),
                    message: format!("Unknown front matter field '{unknown_key}'"),
                    field: Some(unknown_key.to_string()),
                });
            }
        }
    }

    if schema.is_none() {
        issues.push(Issue {
            severity: Severity::Warning,
            code: "missing_schema".to_string(),
            filename: filename.to_string(),
            message: "Missing schema version (recommended: 2)".to_string(),
            field: Some("schema".to_string()),
        });
    }

    if id.is_none() {
        issues.push(Issue {
            severity: Severity::Warning,
            code: "missing_id".to_string(),
            filename: filename.to_string(),
            message: "Missing unique proposal ID".to_string(),
            field: Some("id".to_string()),
        });
    }

    if project.is_none() {
        issues.push(Issue {
            severity: Severity::Error,
            code: "missing_project".to_string(),
            filename: filename.to_string(),
            message: "Missing project name".to_string(),
            field: Some("project".to_string()),
        });
    }

    if timestamp.is_none() {
        issues.push(Issue {
            severity: Severity::Error,
            code: "missing_timestamp".to_string(),
            filename: filename.to_string(),
            message: "Missing timestamp".to_string(),
            field: Some("timestamp".to_string()),
        });
    }

    if (archived_at.is_some() || resolution.is_some()) && resolution.is_none() {
        issues.push(Issue {
            severity: Severity::Warning,
            code: "missing_resolution".to_string(),
            filename: filename.to_string(),
            message: "Archived idea is missing resolution".to_string(),
            field: Some("resolution".to_string()),
        });
    }

    let final_title = title
        .or_else(|| get_default_title(body))
        .unwrap_or_default();
    let final_id = id.unwrap_or_else(|| derive_deterministic_id(filename));
    let final_project = project.unwrap_or_default();
    let final_timestamp = timestamp.unwrap_or(0);
    let final_kind = kind.unwrap_or(Kind::Unspecified);

    Some(IdeaMeta {
        schema,
        id: final_id,
        project: final_project,
        kind: final_kind,
        item_type,
        status,
        timestamp: final_timestamp,
        created_at_ns,
        title: final_title,
        tags,
        priority,
        updated_at,
        revision,
        created_by,
        claimed_by,
        claim_expires_at,
        parent_id,
        depends_on,
        related,
        handoff,
        activity,
        archived_at,
        resolution,
        resolution_note,
        filename: filename.to_string(),
        body: body.to_string(),
        score: None,
        raw_frontmatter_map: mapping,
    })
}

fn parse_optional_string(
    value: &Value,
    field: &str,
    filename: &str,
    issues: &mut Vec<Issue>,
) -> Option<String> {
    if value.is_null() {
        return None;
    }

    match value.as_str() {
        Some(value) if !value.trim().is_empty() => Some(value.trim().to_string()),
        Some(_) => None,
        None => {
            issues.push(Issue {
                severity: Severity::Warning,
                code: format!("invalid_{field}"),
                filename: filename.to_string(),
                message: format!("{field} must be a string"),
                field: Some(field.to_string()),
            });
            None
        }
    }
}

fn parse_string_list(
    value: &Value,
    field: &str,
    filename: &str,
    issues: &mut Vec<Issue>,
) -> Vec<String> {
    let Some(items) = value.as_sequence() else {
        issues.push(Issue {
            severity: Severity::Warning,
            code: format!("invalid_{field}"),
            filename: filename.to_string(),
            message: format!("{field} must be a sequence of strings"),
            field: Some(field.to_string()),
        });
        return Vec::new();
    };

    let mut result = Vec::with_capacity(items.len());
    for item in items {
        if let Some(value) = item.as_str() {
            if !value.trim().is_empty() {
                result.push(value.trim().to_string());
            }
        } else {
            issues.push(Issue {
                severity: Severity::Warning,
                code: format!("invalid_{field}"),
                filename: filename.to_string(),
                message: format!("Every {field} entry must be a string"),
                field: Some(field.to_string()),
            });
        }
    }
    result
}

pub fn derive_deterministic_id(seed: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    seed.hash(&mut hasher);
    let h1 = hasher.finish();
    let mut hasher2 = DefaultHasher::new();
    (!h1).hash(&mut hasher2);
    let h2 = hasher2.finish();

    let combined = ((h1 as u128) << 64) | (h2 as u128);
    format!("{:012x}", combined & 0x0000_ffff_ffff_ffff)
}

pub fn serialize_front_matter(meta: &IdeaMeta) -> String {
    let mut out = String::from("---\n");
    if let Some(s) = meta.schema {
        out.push_str(&format!("schema: {s}\n"));
    }
    out.push_str(&format!("id: \"{}\"\n", meta.id));
    out.push_str(&format!(
        "project: \"{}\"\n",
        escape_yaml_string(&meta.project)
    ));
    out.push_str(&format!("kind: \"{}\"\n", meta.kind.as_str()));
    if let Some(item_type) = meta.item_type {
        out.push_str(&format!("type: \"{}\"\n", item_type.as_str()));
    }
    if let Some(status) = meta.status {
        out.push_str(&format!("status: \"{}\"\n", status.as_str()));
    }
    out.push_str(&format!("timestamp: {}\n", meta.timestamp));
    if let Some(ns) = meta.created_at_ns {
        out.push_str(&format!("created_at_ns: {ns}\n"));
    }
    out.push_str(&format!("title: \"{}\"\n", escape_yaml_string(&meta.title)));
    if let Some(tags) = &meta.tags {
        if !tags.trim().is_empty() {
            out.push_str(&format!("tags: \"{}\"\n", escape_yaml_string(tags)));
        }
    }
    if let Some(priority) = meta.priority {
        out.push_str(&format!("priority: \"{}\"\n", priority.as_str()));
    }
    if let Some(updated_at) = meta.updated_at {
        out.push_str(&format!("updated_at: {updated_at}\n"));
    }
    if let Some(revision) = meta.revision {
        out.push_str(&format!("revision: {revision}\n"));
    }
    append_quoted_field(&mut out, "created_by", meta.created_by.as_deref());
    append_quoted_field(&mut out, "claimed_by", meta.claimed_by.as_deref());
    if let Some(claim_expires_at) = meta.claim_expires_at {
        out.push_str(&format!("claim_expires_at: {claim_expires_at}\n"));
    }
    append_quoted_field(&mut out, "parent_id", meta.parent_id.as_deref());
    append_yaml_sequence(&mut out, "depends_on", &meta.depends_on);
    append_yaml_sequence(&mut out, "related", &meta.related);
    if let Some(handoff) = &meta.handoff {
        append_yaml_value(&mut out, "handoff", handoff);
    }
    append_yaml_sequence(&mut out, "activity", &meta.activity);
    if let Some(archived_at) = meta.archived_at {
        out.push_str(&format!("archived_at: {archived_at}\n"));
    }
    if let Some(resolution) = meta.resolution {
        out.push_str(&format!("resolution: \"{}\"\n", resolution.as_str()));
    }
    if let Some(note) = &meta.resolution_note {
        if !note.trim().is_empty() {
            out.push_str(&format!(
                "resolution_note: \"{}\"\n",
                escape_yaml_string(note)
            ));
        }
    }

    // Preserve any custom unknown fields that were in raw_frontmatter_map
    for (k, v) in &meta.raw_frontmatter_map {
        if let Some(key) = k.as_str() {
            if matches!(
                key,
                "schema"
                    | "id"
                    | "project"
                    | "kind"
                    | "type"
                    | "status"
                    | "timestamp"
                    | "created_at_ns"
                    | "title"
                    | "tags"
                    | "priority"
                    | "updated_at"
                    | "revision"
                    | "created_by"
                    | "claimed_by"
                    | "claim_expires_at"
                    | "parent_id"
                    | "depends_on"
                    | "related"
                    | "handoff"
                    | "activity"
                    | "archived_at"
                    | "resolution"
                    | "resolution_note"
            ) {
                continue;
            }
            if v.is_mapping() || v.is_sequence() {
                append_yaml_value(&mut out, key, v);
            } else if let Ok(v_str) = serde_yaml::to_string(v) {
                out.push_str(&format!("{key}: {v_str}"));
            }
        }
    }

    out.push_str("---\n");
    out
}

fn append_quoted_field(out: &mut String, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        if !value.trim().is_empty() {
            out.push_str(&format!(
                "{key}: \"{}\"\n",
                escape_yaml_string(value.trim())
            ));
        }
    }
}

fn append_yaml_sequence<T: Serialize>(out: &mut String, key: &str, value: &[T]) {
    if !value.is_empty() {
        append_yaml_value(out, key, value);
    }
}

fn append_yaml_value<T: Serialize + ?Sized>(out: &mut String, key: &str, value: &T) {
    let Ok(serialized) = serde_yaml::to_string(value) else {
        return;
    };

    out.push_str(&format!("{key}:\n"));
    for line in serialized.lines() {
        out.push_str("  ");
        out.push_str(line);
        out.push('\n');
    }
}

pub fn render_full_document(meta: &IdeaMeta) -> String {
    let fm = serialize_front_matter(meta);
    if meta.body.is_empty() {
        fm
    } else {
        format!("{fm}{}", meta.body)
    }
}

fn escape_yaml_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_front_matter_diagnostics_reject_malformed_values() {
        let content = r#"---
schema: "not-a-number"
id: "short"
project: ""
kind: "invalid-kind"
timestamp: "not-int"
priority: "super-high"
resolution: "someday"
unknown_custom_field: true
---
# Invalid Proposal
Body text
"#;
        let mut issues = Vec::new();
        let meta = parse_front_matter_detailed("test.md", content, &mut issues);
        assert!(meta.is_some());

        let codes: Vec<&str> = issues.iter().map(|i| i.code.as_str()).collect();
        assert!(codes.contains(&"invalid_schema"));
        assert!(codes.contains(&"invalid_id"));
        assert!(codes.contains(&"missing_project"));
        assert!(codes.contains(&"invalid_kind"));
        assert!(codes.contains(&"missing_timestamp"));
        assert!(codes.contains(&"invalid_priority"));
        assert!(codes.contains(&"invalid_resolution"));
        assert!(codes.contains(&"unknown_field"));
    }

    #[test]
    fn test_front_matter_rewrite_preserves_body_and_unknown_fields() {
        let content = r#"---
schema: 1
id: "0123456789ab"
project: "test"
kind: "technical"
timestamp: 1700000000
title: "Original Title"
custom_key: "custom_value"
---
# Original Title

Preserved body paragraph.
"#;
        let mut issues = Vec::new();
        let mut meta =
            parse_front_matter_detailed("0123456789ab.md", content, &mut issues).unwrap();
        assert_eq!(meta.title, "Original Title");

        meta.priority = Some(Priority::High);
        meta.title = "Updated Title".to_string();

        let doc = render_full_document(&meta);
        assert!(doc.contains("title: \"Updated Title\""));
        assert!(doc.contains("priority: \"high\""));
        assert!(doc.contains("custom_key: custom_value"));
        assert!(doc.contains("Preserved body paragraph."));
    }

    #[test]
    fn test_title_extraction_from_first_heading() {
        let content = r#"---
schema: 1
id: "0123456789ab"
project: "test"
kind: "product"
timestamp: 1700000000
---
# Automatic Title from Heading

Body details.
"#;
        let mut issues = Vec::new();
        let meta = parse_front_matter_detailed("test.md", content, &mut issues).unwrap();
        assert_eq!(meta.title, "Automatic Title from Heading");
    }

    #[test]
    fn test_work_item_fields_round_trip() {
        let content = r#"---
schema: 2
id: "0123456789ab"
project: "test"
kind: "technical"
type: "task"
status: "in_progress"
timestamp: 1700000000
title: "Work item"
updated_at: 1700000010
revision: 2
created_by: "agent:test"
claimed_by: "agent:test"
claim_expires_at: 1700003600
parent_id: "abcdefabcdef"
depends_on:
  - "fedcbafedcba"
related:
  - "111111111111"
handoff:
  progress: "Half complete"
  next: "Run tests"
activity:
  - at: 1700000010
    actor: "agent:test"
    action: "claimed"
    to: "in_progress"
---
# Work item
"#;
        let mut issues = Vec::new();
        let meta = parse_front_matter_detailed("0123456789ab.md", content, &mut issues).unwrap();
        assert!(issues.is_empty());
        assert_eq!(meta.work_type(), WorkType::Task);
        assert_eq!(meta.current_status(), Status::InProgress);
        assert_eq!(meta.current_revision(), 2);
        assert_eq!(meta.depends_on, vec!["fedcbafedcba"]);
        assert_eq!(
            meta.handoff.as_ref().unwrap().next.as_deref(),
            Some("Run tests")
        );
        assert_eq!(meta.activity.len(), 1);

        let rendered = render_full_document(&meta);
        let mut rendered_issues = Vec::new();
        let parsed =
            parse_front_matter_detailed("0123456789ab.md", &rendered, &mut rendered_issues)
                .unwrap();
        assert!(rendered_issues.is_empty());
        assert_eq!(parsed.current_status(), Status::InProgress);
        assert_eq!(parsed.activity, meta.activity);
    }

    #[test]
    fn legacy_records_get_derived_work_defaults() {
        let content = r#"---
project: "test"
timestamp: 1
title: "Legacy"
---
# Legacy
"#;
        let mut issues = Vec::new();
        let meta = parse_front_matter_detailed("legacy.md", content, &mut issues).unwrap();
        assert_eq!(meta.work_type(), WorkType::Idea);
        assert_eq!(meta.current_status(), Status::Captured);
        assert_eq!(meta.current_revision(), 0);
    }
}
