pub(crate) mod author;
pub(crate) mod repo;
pub(crate) mod search;

/// Parse an "owner/name" skill reference.
pub(crate) fn parse_skill_ref(s: &str) -> Result<(&str, &str), String> {
    let parts: Vec<&str> = s.splitn(2, '/').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err(format!(
            "Invalid skill reference: '{s}'. Expected format: owner/name"
        ));
    }
    Ok((parts[0], parts[1]))
}
