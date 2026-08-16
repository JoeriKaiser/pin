use crate::model::IdeaMeta;

pub fn contains_word_ignore_case(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let h_lower = haystack.to_ascii_lowercase();
    let n_lower = needle.to_ascii_lowercase();
    h_lower.contains(&n_lower)
}

pub fn calculate_search_score(meta: &IdeaMeta, query: &str) -> Option<usize> {
    let terms: Vec<&str> = query.split_whitespace().filter(|s| !s.is_empty()).collect();
    if terms.is_empty() {
        return Some(0);
    }

    let mut total_score: usize = 0;
    let tags_str = meta.tags.as_deref().unwrap_or("");

    for term in terms {
        let in_title = contains_word_ignore_case(&meta.title, term);
        let in_tags = contains_word_ignore_case(tags_str, term);
        let in_body = contains_word_ignore_case(&meta.body, term);

        if in_title {
            total_score += 100;
        } else if in_tags {
            total_score += 50;
        } else if in_body {
            total_score += 10;
        } else {
            return None; // AND matching: all terms must match somewhere
        }
    }

    Some(total_score)
}

pub fn sort_search_results(ideas: &mut [IdeaMeta]) {
    ideas.sort_by(|a, b| {
        let score_cmp = b.score.unwrap_or(0).cmp(&a.score.unwrap_or(0));
        if score_cmp != std::cmp::Ordering::Equal {
            return score_cmp;
        }

        let prio_cmp = b.priority_rank().cmp(&a.priority_rank());
        if prio_cmp != std::cmp::Ordering::Equal {
            return prio_cmp;
        }

        b.timestamp.cmp(&a.timestamp)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Kind, Priority};

    fn make_idea(
        id: &str,
        title: &str,
        tags: Option<&str>,
        body: &str,
        priority: Option<Priority>,
        timestamp: i64,
    ) -> IdeaMeta {
        IdeaMeta {
            schema: Some(1),
            id: id.to_string(),
            project: "test".to_string(),
            kind: Kind::Technical,
            timestamp,
            created_at_ns: None,
            title: title.to_string(),
            tags: tags.map(|s| s.to_string()),
            priority,
            archived_at: None,
            resolution: None,
            resolution_note: None,
            filename: format!("{id}.md"),
            body: body.to_string(),
            score: None,
            raw_frontmatter_map: serde_yaml::Mapping::new(),
        }
    }

    #[test]
    fn test_search_ranks_title_matches_above_body_matches() {
        let mut ideas = vec![
            make_idea(
                "01",
                "Incidental Note",
                None,
                "This body discusses ranked retrieval results.",
                None,
                100,
            ),
            make_idea(
                "02",
                "Ranked retrieval results",
                None,
                "Direct title match.",
                None,
                100,
            ),
        ];

        for idea in &mut ideas {
            idea.score = calculate_search_score(idea, "ranked results");
        }

        assert_eq!(ideas[0].score, Some(20)); // 2 body matches: 10 + 10 = 20
        assert_eq!(ideas[1].score, Some(200)); // 2 title matches: 100 + 100 = 200

        sort_search_results(&mut ideas);
        assert_eq!(ideas[0].title, "Ranked retrieval results");
    }

    #[test]
    fn test_search_multi_term_and_matching() {
        let idea = make_idea(
            "01",
            "Cache Optimization",
            Some("perf, latency"),
            "Improves query throughput.",
            None,
            100,
        );

        // All terms present across title, tags, body
        assert!(calculate_search_score(&idea, "cache latency throughput").is_some());

        // One term missing
        assert!(calculate_search_score(&idea, "cache database").is_none());
    }
}
