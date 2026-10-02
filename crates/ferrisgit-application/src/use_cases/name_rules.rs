/// Usernames, group names and repo names all become a URL path segment. No `.` on purpose: a name ending in `.git`
/// would send the owner's own pages to the git handler.
pub(crate) fn is_valid_path_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}
