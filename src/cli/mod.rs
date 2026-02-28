pub(crate) mod author;
pub(crate) mod install;
pub(crate) mod search;
pub(crate) mod setup;
pub(crate) mod trust;

use skillet_mcp::safety;

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

/// Print a safety report to stdout/stderr.
pub(crate) fn print_safety_report(report: &safety::SafetyReport) {
    let danger_count = report
        .findings
        .iter()
        .filter(|f| f.severity == safety::Severity::Danger)
        .count();
    let warning_count = report
        .findings
        .iter()
        .filter(|f| f.severity == safety::Severity::Warning)
        .count();

    println!("Safety scan: {danger_count} danger, {warning_count} warning\n");

    for f in &report.findings {
        let line_info = match f.line {
            Some(n) => format!("{}:{n}", f.file),
            None => f.file.clone(),
        };
        println!("  [{severity}] {line_info}", severity = f.severity);
        println!("    rule: {}", f.rule_id);
        println!("    {}", f.message);
        println!("    matched: {}", f.matched);
    }
}
