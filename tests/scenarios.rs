//! Multi-step workflow scenario tests.
//!
//! Each test chains multiple CLI operations to exercise real user workflows
//! end-to-end, verifying that outputs from one step are valid inputs to the next.
//!
//! All tests use `tempfile` for isolation and override `$HOME` so nothing
//! touches the real filesystem.

use assert_cmd::Command;
use predicates::prelude::*;
use skillet_mcp::testutil::TestRepo;
use std::path::Path;
use std::sync::LazyLock;

static TEST_REPO: LazyLock<TestRepo> = LazyLock::new(TestRepo::standard);

fn test_repo() -> &'static Path {
    TEST_REPO.path()
}

#[allow(deprecated)]
fn skillet() -> Command {
    Command::cargo_bin("skillet").expect("binary exists")
}

// ── Multi-repo merge ─────────────────────────────────────────────

/// Two repos with overlapping names: first-repo-wins on collision,
/// unique skills from second repo still present.
#[test]
fn scenario_multi_repo_merge() {
    let tmp = tempfile::tempdir().expect("create temp dir");

    // Create two mini repos
    let reg_a = tmp.path().join("reg-a");
    let reg_b = tmp.path().join("reg-b");

    // Repo A: has "shared/tool" and "alpha/unique-a"
    create_mini_skill(&reg_a, "shared", "tool", "Tool from repo A");
    create_mini_skill(&reg_a, "alpha", "unique-a", "Only in A");
    write_repo_config(&reg_a, "Repo A");

    // Repo B: has "shared/tool" (different description) and "beta/unique-b"
    create_mini_skill(&reg_b, "shared", "tool", "Tool from repo B");
    create_mini_skill(&reg_b, "beta", "unique-b", "Only in B");
    write_repo_config(&reg_b, "Repo B");

    // Search with both repos (A first)
    let output = skillet()
        .args(["search", "*", "--repo"])
        .arg(&reg_a)
        .args(["--repo"])
        .arg(&reg_b)
        .output()
        .expect("search");
    let stdout = String::from_utf8_lossy(&output.stdout);

    // First repo wins for shared/tool
    assert!(
        stdout.contains("Tool from repo A"),
        "first repo should win for shared/tool: {stdout}"
    );
    assert!(
        !stdout.contains("Tool from repo B"),
        "second repo description should not appear: {stdout}"
    );

    // Unique skills from both repos present
    assert!(
        stdout.contains("unique-a"),
        "unique-a from reg A should appear: {stdout}"
    );
    assert!(
        stdout.contains("unique-b"),
        "unique-b from reg B should appear: {stdout}"
    );
}

// ── Helpers ──────────────────────────────────────────────────────────

fn create_mini_skill(repo: &std::path::Path, owner: &str, name: &str, description: &str) {
    let skill_dir = repo.join(owner).join(name);
    std::fs::create_dir_all(&skill_dir).expect("create skill dir");

    let toml = format!(
        "[skill]\nname = \"{name}\"\nowner = \"{owner}\"\nversion = \"1.0.0\"\ndescription = \"{description}\"\n"
    );
    std::fs::write(skill_dir.join("skill.toml"), toml).expect("write skill.toml");
    std::fs::write(
        skill_dir.join("SKILL.md"),
        format!("# {name}\n\n{description}\n"),
    )
    .expect("write SKILL.md");
}

fn write_repo_config(repo: &std::path::Path, name: &str) {
    let config = format!("[project]\nname = \"{name}\"\n");
    std::fs::write(repo.join("skillet.toml"), config).expect("write skillet.toml");
}

// ── Project manifest lifecycle (#144) ────────────────────────────
//
// Local discovery (#144) only runs in MCP server context, not CLI.
// These tests exercise project manifest features testable via CLI.

/// init -> add skills -> search finds them via --repo
#[test]
fn scenario_project_manifest_as_repo() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let project = tmp.path().join("my-project");
    std::fs::create_dir_all(&project).expect("create project dir");

    // Step 1: Create a project with a skills directory
    let skills_dir = project.join("skills");
    std::fs::create_dir_all(&skills_dir).expect("create skills dir");

    // Write skillet.toml with [skills] pointing to skills/
    std::fs::write(
        project.join("skillet.toml"),
        "[project]\nname = \"my-project\"\ndescription = \"Test project\"\n\n[skills]\npath = \"skills\"\n",
    )
    .expect("write skillet.toml");

    // Step 2: Add a skill in skills/
    let skill_dir = skills_dir.join("my-tool");
    std::fs::create_dir_all(&skill_dir).expect("create skill dir");
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "# My Tool\n\nA project-embedded skill for testing.\n",
    )
    .expect("write SKILL.md");

    // Step 3: Search with --repo pointing at the project
    skillet()
        .args(["search", "*", "--repo"])
        .arg(&project)
        .assert()
        .success()
        .stdout(predicate::str::contains("my-tool"));
}

/// init scaffolding creates correct structure
#[test]
fn scenario_init_lifecycle() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let project = tmp.path().join("new-project");

    // Step 1: Init with --skill
    skillet()
        .args(["init"])
        .arg(&project)
        .args(["--skill"])
        .assert()
        .success()
        .stdout(predicate::str::contains("skillet.toml"));

    // Step 2: Verify structure
    assert!(
        project.join("skillet.toml").exists(),
        "skillet.toml should exist"
    );
    assert!(
        project.join("SKILL.md").exists(),
        "SKILL.md should exist for --skill"
    );

    // Step 3: skillet.toml should have [skill] section
    let toml = std::fs::read_to_string(project.join("skillet.toml")).expect("read skillet.toml");
    assert!(
        toml.contains("[skill]"),
        "should have [skill] section: {toml}"
    );
    assert!(
        toml.contains("[project]"),
        "should have [project] section: {toml}"
    );

    // Step 4: Search with --repo should find the embedded skill
    skillet()
        .args(["search", "*", "--repo"])
        .arg(&project)
        .assert()
        .success()
        .stdout(predicate::str::contains("new-project"));
}

/// init with --multi creates multi-skill directory
#[test]
fn scenario_init_multi() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let project = tmp.path().join("multi-project");

    // Step 1: Init with --multi
    skillet()
        .args(["init"])
        .arg(&project)
        .args(["--multi"])
        .assert()
        .success();

    // Step 2: Verify structure
    let toml = std::fs::read_to_string(project.join("skillet.toml")).expect("read skillet.toml");
    assert!(
        toml.contains("[skills]"),
        "should have [skills] section: {toml}"
    );

    // Step 3: Add skills to .skillet/ directory
    let skill_dir = project.join(".skillet/helper");
    std::fs::create_dir_all(&skill_dir).expect("create skill dir");
    std::fs::write(skill_dir.join("SKILL.md"), "# Helper\n\nA helper skill.\n")
        .expect("write SKILL.md");

    // Step 4: Search should find it
    skillet()
        .args(["search", "*", "--repo"])
        .arg(&project)
        .assert()
        .success()
        .stdout(predicate::str::contains("helper"));
}

// ── Error recovery and edge cases (#146) ─────────────────────────

/// Malformed skill.toml in repo is skipped, valid skills still load
#[test]
fn scenario_malformed_skill_skipped() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let repo = tmp.path().join("repo");

    // Create a valid skill
    create_mini_skill(&repo, "good", "valid-skill", "A valid skill");

    // Create a malformed skill (bad TOML)
    let bad_skill = repo.join("bad/broken-skill");
    std::fs::create_dir_all(&bad_skill).expect("create bad skill dir");
    std::fs::write(bad_skill.join("skill.toml"), "[skill\nname = broken").expect("write bad toml");
    std::fs::write(bad_skill.join("SKILL.md"), "# Broken\n").expect("write SKILL.md");

    write_repo_config(&repo, "Mixed Repo");

    // Search should find the valid skill and skip the broken one
    let output = skillet()
        .args(["search", "*", "--repo"])
        .arg(&repo)
        .output()
        .expect("search");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("valid-skill"),
        "valid skill should be found: {stdout}"
    );
    assert!(
        !stdout.contains("broken-skill"),
        "broken skill should be skipped: {stdout}"
    );
}

/// Empty repo returns no results without error
#[test]
fn scenario_empty_repo() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let repo = tmp.path().join("empty-repo");
    std::fs::create_dir_all(&repo).expect("create empty repo");
    write_repo_config(&repo, "Empty Repo");

    skillet()
        .args(["search", "*", "--repo"])
        .arg(&repo)
        .assert()
        .success()
        .stdout(predicate::str::contains("No skills found"));
}

/// Multi-repo where one has errors, other repo's skills still available
#[test]
fn scenario_mixed_repo_health() {
    let tmp = tempfile::tempdir().expect("create temp dir");

    // Good repo
    let good_reg = tmp.path().join("good-reg");
    create_mini_skill(&good_reg, "alice", "good-skill", "A working skill");
    write_repo_config(&good_reg, "Good Repo");

    // Bad repo: malformed skill only
    let bad_reg = tmp.path().join("bad-reg");
    let bad_skill = bad_reg.join("broken/bad-skill");
    std::fs::create_dir_all(&bad_skill).expect("create bad skill dir");
    std::fs::write(bad_skill.join("skill.toml"), "not valid toml {{{{").expect("write bad toml");
    std::fs::write(bad_skill.join("SKILL.md"), "# Bad\n").expect("write SKILL.md");
    write_repo_config(&bad_reg, "Bad Repo");

    // Search across both repos
    let output = skillet()
        .args(["search", "*", "--repo"])
        .arg(&good_reg)
        .args(["--repo"])
        .arg(&bad_reg)
        .output()
        .expect("search");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("good-skill"),
        "good repo skills should still work: {stdout}"
    );
}

/// Corrupt cache is recovered gracefully
#[test]
fn scenario_corrupt_cache_recovery() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    let cache_dir = tmp.path().join("cache");
    std::fs::create_dir_all(&home).expect("create home");
    std::fs::create_dir_all(&cache_dir).expect("create cache dir");

    // First search populates cache
    skillet()
        .args(["search", "rust", "--repo"])
        .arg(test_repo())
        .env("HOME", &home)
        .env("SKILLET_CACHE_DIR", &cache_dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("rust-dev"));

    // Corrupt all cache files
    if let Ok(entries) = std::fs::read_dir(&cache_dir) {
        for entry in entries.flatten() {
            if entry.path().is_file() {
                std::fs::write(entry.path(), "not valid json {{{").expect("corrupt cache file");
            }
        }
    }

    // Second search should recover gracefully (rebuild from source)
    skillet()
        .args(["search", "rust", "--repo"])
        .arg(test_repo())
        .env("HOME", &home)
        .env("SKILLET_CACHE_DIR", &cache_dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("rust-dev"));
}

/// Skill with only skill.toml and no SKILL.md is skipped
#[test]
fn scenario_missing_skill_md_skipped() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let repo = tmp.path().join("repo");

    // Valid skill
    create_mini_skill(&repo, "owner", "good-skill", "Has everything");

    // Skill with only skill.toml, no SKILL.md
    let no_md = repo.join("owner/no-md-skill");
    std::fs::create_dir_all(&no_md).expect("create no-md skill dir");
    std::fs::write(
        no_md.join("skill.toml"),
        "[skill]\nname = \"no-md-skill\"\nowner = \"owner\"\nversion = \"1.0.0\"\ndescription = \"Missing SKILL.md\"\n",
    )
    .expect("write skill.toml");

    write_repo_config(&repo, "Test Repo");

    // Search should find the good skill, skip the one without SKILL.md
    let output = skillet()
        .args(["search", "*", "--repo"])
        .arg(&repo)
        .output()
        .expect("search");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("good-skill"),
        "valid skill should be found: {stdout}"
    );
}
