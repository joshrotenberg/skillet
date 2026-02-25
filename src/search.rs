//! Skill search wrapper over the BM25 index.
//!
//! Builds a BM25 index from the skill registry and provides relevance-ranked
//! search over skill metadata fields.

use crate::bm25::{Bm25Index, IndexOptions};
use crate::state::SkillIndex;

/// Common English stop words excluded from indexing.
const STOP_WORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "but", "by", "for", "if", "in", "into", "is", "it",
    "no", "not", "of", "on", "or", "such", "that", "the", "their", "then", "there", "these",
    "they", "this", "to", "was", "will", "with",
];

/// Search index over skills, backed by BM25.
pub struct SkillSearch {
    index: Bm25Index,
}

impl SkillSearch {
    /// Build a search index from the skill index.
    ///
    /// Each skill's latest non-yanked version is indexed as a JSON document
    /// with fields: id, owner, name, description, trigger, categories, tags.
    pub fn build(skill_index: &SkillIndex) -> Self {
        let docs: Vec<serde_json::Value> = skill_index
            .skills
            .values()
            .filter_map(|entry| {
                let v = entry.latest()?;
                let info = &v.metadata.skill;
                let classification = info.classification.as_ref();

                let categories = classification
                    .map(|c| c.categories.join(" "))
                    .unwrap_or_default();
                let tags = classification.map(|c| c.tags.join(" ")).unwrap_or_default();

                Some(serde_json::json!({
                    "id": format!("{}/{}", entry.owner, entry.name),
                    "owner": entry.owner,
                    "name": entry.name,
                    "description": info.description,
                    "trigger": info.trigger.as_deref().unwrap_or(""),
                    "categories": categories,
                    "tags": tags,
                }))
            })
            .collect();

        let options = IndexOptions {
            fields: vec![
                "owner".to_string(),
                "name".to_string(),
                "description".to_string(),
                "trigger".to_string(),
                "categories".to_string(),
                "tags".to_string(),
            ],
            id_field: Some("id".to_string()),
            stopwords: STOP_WORDS.iter().map(|s| s.to_string()).collect(),
            lowercase: true,
            k1: 1.2,
            b: 0.75,
        };

        Self {
            index: Bm25Index::build(&docs, options),
        }
    }

    /// Search skills by query. Returns `(owner, name, score)` tuples sorted
    /// by relevance (highest score first).
    pub fn search(&self, query: &str, limit: usize) -> Vec<(String, String, f64)> {
        self.index
            .search(query, limit)
            .into_iter()
            .filter_map(|result| {
                let (owner, name) = result.id.split_once('/')?;
                Some((owner.to_string(), name.to_string(), result.score))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{
        Classification, Compatibility, SkillEntry, SkillInfo, SkillMetadata, SkillVersion,
    };
    use std::collections::HashMap;

    fn make_entry(owner: &str, name: &str, description: &str, tags: &[&str]) -> SkillEntry {
        SkillEntry {
            owner: owner.to_string(),
            name: name.to_string(),
            versions: vec![SkillVersion {
                version: "1.0.0".to_string(),
                metadata: SkillMetadata {
                    skill: SkillInfo {
                        name: name.to_string(),
                        owner: owner.to_string(),
                        version: "1.0.0".to_string(),
                        description: description.to_string(),
                        trigger: None,
                        license: None,
                        author: None,
                        classification: Some(Classification {
                            categories: vec!["development".to_string()],
                            tags: tags.iter().map(|t| t.to_string()).collect(),
                        }),
                        compatibility: Some(Compatibility {
                            requires_tool_use: None,
                            requires_vision: None,
                            min_context_tokens: None,
                            required_tools: Vec::new(),
                            required_mcp_servers: Vec::new(),
                            verified_with: vec!["claude-opus-4-6".to_string()],
                        }),
                    },
                },
                skill_md: String::new(),
                skill_toml_raw: String::new(),
                yanked: false,
                files: HashMap::new(),
                published: None,
                has_content: true,
            }],
        }
    }

    fn test_index() -> SkillIndex {
        let mut skills = HashMap::new();
        skills.insert(
            ("acme".to_string(), "rust-dev".to_string()),
            make_entry(
                "acme",
                "rust-dev",
                "Rust development standards and conventions",
                &["rust", "cargo", "clippy"],
            ),
        );
        skills.insert(
            ("acme".to_string(), "code-review".to_string()),
            make_entry(
                "acme",
                "code-review",
                "Code review best practices and guidelines",
                &["review", "quality"],
            ),
        );
        skills.insert(
            ("acme".to_string(), "docker-workflow".to_string()),
            make_entry(
                "acme",
                "docker-workflow",
                "Docker container workflow and best practices",
                &["docker", "containers"],
            ),
        );
        skills.insert(
            ("acme".to_string(), "python-dev".to_string()),
            make_entry(
                "acme",
                "python-dev",
                "Python development standards and testing",
                &["python", "pytest"],
            ),
        );
        SkillIndex {
            skills,
            ..Default::default()
        }
    }

    #[test]
    fn test_build_from_skill_index() {
        let idx = test_index();
        let search = SkillSearch::build(&idx);
        assert_eq!(search.index.doc_count, 4);
    }

    #[test]
    fn test_search_by_name() {
        let idx = test_index();
        let search = SkillSearch::build(&idx);
        let results = search.search("rust", 10);

        assert!(!results.is_empty());
        assert_eq!(results[0].0, "acme");
        assert_eq!(results[0].1, "rust-dev");
    }

    #[test]
    fn test_search_by_description() {
        let idx = test_index();
        let search = SkillSearch::build(&idx);
        let results = search.search("code review", 10);

        assert!(!results.is_empty());
        assert_eq!(results[0].0, "acme");
        assert_eq!(results[0].1, "code-review");
    }

    #[test]
    fn test_search_multi_term() {
        let idx = test_index();
        let search = SkillSearch::build(&idx);
        let results = search.search("rust testing", 10);

        // rust-dev should rank above others (matches "rust" in name, desc, tags)
        assert!(!results.is_empty());
        assert_eq!(results[0].0, "acme");
        assert_eq!(results[0].1, "rust-dev");
    }

    #[test]
    fn test_search_no_results() {
        let idx = test_index();
        let search = SkillSearch::build(&idx);
        let results = search.search("kubernetes helm", 10);
        assert!(results.is_empty());
    }
}
