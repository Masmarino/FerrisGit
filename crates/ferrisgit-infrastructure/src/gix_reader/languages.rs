use std::collections::HashMap;
use std::path::Path;

use super::{GitReadError, GixRepositoryReader, LanguageStat, commit_by_sha, open};

/// File entries `compute_language_stats` visits before giving up with `None`, so the frontend
/// shows no language bar rather than a partial one.
const MAX_FILES_FOR_LANGUAGE_STATS: usize = 2000;

/// Languages listed individually; the rest are folded into "Other".
const TOP_LANGUAGES: usize = 6;

impl GixRepositoryReader {
    pub fn compute_language_stats(
        &self,
        disk_path: &Path,
        revision: &str,
    ) -> Result<Option<Vec<LanguageStat>>, GitReadError> {
        self.compute_language_stats_with_limit(disk_path, revision, MAX_FILES_FOR_LANGUAGE_STATS)
    }

    /// `compute_language_stats` with an explicit file limit, so tests can trip the cutoff cheaply.
    pub fn compute_language_stats_with_limit(
        &self,
        disk_path: &Path,
        revision: &str,
        max_files: usize,
    ) -> Result<Option<Vec<LanguageStat>>, GitReadError> {
        let repo = open(disk_path)?;
        let Some(commit) = commit_by_sha(&repo, revision) else {
            return Ok(None);
        };
        let tree = commit.tree().map_err(GitReadError::other)?;

        let mut bytes_by_language: HashMap<&'static str, u64> = HashMap::new();
        let mut files_visited = 0usize;
        if !walk_tree_for_languages(
            &repo,
            &tree,
            max_files,
            &mut files_visited,
            &mut bytes_by_language,
        )? {
            return Ok(None);
        }

        let total: u64 = bytes_by_language.values().sum();
        if total == 0 {
            return Ok(Some(Vec::new()));
        }

        let percentage = |bytes: u64| (bytes as f32 / total as f32) * 100.0;
        let mut stats: Vec<LanguageStat> = bytes_by_language
            .into_iter()
            .map(|(name, bytes)| LanguageStat {
                name: name.to_string(),
                bytes,
                percentage: percentage(bytes),
            })
            .collect();
        stats.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.name.cmp(&b.name)));

        if stats.len() > TOP_LANGUAGES {
            let other_bytes: u64 = stats[TOP_LANGUAGES..].iter().map(|s| s.bytes).sum();
            stats.truncate(TOP_LANGUAGES);
            stats.push(LanguageStat {
                name: "Other".to_string(),
                bytes: other_bytes,
                percentage: percentage(other_bytes),
            });
        }
        Ok(Some(stats))
    }
}

fn language_for_extension(file_name: &str) -> Option<&'static str> {
    let extension = file_name.rsplit('.').next().unwrap_or("").to_lowercase();
    match extension.as_str() {
        "rs" => Some("Rust"),
        "ts" | "tsx" => Some("TypeScript"),
        "js" | "jsx" => Some("JavaScript"),
        "html" => Some("HTML"),
        "scss" | "css" => Some("CSS"),
        "py" => Some("Python"),
        "go" => Some("Go"),
        "java" => Some("Java"),
        "c" | "h" => Some("C"),
        "cpp" | "hpp" => Some("C++"),
        "rb" => Some("Ruby"),
        "sh" | "bash" => Some("Shell"),
        "sql" => Some("SQL"),
        "md" => Some("Markdown"),
        "json" => Some("JSON"),
        "yml" | "yaml" => Some("YAML"),
        _ => None,
    }
}

/// Recursively sums bytes per language across `tree` using `find_header` (no content decode).
/// Returns `Ok(false)` as soon as `*files_visited` exceeds `max_files`.
fn walk_tree_for_languages(
    repo: &gix::Repository,
    tree: &gix::Tree,
    max_files: usize,
    files_visited: &mut usize,
    bytes_by_language: &mut HashMap<&'static str, u64>,
) -> Result<bool, GitReadError> {
    for entry in tree.iter() {
        let entry = entry.map_err(GitReadError::other)?;
        if entry.mode().is_tree() {
            let Ok(object) = entry.object() else { continue };
            if !walk_tree_for_languages(
                repo,
                &object.into_tree(),
                max_files,
                files_visited,
                bytes_by_language,
            )? {
                return Ok(false);
            }
            continue;
        }
        *files_visited += 1;
        if *files_visited > max_files {
            return Ok(false);
        }
        if let Some(language) = language_for_extension(&entry.filename().to_string()) {
            let header = repo
                .find_header(entry.oid().to_owned())
                .map_err(GitReadError::other)?;
            *bytes_by_language.entry(language).or_insert(0) += header.size();
        }
    }
    Ok(true)
}
