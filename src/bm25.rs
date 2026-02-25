//! BM25 search indexing for skill discovery.
//!
//! Vendored from jpx-engine with modifications for the skillet registry:
//! removed serde derives (index is ephemeral), removed source document storage
//! (skill data lives in SkillIndex), removed portability fields.
//!
//! # BM25 Formula
//!
//! ```text
//! score(D,Q) = Σ IDF(qi) * (f(qi,D) * (k1 + 1)) / (f(qi,D) + k1 * (1 - b + b * |D|/avgdl))
//! ```
//!
//! Where:
//! - f(qi,D) = term frequency of qi in document D
//! - |D| = document length
//! - avgdl = average document length
//! - k1 = term frequency saturation parameter (default 1.2)
//! - b = length normalization parameter (default 0.75)

use std::collections::HashMap;

/// BM25 index structure
#[derive(Debug, Clone)]
pub struct Bm25Index {
    /// Index configuration
    pub options: IndexOptions,

    /// Total number of documents
    pub doc_count: usize,

    /// Average document length (in tokens)
    pub avg_doc_length: f64,

    /// Document metadata: id -> DocInfo
    pub docs: HashMap<String, DocInfo>,

    /// Inverted index: term -> TermInfo
    pub terms: HashMap<String, TermInfo>,
}

/// Index configuration options
#[derive(Debug, Clone)]
pub struct IndexOptions {
    /// Fields to index (empty = treat input as text)
    pub fields: Vec<String>,

    /// Field to use as document ID (default: array index)
    pub id_field: Option<String>,

    /// Normalize case (default: true)
    pub lowercase: bool,

    /// Terms to exclude from indexing
    pub stopwords: Vec<String>,

    /// BM25 k1 parameter (term frequency saturation)
    pub k1: f64,

    /// BM25 b parameter (length normalization)
    pub b: f64,
}

impl Default for IndexOptions {
    fn default() -> Self {
        Self {
            fields: Vec::new(),
            id_field: None,
            lowercase: true,
            stopwords: Vec::new(),
            k1: 1.2,
            b: 0.75,
        }
    }
}

/// Document metadata
#[derive(Debug, Clone)]
pub struct DocInfo {
    /// Document length in tokens
    pub length: usize,

    /// Per-field token counts (for multi-field indices).
    /// Used during index construction; retained for potential field-level scoring.
    #[allow(dead_code)]
    pub field_lengths: HashMap<String, usize>,
}

/// Term information in the inverted index
#[derive(Debug, Clone)]
pub struct TermInfo {
    /// Document frequency (number of documents containing this term)
    pub df: usize,

    /// Postings: doc_id -> term frequency in that document
    pub postings: HashMap<String, usize>,
}

/// Search result
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// Document ID
    pub id: String,

    /// BM25 score
    pub score: f64,

    /// Matched terms. Retained for potential match highlighting.
    #[allow(dead_code)]
    pub matches: HashMap<String, Vec<String>>,
}

impl Bm25Index {
    /// Create a new empty index with the given options
    pub fn new(options: IndexOptions) -> Self {
        Self {
            options,
            doc_count: 0,
            avg_doc_length: 0.0,
            docs: HashMap::new(),
            terms: HashMap::new(),
        }
    }

    /// Build an index from an array of documents
    pub fn build(docs: &[serde_json::Value], options: IndexOptions) -> Self {
        let mut index = Self::new(options);
        let mut total_length = 0usize;

        for (i, doc) in docs.iter().enumerate() {
            let doc_id = index.get_doc_id(doc, i);
            let (tokens, field_lengths) = index.tokenize_doc(doc);
            let doc_length = tokens.len();
            total_length += doc_length;

            // Store document info
            index.docs.insert(
                doc_id.clone(),
                DocInfo {
                    length: doc_length,
                    field_lengths,
                },
            );

            // Update inverted index
            let mut term_freqs: HashMap<String, usize> = HashMap::new();
            for token in tokens {
                *term_freqs.entry(token).or_insert(0) += 1;
            }

            for (term, freq) in term_freqs {
                let term_info = index.terms.entry(term).or_insert(TermInfo {
                    df: 0,
                    postings: HashMap::new(),
                });
                term_info.df += 1;
                term_info.postings.insert(doc_id.clone(), freq);
            }

            index.doc_count += 1;
        }

        // Calculate average document length
        if index.doc_count > 0 {
            index.avg_doc_length = total_length as f64 / index.doc_count as f64;
        }

        index
    }

    /// Get document ID from a document
    fn get_doc_id(&self, doc: &serde_json::Value, index: usize) -> String {
        if let Some(id) = self
            .options
            .id_field
            .as_ref()
            .and_then(|id_field| doc.get(id_field))
        {
            return match id {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                _ => format!("{}", index),
            };
        }
        format!("{}", index)
    }

    /// Tokenize a document into terms
    fn tokenize_doc(&self, doc: &serde_json::Value) -> (Vec<String>, HashMap<String, usize>) {
        let mut tokens = Vec::new();
        let mut field_lengths = HashMap::new();

        if self.options.fields.is_empty() {
            // Treat entire doc as text
            let text = self.extract_text(doc);
            tokens = self.tokenize_text(&text);
        } else {
            // Index specific fields
            for field in &self.options.fields {
                if let Some(value) = doc.get(field) {
                    let text = self.extract_text(value);
                    let field_tokens = self.tokenize_text(&text);
                    field_lengths.insert(field.clone(), field_tokens.len());
                    tokens.extend(field_tokens);
                }
            }
        }

        (tokens, field_lengths)
    }

    /// Extract text from a JSON value
    fn extract_text(&self, value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Array(arr) => arr
                .iter()
                .filter_map(|v| {
                    if let serde_json::Value::String(s) = v {
                        Some(s.as_str())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join(" "),
            serde_json::Value::Object(obj) => obj
                .values()
                .map(|v| self.extract_text(v))
                .collect::<Vec<_>>()
                .join(" "),
            _ => String::new(),
        }
    }

    /// Tokenize text into terms
    pub fn tokenize_text(&self, text: &str) -> Vec<String> {
        let text = if self.options.lowercase {
            text.to_lowercase()
        } else {
            text.to_string()
        };

        text.split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|s| !s.is_empty())
            .filter(|s| !self.options.stopwords.contains(&s.to_string()))
            .map(stem_simple)
            .collect()
    }
}

/// Simple plural stemmer for search indexing.
///
/// Handles common English plural forms:
/// - "databases" -> "database" (strip -s after vowel+consonant+e pattern)
/// - "ACLs" -> "ACL" (strip -s)
/// - "queries" -> "query" (ies -> y)
/// - "boxes" -> "box" (strip -es after x/z)
///
/// This is intentionally simple - it improves recall for plural/singular
/// matching without the complexity of a full Porter stemmer.
fn stem_simple(term: &str) -> String {
    let t = term.to_string();
    let len = t.len();

    // Skip very short terms
    if len < 3 {
        return t;
    }

    // Handle -ies -> -y (queries -> query, entries -> entry)
    if len > 3 && t.ends_with("ies") {
        return format!("{}y", &t[..len - 3]);
    }

    // Handle -xes -> -x and -zes -> -z (boxes -> box, buzzes handled by -ss check)
    if len > 3 && (t.ends_with("xes") || t.ends_with("zes")) {
        return t[..len - 2].to_string();
    }

    // Handle -sses -> -ss (classes -> class, but keep the ss)
    if len > 4 && t.ends_with("sses") {
        return t[..len - 2].to_string();
    }

    // Handle -shes -> -sh (dishes -> dish)
    if len > 4 && t.ends_with("shes") {
        return t[..len - 2].to_string();
    }

    // Handle simple -s (but not -ss like "lass", "class", "boss")
    // This covers: databases -> database, caches -> cache, shards -> shard
    if t.ends_with('s') && !t.ends_with("ss") {
        return t[..len - 1].to_string();
    }

    t
}

impl Bm25Index {
    /// Calculate IDF for a term
    fn idf(&self, term: &str) -> f64 {
        let df = self.terms.get(term).map(|t| t.df as f64).unwrap_or(0.0);

        if df == 0.0 {
            return 0.0;
        }

        let n = self.doc_count as f64;
        // IDF formula: ln((N - df + 0.5) / (df + 0.5) + 1)
        ((n - df + 0.5) / (df + 0.5) + 1.0).ln()
    }

    /// Calculate BM25 score for a document given query terms
    fn score_doc(&self, doc_id: &str, query_terms: &[String]) -> f64 {
        let doc_info = match self.docs.get(doc_id) {
            Some(info) => info,
            None => return 0.0,
        };

        let doc_length = doc_info.length as f64;
        let k1 = self.options.k1;
        let b = self.options.b;
        let avgdl = self.avg_doc_length;

        let mut score = 0.0;

        for term in query_terms {
            let idf = self.idf(term);
            let tf = self
                .terms
                .get(term)
                .and_then(|t| t.postings.get(doc_id))
                .copied()
                .unwrap_or(0) as f64;

            if tf > 0.0 {
                // BM25 formula
                let numerator = tf * (k1 + 1.0);
                let denominator = tf + k1 * (1.0 - b + b * doc_length / avgdl);
                score += idf * numerator / denominator;
            }
        }

        score
    }

    /// Search the index
    pub fn search(&self, query: &str, top_k: usize) -> Vec<SearchResult> {
        let query_terms = self.tokenize_text(query);

        if query_terms.is_empty() {
            return Vec::new();
        }

        // Find candidate documents (those containing at least one query term)
        let mut candidates: HashMap<String, f64> = HashMap::new();

        for term in &query_terms {
            if let Some(term_info) = self.terms.get(term) {
                for doc_id in term_info.postings.keys() {
                    candidates.entry(doc_id.clone()).or_insert(0.0);
                }
            }
        }

        // Score all candidates
        let mut results: Vec<SearchResult> = candidates
            .keys()
            .map(|doc_id| {
                let score = self.score_doc(doc_id, &query_terms);
                let matches = self.get_matches(doc_id, &query_terms);

                SearchResult {
                    id: doc_id.clone(),
                    score,
                    matches,
                }
            })
            .filter(|r| r.score > 0.0)
            .collect();

        // Sort by score descending
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Return top_k results
        results.truncate(top_k);
        results
    }

    /// Get matched terms for a document
    fn get_matches(&self, doc_id: &str, query_terms: &[String]) -> HashMap<String, Vec<String>> {
        let mut matches: HashMap<String, Vec<String>> = HashMap::new();

        for term in query_terms {
            if self
                .terms
                .get(term)
                .is_some_and(|term_info| term_info.postings.contains_key(doc_id))
            {
                matches
                    .entry("_matched".to_string())
                    .or_default()
                    .push(term.clone());
            }
        }

        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_build_index_simple() {
        let docs = vec![
            json!("hello world"),
            json!("hello there"),
            json!("goodbye world"),
        ];

        let index = Bm25Index::build(&docs, IndexOptions::default());

        assert_eq!(index.doc_count, 3);
        assert!(index.terms.contains_key("hello"));
        assert!(index.terms.contains_key("world"));
        assert_eq!(index.terms.get("hello").unwrap().df, 2);
        assert_eq!(index.terms.get("world").unwrap().df, 2);
    }

    #[test]
    fn test_build_index_with_fields() {
        let docs = vec![
            json!({"name": "create_cluster", "description": "Create a new cluster"}),
            json!({"name": "delete_cluster", "description": "Delete an existing cluster"}),
            json!({"name": "list_backups", "description": "List all backups"}),
        ];

        let options = IndexOptions {
            fields: vec!["name".to_string(), "description".to_string()],
            id_field: Some("name".to_string()),
            ..Default::default()
        };

        let index = Bm25Index::build(&docs, options);

        assert_eq!(index.doc_count, 3);
        assert!(index.docs.contains_key("create_cluster"));
        assert!(index.docs.contains_key("delete_cluster"));
        assert!(index.terms.contains_key("cluster"));
        assert_eq!(index.terms.get("cluster").unwrap().df, 2);
    }

    #[test]
    fn test_search_basic() {
        let docs = vec![
            json!({"name": "create_cluster", "description": "Create a new Redis cluster"}),
            json!({"name": "delete_cluster", "description": "Delete an existing cluster"}),
            json!({"name": "create_backup", "description": "Create a backup of data"}),
        ];

        let options = IndexOptions {
            fields: vec!["name".to_string(), "description".to_string()],
            id_field: Some("name".to_string()),
            ..Default::default()
        };

        let index = Bm25Index::build(&docs, options);
        let results = index.search("cluster", 10);

        assert_eq!(results.len(), 2);
        let ids: Vec<_> = results.iter().map(|r| r.id.as_str()).collect();
        assert!(ids.contains(&"create_cluster"));
        assert!(ids.contains(&"delete_cluster"));
    }

    #[test]
    fn test_search_ranking() {
        let docs = vec![
            json!({"name": "cluster_manager", "description": "Manage cluster operations"}),
            json!({"name": "backup_tool", "description": "Backup tool for cluster data"}),
            json!({"name": "monitor", "description": "Monitor system health"}),
        ];

        let options = IndexOptions {
            fields: vec!["name".to_string(), "description".to_string()],
            id_field: Some("name".to_string()),
            ..Default::default()
        };

        let index = Bm25Index::build(&docs, options);
        let results = index.search("cluster", 10);

        // cluster_manager should rank higher (has "cluster" in both name and description)
        assert!(!results.is_empty());
        assert_eq!(results[0].id, "cluster_manager");
    }

    #[test]
    fn test_search_multi_term() {
        let docs = vec![
            json!({"name": "create_backup", "description": "Create a backup in a region"}),
            json!({"name": "restore_backup", "description": "Restore from backup"}),
            json!({"name": "list_regions", "description": "List available regions"}),
        ];

        let options = IndexOptions {
            fields: vec!["name".to_string(), "description".to_string()],
            id_field: Some("name".to_string()),
            ..Default::default()
        };

        let index = Bm25Index::build(&docs, options);
        let results = index.search("backup region", 10);

        // create_backup should rank highest (has both terms)
        assert!(!results.is_empty());
        assert_eq!(results[0].id, "create_backup");
    }

    #[test]
    fn test_stopwords() {
        let docs = vec![json!("the quick brown fox"), json!("the lazy dog")];

        let options = IndexOptions {
            stopwords: vec!["the".to_string()],
            ..Default::default()
        };

        let index = Bm25Index::build(&docs, options);

        assert!(!index.terms.contains_key("the"));
        assert!(index.terms.contains_key("quick"));
    }

    #[test]
    fn test_case_insensitive() {
        let docs = vec![json!("Hello World"), json!("HELLO THERE")];

        let index = Bm25Index::build(&docs, IndexOptions::default());
        let results = index.search("hello", 10);

        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_empty_index_search() {
        let index = Bm25Index::new(IndexOptions::default());
        let results = index.search("anything", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_empty_query_search() {
        let docs = vec![json!("hello world"), json!("goodbye world")];
        let index = Bm25Index::build(&docs, IndexOptions::default());
        let results = index.search("", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_single_document_index() {
        let docs = vec![json!("the rust programming language")];
        let index = Bm25Index::build(&docs, IndexOptions::default());

        assert_eq!(index.doc_count, 1);

        let results = index.search("rust", 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "0");
        assert!(results[0].score > 0.0);
    }

    #[test]
    fn test_stem_simple_plural_s() {
        assert_eq!(stem_simple("databases"), "database");
    }

    #[test]
    fn test_stem_simple_plural_ies() {
        assert_eq!(stem_simple("queries"), "query");
    }

    #[test]
    fn test_stem_simple_plural_xes() {
        assert_eq!(stem_simple("boxes"), "box");
    }

    #[test]
    fn test_stem_simple_short_word() {
        assert_eq!(stem_simple("is"), "is");
    }

    #[test]
    fn test_stem_simple_no_change() {
        assert_eq!(stem_simple("data"), "data");
    }

    #[test]
    fn test_idf_zero_for_unknown_term() {
        let docs = vec![json!("hello world"), json!("goodbye world")];
        let index = Bm25Index::build(&docs, IndexOptions::default());
        let idf = index.idf("nonexistent_term");
        assert_eq!(idf, 0.0);
    }
}
