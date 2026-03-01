//! CLI integration tests using assert_cmd.
//!
//! All tests use `--registry test-registry` to point at the in-repo fixture.
//! Tests that write to disk use tempfile for isolation and override `$HOME`
//! so config/manifest paths don't touch the real filesystem.

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::PathBuf;

fn test_registry() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-registry")
}

fn test_npm_registry() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-npm-registry")
}

fn official_registry() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("registry")
}

#[allow(deprecated)] // cargo_bin_cmd! macro has compile-time issues; cargo_bin works fine
fn skillet() -> Command {
    Command::cargo_bin("skillet").expect("binary exists")
}

// ── Search and discovery ─────────────────────────────────────────────

#[test]
fn search_by_keyword() {
    skillet()
        .args(["search", "rust", "--registry"])
        .arg(test_registry())
        .assert()
        .success()
        .stdout(predicate::str::contains("rust-dev"));
}

#[test]
fn search_wildcard_lists_all() {
    skillet()
        .args(["search", "*", "--registry"])
        .arg(test_registry())
        .assert()
        .success()
        .stdout(predicate::str::contains("Found"));
}

#[test]
fn search_owner_filter() {
    skillet()
        .args(["search", "*", "--owner", "joshrotenberg", "--registry"])
        .arg(test_registry())
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
        .args(["search", "*", "--category", "security", "--registry"])
        .arg(test_registry())
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
        .args(["search", "*", "--tag", "pytest", "--registry"])
        .arg(test_registry())
        .assert()
        .success()
        .stdout(predicate::str::contains("python-dev"));
}

#[test]
fn search_no_results() {
    skillet()
        .args(["search", "nonexistent_xyzzy_skill", "--registry"])
        .arg(test_registry())
        .assert()
        .success()
        .stdout(predicate::str::contains("No skills found"));
}

#[test]
fn categories_lists_with_counts() {
    skillet()
        .args(["categories", "--registry"])
        .arg(test_registry())
        .assert()
        .success()
        .stdout(predicate::str::contains("development").and(predicate::str::contains("categor")));
}

// ── Info ─────────────────────────────────────────────────────────────

#[test]
fn info_shows_skill_details() {
    skillet()
        .args(["info", "joshrotenberg/rust-dev", "--registry"])
        .arg(test_registry())
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
        .args(["info", "nonexistent/skill", "--registry"])
        .arg(test_registry())
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
        .args(["install", "joshrotenberg/rust-dev", "--registry"])
        .arg(test_registry())
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
        .args(["install", "joshrotenberg/rust-dev", "--registry"])
        .arg(test_registry())
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
        .args(["install", "nonexistent/skill", "--registry"])
        .arg(test_registry())
        .env("HOME", &home)
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

// ── Authoring: validate ──────────────────────────────────────────────

#[test]
fn validate_clean_skill() {
    skillet()
        .args(["validate"])
        .arg(test_registry().join("joshrotenberg/rust-dev"))
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
        .arg(test_registry().join("acme/unsafe-demo"))
        .assert()
        .code(2)
        .stdout(predicate::str::contains("Safety scan"))
        .stderr(predicate::str::contains("safety issues detected"));
}

#[test]
fn validate_unsafe_skill_skip_safety() {
    skillet()
        .args(["validate", "--skip-safety"])
        .arg(test_registry().join("acme/unsafe-demo"))
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
fn setup_no_official_registry() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    skillet()
        .args(["setup", "--no-official-registry"])
        .env("HOME", &home)
        .assert()
        .success();

    let content =
        std::fs::read_to_string(home.join(".config/skillet/config.toml")).expect("read config");
    assert!(
        !content.contains("joshrotenberg/skillet.git"),
        "config should NOT contain official registry URL: {content}"
    );
}

// ── npm-style registry tests ─────────────────────────────────────

#[test]
fn search_npm_registry() {
    skillet()
        .args(["search", "*", "--registry"])
        .arg(test_npm_registry())
        .assert()
        .success()
        .stdout(
            predicate::str::contains("redis-caching")
                .and(predicate::str::contains("vector-search"))
                .and(predicate::str::contains("session-management")),
        );
}

#[test]
fn search_npm_registry_by_keyword() {
    skillet()
        .args(["search", "caching", "--registry"])
        .arg(test_npm_registry())
        .assert()
        .success()
        .stdout(predicate::str::contains("redis-caching"));
}

#[test]
fn info_npm_registry_with_frontmatter() {
    skillet()
        .args(["info", "redis/redis-caching", "--registry"])
        .arg(test_npm_registry())
        .assert()
        .success()
        .stdout(
            predicate::str::contains("redis/redis-caching")
                .and(predicate::str::contains("2.1.0"))
                .and(predicate::str::contains("caching")),
        );
}

#[test]
fn info_npm_registry_no_frontmatter() {
    skillet()
        .args(["info", "redis/session-management", "--registry"])
        .arg(test_npm_registry())
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
            predicate::str::contains("add-registry")
                .and(predicate::str::contains("pin"))
                .and(predicate::str::contains("list")),
        );
}

#[test]
fn search_missing_query() {
    skillet()
        .args(["search", "--registry"])
        .arg(test_registry())
        .assert()
        .failure()
        .stderr(predicate::str::contains("QUERY").or(predicate::str::contains("required")));
}

#[test]
fn install_missing_skill_arg() {
    skillet()
        .args(["install", "--registry"])
        .arg(test_registry())
        .assert()
        .failure()
        .stderr(predicate::str::contains("SKILL").or(predicate::str::contains("required")));
}

#[test]
fn info_missing_skill_arg() {
    skillet()
        .args(["info", "--registry"])
        .arg(test_registry())
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
        .args(["install", "joshrotenberg/rust-dev", "--registry"])
        .arg(test_registry())
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
        .args(["install", "joshrotenberg/rust-dev", "--registry"])
        .arg(test_registry())
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
fn trust_add_and_list_registry() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    // Add a trusted registry
    skillet()
        .args(["trust", "add-registry", "https://example.com/registry.git"])
        .env("HOME", &home)
        .assert()
        .success()
        .stdout(predicate::str::contains("Trusted"));

    // List should show it
    skillet()
        .args(["trust", "list"])
        .env("HOME", &home)
        .assert()
        .success()
        .stdout(predicate::str::contains("https://example.com/registry.git"));
}

#[test]
fn trust_add_registry_with_note() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    skillet()
        .args([
            "trust",
            "add-registry",
            "https://example.com/repo.git",
            "--note",
            "Official registry",
        ])
        .env("HOME", &home)
        .assert()
        .success()
        .stdout(predicate::str::contains("Trusted"));

    // List should show the note
    skillet()
        .args(["trust", "list"])
        .env("HOME", &home)
        .assert()
        .success()
        .stdout(
            predicate::str::contains("https://example.com/repo.git")
                .and(predicate::str::contains("Official registry")),
        );
}

#[test]
fn trust_add_registry_idempotent() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    // Add twice
    skillet()
        .args(["trust", "add-registry", "https://example.com/repo.git"])
        .env("HOME", &home)
        .assert()
        .success();

    skillet()
        .args(["trust", "add-registry", "https://example.com/repo.git"])
        .env("HOME", &home)
        .assert()
        .success()
        .stdout(predicate::str::contains("already trusted"));
}

#[test]
fn trust_remove_registry() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    // Add then remove
    skillet()
        .args(["trust", "add-registry", "https://example.com/repo.git"])
        .env("HOME", &home)
        .assert()
        .success();

    skillet()
        .args(["trust", "remove-registry", "https://example.com/repo.git"])
        .env("HOME", &home)
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed"));

    // List should be empty
    skillet()
        .args(["trust", "list"])
        .env("HOME", &home)
        .assert()
        .success()
        .stdout(predicate::str::contains("No trusted registries"));
}

#[test]
fn trust_remove_nonexistent_registry() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    skillet()
        .args([
            "trust",
            "remove-registry",
            "https://example.com/nonexistent.git",
        ])
        .env("HOME", &home)
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
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
        .stdout(predicate::str::contains("No trusted registries"));
}

#[test]
fn trust_pin_skill() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    skillet()
        .args(["trust", "pin", "joshrotenberg/rust-dev", "--registry"])
        .arg(test_registry())
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
        .args(["trust", "pin", "nonexistent/skill-xyzzy", "--registry"])
        .arg(test_registry())
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
        .args(["trust", "pin", "joshrotenberg/rust-dev", "--registry"])
        .arg(test_registry())
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

#[test]
fn trust_list_registries_only() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    // Add a registry and pin a skill
    skillet()
        .args(["trust", "add-registry", "https://example.com/repo.git"])
        .env("HOME", &home)
        .assert()
        .success();

    skillet()
        .args(["trust", "pin", "joshrotenberg/rust-dev", "--registry"])
        .arg(test_registry())
        .env("HOME", &home)
        .assert()
        .success();

    // List with --registries-only should show registry but not pinned skill details
    skillet()
        .args(["trust", "list", "--registries-only"])
        .env("HOME", &home)
        .assert()
        .success()
        .stdout(predicate::str::contains("https://example.com/repo.git"));
}

// ── Official registry (in-repo) ──────────────────────────────────

#[test]
fn search_official_registry() {
    skillet()
        .args(["search", "*", "--registry"])
        .arg(official_registry())
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
        .args(["info", "skillet/user", "--registry"])
        .arg(official_registry())
        .assert()
        .success()
        .stdout(
            predicate::str::contains("skillet/user")
                .and(predicate::str::contains("version"))
                .and(predicate::str::contains("description"))
                .and(predicate::str::contains("consumer")),
        );
}
