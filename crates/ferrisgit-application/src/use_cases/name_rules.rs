/// Usernames, group names and repository names all end up as a URL path segment. `.` is left out on purpose: a segment
/// ending in `.git` (`x.git`) would make the git-vs-SPA fallback send the owner's own pages to the git handler.
pub(crate) fn is_valid_path_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}
