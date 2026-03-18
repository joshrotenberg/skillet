//! Persistent skill annotations.
//!
//! Agents can attach notes to skills that persist across sessions.
//! Stored as JSON in `~/.config/skillet/annotations.json`.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config;

/// A single annotation on a skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Annotation {
    /// The note text.
    pub note: String,
    /// ISO 8601 timestamp.
    pub created_at: String,
}

/// All annotations, keyed by `owner/name`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnnotationStore {
    #[serde(flatten)]
    pub skills: HashMap<String, Vec<Annotation>>,
}

/// Path to the annotations file.
fn annotations_path() -> PathBuf {
    config::config_dir().join("annotations.json")
}

/// Load annotations from disk. Returns empty store if file doesn't exist.
pub fn load() -> AnnotationStore {
    let path = annotations_path();
    if !path.is_file() {
        return AnnotationStore::default();
    }
    match std::fs::read_to_string(&path) {
        Ok(data) => serde_json::from_str(&data).unwrap_or_default(),
        Err(_) => AnnotationStore::default(),
    }
}

/// Save annotations to disk.
pub fn save(store: &AnnotationStore) -> crate::error::Result<()> {
    let path = annotations_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_string_pretty(store)
        .map_err(|e| crate::error::Error::Other(e.to_string()))?;
    std::fs::write(&path, data)?;
    Ok(())
}

/// Add an annotation to a skill.
pub fn annotate(owner: &str, name: &str, note: &str) -> crate::error::Result<usize> {
    let mut store = load();
    let key = format!("{owner}/{name}");
    let entry = store.skills.entry(key).or_default();
    entry.push(Annotation {
        note: note.to_string(),
        created_at: config::now_iso8601(),
    });
    let count = entry.len();
    save(&store)?;
    Ok(count)
}

/// Get annotations for a skill.
pub fn get(owner: &str, name: &str) -> Vec<Annotation> {
    let store = load();
    let key = format!("{owner}/{name}");
    store.skills.get(&key).cloned().unwrap_or_default()
}

/// List all annotated skills with their annotation counts.
pub fn list_all() -> Vec<(String, usize)> {
    let store = load();
    let mut result: Vec<(String, usize)> = store
        .skills
        .iter()
        .map(|(k, v)| (k.clone(), v.len()))
        .collect();
    result.sort_by(|a, b| a.0.cmp(&b.0));
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn annotation_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        // Override HOME for isolated config dir
        let prev = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", tmp.path()) };

        let count = annotate("acme", "rust-dev", "Missing async pool docs").unwrap();
        assert_eq!(count, 1);

        let count = annotate("acme", "rust-dev", "Could use error handling section").unwrap();
        assert_eq!(count, 2);

        let notes = get("acme", "rust-dev");
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].note, "Missing async pool docs");
        assert_eq!(notes[1].note, "Could use error handling section");
        assert!(!notes[0].created_at.is_empty());

        let all = list_all();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0], ("acme/rust-dev".to_string(), 2));

        // Empty skill
        let empty = get("nobody", "nothing");
        assert!(empty.is_empty());

        if let Some(h) = prev {
            unsafe { std::env::set_var("HOME", h) };
        }
    }
}
