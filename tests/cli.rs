//! CLI integration tests using assert_cmd.
//!
//! All tests use `--repo test-repo` to point at the in-repo fixture.
//! Tests that write to disk use tempfile for isolation and override `$HOME`
//! so config/manifest paths don't touch the real filesystem.

use assert_cmd::Command;
use predicates::prelude::*;
use skillet_mcp::testutil::TestRepo;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

static TEST_REPO: LazyLock<TestRepo> = LazyLock::new(TestRepo::standard);
static TEST_NPM_REPO: LazyLock<TestRepo> = LazyLock::new(TestRepo::npm_style);

fn test_repo() -> &'static Path {
    TEST_REPO.path()
}

fn test_npm_repo() -> &'static Path {
    TEST_NPM_REPO.path()
}

fn official_repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("skills")
}

#[allow(deprecated)] // cargo_bin_cmd! macro has compile-time issues; cargo_bin works fine
fn skillet() -> Command {
    Command::cargo_bin("skillet").expect("binary exists")
}

// ── Search and discovery ─────────────────────────────────────────────

#[test]
fn search_by_keyword() {
    skillet()
        .args(["search", "rust", "--repo"])
        .arg(test_repo())
        .assert()
        .success()
        .stdout(predicate::str::contains("rust-dev"));
}

#[test]
fn search_wildcard_lists_all() {
    skillet()
        .args(["search", "*", "--repo"])
        .arg(test_repo())
        .assert()
        .success()
        .stdout(predicate::str::contains("Found"));
}

#[test]
fn search_owner_filter() {
    skillet()
        .args(["search", "*", "--owner", "joshrotenberg", "--repo"])
        .arg(test_repo())
        .assert()
        .success()
        .stdout(
            predicate::str::contains("joshrotenberg/rust-dev")
                .and(predicate::str::contains("acme/").not()),
        );
}

#[test]
fn search_category_filter() {
    skillet()
        .args(["search", "*", "--category", "security", "--repo"])
        .arg(test_repo())
        .assert()
        .success()
        .stdout(
            predicate::str::contains("security-audit")
                .and(predicate::str::contains("python-dev").not()),
        );
}

#[test]
fn search_tag_filter() {
    skillet()
        .args(["search", "*", "--tag", "pytest", "--repo"])
        .arg(test_repo())
        .assert()
        .success()
        .stdout(predicate::str::contains("python-dev"));
}

#[test]
fn search_no_results() {
    skillet()
        .args(["search", "nonexistent_xyzzy_skill", "--repo"])
        .arg(test_repo())
        .assert()
        .success()
        .stdout(predicate::str::contains("No skills found"));
}

#[test]
fn categories_lists_with_counts() {
    skillet()
        .args(["categories", "--repo"])
        .arg(test_repo())
        .assert()
        .success()
        .stdout(predicate::str::contains("development").and(predicate::str::contains("categor")));
}

// ── Info ─────────────────────────────────────────────────────────────

#[test]
fn info_shows_skill_details() {
    skillet()
        .args(["info", "joshrotenberg/rust-dev", "--repo"])
        .arg(test_repo())
        .assert()
        .success()
        .stdout(
            predicate::str::contains("joshrotenberg/rust-dev")
                .and(predicate::str::contains("version"))
                .and(predicate::str::contains("description"))
                .and(predicate::str::contains("Rust")),
        );
}

#[test]
fn info_not_found() {
    skillet()
        .args(["info", "nonexistent/skill", "--repo"])
        .arg(test_repo())
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

// ── Install and list ─────────────────────────────────────────────────

#[test]
fn install_writes_files() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    skillet()
        .args(["install", "joshrotenberg/rust-dev", "--repo"])
        .arg(test_repo())
        .args(["--target", "agents"])
        .env("HOME", &home)
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Installed joshrotenberg/rust-dev"));

    // Verify the SKILL.md was written
    let skill_md = tmp.path().join(".agents/skills/rust-dev/SKILL.md");
    assert!(
        skill_md.exists(),
        "SKILL.md should be written at {}",
        skill_md.display()
    );
}

#[test]
fn list_shows_installed_skill() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    // Install first
    skillet()
        .args(["install", "joshrotenberg/rust-dev", "--repo"])
        .arg(test_repo())
        .args(["--target", "agents"])
        .env("HOME", &home)
        .current_dir(tmp.path())
        .assert()
        .success();

    // List should show it
    skillet()
        .args(["list"])
        .env("HOME", &home)
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(
            predicate::str::contains("joshrotenberg/rust-dev")
                .and(predicate::str::contains("installed")),
        );
}

#[test]
fn list_empty_when_nothing_installed() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    skillet()
        .args(["list"])
        .env("HOME", &home)
        .assert()
        .success()
        .stdout(predicate::str::contains("No skills installed"));
}

#[test]
fn install_not_found() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    skillet()
        .args(["install", "nonexistent/skill", "--repo"])
        .arg(test_repo())
        .env("HOME", &home)
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

// ── Uninstall ────────────────────────────────────────────────────────

#[test]
fn uninstall_removes_installed_skill() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    // Install first
    skillet()
        .args(["install", "joshrotenberg/rust-dev", "--repo"])
        .arg(test_repo())
        .args(["--target", "agents"])
        .env("HOME", &home)
        .current_dir(tmp.path())
        .assert()
        .success();

    let skill_md = tmp.path().join(".agents/skills/rust-dev/SKILL.md");
    assert!(skill_md.exists(), "SKILL.md should exist after install");

    // Uninstall
    skillet()
        .args(["uninstall", "joshrotenberg/rust-dev"])
        .env("HOME", &home)
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Uninstalled joshrotenberg/rust-dev",
        ));

    // Files should be gone
    assert!(!skill_md.exists(), "SKILL.md should be removed");

    // List should be empty
    skillet()
        .args(["list"])
        .env("HOME", &home)
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("No skills installed"));
}

#[test]
fn uninstall_not_installed() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    skillet()
        .args(["uninstall", "nobody/nothing"])
        .env("HOME", &home)
        .assert()
        .failure()
        .stderr(predicate::str::contains("not installed"));
}

// ── Authoring: validate ──────────────────────────────────────────────

#[test]
fn validate_clean_skill() {
    skillet()
        .args(["validate"])
        .arg(test_repo().join("joshrotenberg/rust-dev"))
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Validation passed")
                .and(predicate::str::contains("skill.toml"))
                .and(predicate::str::contains("SKILL.md")),
        );
}

#[test]
fn validate_unsafe_skill_exits_2() {
    skillet()
        .args(["validate"])
        .arg(test_repo().join("acme/unsafe-demo"))
        .assert()
        .code(2)
        .stdout(predicate::str::contains("Safety scan"))
        .stderr(predicate::str::contains("safety issues detected"));
}

#[test]
fn validate_unsafe_skill_skip_safety() {
    skillet()
        .args(["validate", "--skip-safety"])
        .arg(test_repo().join("acme/unsafe-demo"))
        .assert()
        .success()
        .stdout(predicate::str::contains("Validation passed"));
}

#[test]
fn validate_nonexistent_path() {
    skillet()
        .args(["validate", "/tmp/nonexistent-skill-path-xyzzy"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error").or(predicate::str::contains("failed")));
}

// ── Setup ───────────────────────────────────────────────────────────

#[test]
fn setup_creates_config() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    skillet()
        .args(["setup"])
        .env("HOME", &home)
        .assert()
        .success()
        .stdout(
            predicate::str::contains("config.toml")
                .and(predicate::str::contains("mcpServers"))
                .and(predicate::str::contains("[install]")),
        );

    let config_path = home.join(".config/skillet/config.toml");
    assert!(
        config_path.exists(),
        "config.toml should be written at {}",
        config_path.display()
    );
}

#[test]
fn setup_refuses_overwrite() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    // First run succeeds
    skillet()
        .args(["setup"])
        .env("HOME", &home)
        .assert()
        .success();

    // Second run without --force fails
    skillet()
        .args(["setup"])
        .env("HOME", &home)
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("already exists").and(predicate::str::contains("--force")),
        );
}

#[test]
fn setup_force_overwrites() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    // First run
    skillet()
        .args(["setup"])
        .env("HOME", &home)
        .assert()
        .success();

    // Second run with --force succeeds
    skillet()
        .args(["setup", "--force"])
        .env("HOME", &home)
        .assert()
        .success();
}

#[test]
fn setup_custom_target() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    skillet()
        .args(["setup", "--target", "claude"])
        .env("HOME", &home)
        .assert()
        .success()
        .stdout(predicate::str::contains("claude"));

    let content =
        std::fs::read_to_string(home.join(".config/skillet/config.toml")).expect("read config");
    assert!(
        content.contains("claude"),
        "config should contain 'claude': {content}"
    );
}

#[test]
fn setup_custom_remote() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    skillet()
        .args(["setup", "--remote", "https://example.com/repo.git"])
        .env("HOME", &home)
        .assert()
        .success()
        .stdout(predicate::str::contains("https://example.com/repo.git"));

    let content =
        std::fs::read_to_string(home.join(".config/skillet/config.toml")).expect("read config");
    assert!(
        content.contains("https://example.com/repo.git"),
        "config should contain custom remote: {content}"
    );
}

#[test]
fn setup_no_official_repo() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    skillet()
        .args(["setup", "--no-official-repo"])
        .env("HOME", &home)
        .assert()
        .success();

    let content =
        std::fs::read_to_string(home.join(".config/skillet/config.toml")).expect("read config");
    assert!(
        !content.contains("joshrotenberg/skillet.git"),
        "config should NOT contain official repo URL: {content}"
    );
}

// ── Repo management ─────────────────────────────────────────

#[test]
fn repo_add_and_list() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    // Add a remote repo
    skillet()
        .args(["repo", "add", "https://github.com/example/skills.git"])
        .env("HOME", &home)
        .assert()
        .success()
        .stdout(predicate::str::contains("Added repo"));

    // List should show it
    skillet()
        .args(["repo", "list"])
        .env("HOME", &home)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "https://github.com/example/skills.git",
        ));

    // Add a local repo
    skillet()
        .args(["repo", "add", "/tmp/local-repo"])
        .env("HOME", &home)
        .assert()
        .success()
        .stdout(predicate::str::contains("Added repo"));

    // List should show both
    skillet()
        .args(["repo", "list"])
        .env("HOME", &home)
        .assert()
        .success()
        .stdout(
            predicate::str::contains("https://github.com/example/skills.git")
                .and(predicate::str::contains("/tmp/local-repo")),
        );
}

#[test]
fn repo_add_duplicate() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    skillet()
        .args(["repo", "add", "https://github.com/example/skills.git"])
        .env("HOME", &home)
        .assert()
        .success();

    skillet()
        .args(["repo", "add", "https://github.com/example/skills.git"])
        .env("HOME", &home)
        .assert()
        .failure()
        .stderr(predicate::str::contains("already configured"));
}

#[test]
fn repo_remove() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    skillet()
        .args(["repo", "add", "https://github.com/example/skills.git"])
        .env("HOME", &home)
        .assert()
        .success();

    skillet()
        .args(["repo", "remove", "https://github.com/example/skills.git"])
        .env("HOME", &home)
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed repo"));

    // List should be empty now
    skillet()
        .args(["repo", "list"])
        .env("HOME", &home)
        .assert()
        .success()
        .stdout(predicate::str::contains("No repos configured"));
}

#[test]
fn repo_remove_not_found() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    skillet()
        .args(["repo", "remove", "https://github.com/nobody/nothing.git"])
        .env("HOME", &home)
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn repo_list_empty() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    skillet()
        .args(["repo", "list"])
        .env("HOME", &home)
        .assert()
        .success()
        .stdout(predicate::str::contains("No repos configured"));
}

// ── List scoping (#164) ──────────────────────────────────────

#[test]
fn list_scoped_hides_other_projects() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    let project_a = tmp.path().join("project-a");
    let project_b = tmp.path().join("project-b");
    std::fs::create_dir_all(&home).expect("create home");
    std::fs::create_dir_all(&project_a).expect("create project-a");
    std::fs::create_dir_all(&project_b).expect("create project-b");

    // Install a skill from project-a
    skillet()
        .args(["install", "joshrotenberg/rust-dev", "--repo"])
        .arg(test_repo())
        .args(["--target", "agents"])
        .env("HOME", &home)
        .current_dir(&project_a)
        .assert()
        .success();

    // Install a different skill from project-b
    skillet()
        .args(["install", "acme/python-dev", "--repo"])
        .arg(test_repo())
        .args(["--target", "agents"])
        .env("HOME", &home)
        .current_dir(&project_b)
        .assert()
        .success();

    // List from project-a should only show project-a's skill
    skillet()
        .args(["list"])
        .env("HOME", &home)
        .current_dir(&project_a)
        .assert()
        .success()
        .stdout(
            predicate::str::contains("joshrotenberg/rust-dev")
                .and(predicate::str::contains("acme/python-dev").not()),
        );

    // List --all from project-a should show both
    skillet()
        .args(["list", "--all"])
        .env("HOME", &home)
        .current_dir(&project_a)
        .assert()
        .success()
        .stdout(
            predicate::str::contains("joshrotenberg/rust-dev")
                .and(predicate::str::contains("acme/python-dev")),
        );
}

// ── npm-style repo tests ─────────────────────────────────────

#[test]
fn search_npm_repo() {
    skillet()
        .args(["search", "*", "--repo"])
        .arg(test_npm_repo())
        .assert()
        .success()
        .stdout(
            predicate::str::contains("redis-caching")
                .and(predicate::str::contains("vector-search"))
                .and(predicate::str::contains("session-management")),
        );
}

#[test]
fn search_npm_repo_by_keyword() {
    skillet()
        .args(["search", "caching", "--repo"])
        .arg(test_npm_repo())
        .assert()
        .success()
        .stdout(predicate::str::contains("redis-caching"));
}

#[test]
fn info_npm_repo_with_frontmatter() {
    skillet()
        .args(["info", "redis/redis-caching", "--repo"])
        .arg(test_npm_repo())
        .assert()
        .success()
        .stdout(
            predicate::str::contains("redis/redis-caching")
                .and(predicate::str::contains("2.1.0"))
                .and(predicate::str::contains("caching")),
        );
}

#[test]
fn info_npm_repo_no_frontmatter() {
    skillet()
        .args(["info", "redis/session-management", "--repo"])
        .arg(test_npm_repo())
        .assert()
        .success()
        .stdout(
            predicate::str::contains("redis/session-management")
                .and(predicate::str::contains("0.1.0")),
        );
}

// ── CLI hygiene (#141) ──────────────────────────────────────────────

#[test]
fn version_flag() {
    skillet()
        .args(["--version"])
        .assert()
        .success()
        .stdout(predicate::str::contains("skillet"));
}

#[test]
fn help_flag() {
    skillet().args(["--help"]).assert().success().stdout(
        predicate::str::contains("MCP-native skill registry")
            .and(predicate::str::contains("Commands:"))
            .and(predicate::str::contains("search"))
            .and(predicate::str::contains("install")),
    );
}

#[test]
fn help_subcommand_search() {
    skillet()
        .args(["search", "--help"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Search for skills").and(predicate::str::contains("QUERY")),
        );
}

#[test]
fn help_subcommand_install() {
    skillet()
        .args(["install", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Install a skill").and(predicate::str::contains("SKILL")));
}

#[test]
fn help_subcommand_validate() {
    skillet()
        .args(["validate", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Validate a skillpack"));
}

#[test]
fn help_subcommand_trust() {
    skillet()
        .args(["trust", "--help"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("pin")
                .and(predicate::str::contains("list"))
                .and(predicate::str::contains("unpin")),
        );
}

#[test]
fn search_missing_query() {
    skillet()
        .args(["search", "--repo"])
        .arg(test_repo())
        .assert()
        .failure()
        .stderr(predicate::str::contains("QUERY").or(predicate::str::contains("required")));
}

#[test]
fn install_missing_skill_arg() {
    skillet()
        .args(["install", "--repo"])
        .arg(test_repo())
        .assert()
        .failure()
        .stderr(predicate::str::contains("SKILL").or(predicate::str::contains("required")));
}

#[test]
fn info_missing_skill_arg() {
    skillet()
        .args(["info", "--repo"])
        .arg(test_repo())
        .assert()
        .failure()
        .stderr(predicate::str::contains("SKILL").or(predicate::str::contains("required")));
}

#[test]
fn invalid_subcommand() {
    skillet()
        .args(["nonexistent-subcommand"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unrecognized").or(predicate::str::contains("invalid")));
}

// ── Audit / Trust CLI (#140) ────────────────────────────────────────

#[test]
fn audit_no_installed_skills() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    skillet()
        .args(["audit"])
        .env("HOME", &home)
        .assert()
        .success()
        .stdout(predicate::str::contains("No installed skills"));
}

#[test]
fn audit_after_install() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    // Install a skill
    skillet()
        .args(["install", "joshrotenberg/rust-dev", "--repo"])
        .arg(test_repo())
        .args(["--target", "agents"])
        .env("HOME", &home)
        .current_dir(tmp.path())
        .assert()
        .success();

    // Audit should show the installed skill
    skillet()
        .args(["audit"])
        .env("HOME", &home)
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("joshrotenberg/rust-dev"));
}

#[test]
fn audit_detects_tampered_skill() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    // Install a skill
    skillet()
        .args(["install", "joshrotenberg/rust-dev", "--repo"])
        .arg(test_repo())
        .args(["--target", "agents"])
        .env("HOME", &home)
        .current_dir(tmp.path())
        .assert()
        .success();

    // Tamper with the installed SKILL.md
    let skill_md = tmp.path().join(".agents/skills/rust-dev/SKILL.md");
    std::fs::write(&skill_md, "# TAMPERED CONTENT").expect("tamper with skill");

    // Audit should detect the modification
    skillet()
        .args(["audit"])
        .env("HOME", &home)
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stdout(predicate::str::contains("MODIFIED"));
}

#[test]
fn trust_list_empty() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    skillet()
        .args(["trust", "list"])
        .env("HOME", &home)
        .assert()
        .success()
        .stdout(predicate::str::contains("No pinned skills"));
}

#[test]
fn trust_pin_skill() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    skillet()
        .args(["trust", "pin", "joshrotenberg/rust-dev", "--repo"])
        .arg(test_repo())
        .env("HOME", &home)
        .assert()
        .success()
        .stdout(predicate::str::contains("Pinned"));

    // List should show the pinned skill
    skillet()
        .args(["trust", "list"])
        .env("HOME", &home)
        .assert()
        .success()
        .stdout(predicate::str::contains("joshrotenberg/rust-dev"));
}

#[test]
fn trust_pin_nonexistent_skill() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    skillet()
        .args(["trust", "pin", "nonexistent/skill-xyzzy", "--repo"])
        .arg(test_repo())
        .env("HOME", &home)
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn trust_unpin_skill() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    // Pin first
    skillet()
        .args(["trust", "pin", "joshrotenberg/rust-dev", "--repo"])
        .arg(test_repo())
        .env("HOME", &home)
        .assert()
        .success();

    // Unpin
    skillet()
        .args(["trust", "unpin", "joshrotenberg/rust-dev"])
        .env("HOME", &home)
        .assert()
        .success()
        .stdout(predicate::str::contains("Unpinned"));
}

#[test]
fn trust_unpin_not_pinned() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    skillet()
        .args(["trust", "unpin", "nonexistent/skill"])
        .env("HOME", &home)
        .assert()
        .failure()
        .stderr(predicate::str::contains("not pinned"));
}

// ── Official repo (in-repo) ──────────────────────────────────

#[test]
fn search_official_repo() {
    skillet()
        .args(["search", "*", "--repo"])
        .arg(official_repo())
        .assert()
        .success()
        .stdout(
            predicate::str::contains("skillet/user")
                .and(predicate::str::contains("skillet/skill-author"))
                .and(predicate::str::contains("skillet/setup")),
        );
}

#[test]
fn info_official_skill() {
    skillet()
        .args(["info", "skillet/user", "--repo"])
        .arg(official_repo())
        .assert()
        .success()
        .stdout(
            predicate::str::contains("skillet/user")
                .and(predicate::str::contains("version"))
                .and(predicate::str::contains("description"))
                .and(predicate::str::contains("consumer")),
        );
}

// ── Install trust warning ────────────────────────────────────────────

#[test]
fn install_trust_warning_is_soft() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    skillet()
        .args(["install", "joshrotenberg/rust-dev", "--repo"])
        .arg(test_repo())
        .args(["--target", "agents"])
        .env("HOME", &home)
        .current_dir(tmp.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("Tip:").and(predicate::str::contains("Warning:").not()))
        .stdout(predicate::str::contains("Installed joshrotenberg/rust-dev"));
}
