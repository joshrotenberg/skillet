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

// ── Authoring: init-skill ────────────────────────────────────────────

#[test]
fn init_skill_creates_files() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let skill_path = tmp.path().join("test-owner/test-skill");

    skillet()
        .args(["init-skill"])
        .arg(&skill_path)
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Created skillpack")
                .and(predicate::str::contains("test-owner"))
                .and(predicate::str::contains("test-skill")),
        );

    assert!(
        skill_path.join("skill.toml").exists(),
        "skill.toml should exist"
    );
    assert!(
        skill_path.join("SKILL.md").exists(),
        "SKILL.md should exist"
    );
}

#[test]
fn init_skill_with_options() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let skill_path = tmp.path().join("myowner/my-skill");

    skillet()
        .args(["init-skill"])
        .arg(&skill_path)
        .args([
            "--description",
            "A great skill",
            "--category",
            "development",
            "--tags",
            "rust,testing",
        ])
        .assert()
        .success();

    let toml_content =
        std::fs::read_to_string(skill_path.join("skill.toml")).expect("read skill.toml");
    assert!(
        toml_content.contains("A great skill"),
        "should contain description: {toml_content}"
    );
    assert!(
        toml_content.contains("development"),
        "should contain category: {toml_content}"
    );
    assert!(
        toml_content.contains("rust"),
        "should contain tag: {toml_content}"
    );
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

// ── Authoring: pack ──────────────────────────────────────────────────

#[test]
fn pack_creates_manifest_and_versions() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let skill_path = tmp.path().join("myowner/my-skill");

    // Scaffold a skill first
    skillet()
        .args(["init-skill"])
        .arg(&skill_path)
        .assert()
        .success();

    // Pack it
    skillet()
        .args(["pack"])
        .arg(&skill_path)
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Pack succeeded")
                .and(predicate::str::contains("MANIFEST.sha256"))
                .and(predicate::str::contains("versions.toml")),
        );

    assert!(
        skill_path.join("MANIFEST.sha256").exists(),
        "MANIFEST.sha256 should be created"
    );
    assert!(
        skill_path.join("versions.toml").exists(),
        "versions.toml should be created"
    );
}

#[test]
fn pack_unsafe_skill_exits_2() {
    // Copy unsafe-demo to a temp dir so pack doesn't modify the fixture
    let tmp = tempfile::tempdir().expect("create temp dir");
    let src = test_registry().join("acme/unsafe-demo");
    let dst = tmp.path().join("acme/unsafe-demo");
    std::fs::create_dir_all(&dst).expect("create dirs");
    for entry in std::fs::read_dir(&src).expect("read src") {
        let entry = entry.expect("dir entry");
        if entry.file_type().expect("file type").is_file() {
            std::fs::copy(entry.path(), dst.join(entry.file_name())).expect("copy");
        }
    }

    skillet()
        .args(["pack"])
        .arg(&dst)
        .assert()
        .code(2)
        .stdout(predicate::str::contains("Safety scan"))
        .stderr(predicate::str::contains("safety issues detected"));
}

// ── Authoring: init-registry ─────────────────────────────────────────

#[test]
fn init_registry_creates_files() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let registry_path = tmp.path().join("my-registry");

    skillet()
        .args(["init-registry"])
        .arg(&registry_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Initialized skill registry"));

    assert!(
        registry_path.join("skillet.toml").exists(),
        "skillet.toml should be created"
    );
    assert!(
        registry_path.join(".gitignore").exists(),
        ".gitignore should be created"
    );
}

#[test]
fn init_registry_with_options() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let registry_path = tmp.path().join("named-registry");

    skillet()
        .args(["init-registry"])
        .arg(&registry_path)
        .args(["--name", "My Registry", "--description", "A test registry"])
        .assert()
        .success();

    let config =
        std::fs::read_to_string(registry_path.join("skillet.toml")).expect("read skillet.toml");
    assert!(
        config.contains("My Registry"),
        "should contain registry name: {config}"
    );
    assert!(
        config.contains("A test registry"),
        "should contain description: {config}"
    );
}

#[test]
fn init_registry_fails_on_existing_dir() {
    let tmp = tempfile::tempdir().expect("create temp dir");

    // The temp dir itself already exists
    skillet()
        .args(["init-registry"])
        .arg(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
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
        !content.contains("skillet-registry"),
        "config should NOT contain official registry URL: {content}"
    );
}
