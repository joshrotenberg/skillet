//! CLI integration tests using assert_cmd.
//!
//! All tests use `--repo test-repo` to point at the in-repo fixture.

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

#[allow(deprecated)]
fn skillet() -> Command {
    Command::cargo_bin("skillet").expect("binary exists")
}

// -- Search and discovery --

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

// -- Info --

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

// -- Repo management --

#[test]
fn repo_add_and_list() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    skillet()
        .args(["repo", "add", "https://github.com/example/skills.git"])
        .env("HOME", &home)
        .assert()
        .success()
        .stdout(predicate::str::contains("Added repo"));

    skillet()
        .args(["repo", "list"])
        .env("HOME", &home)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "https://github.com/example/skills.git",
        ));

    skillet()
        .args(["repo", "add", "/tmp/local-repo"])
        .env("HOME", &home)
        .assert()
        .success()
        .stdout(predicate::str::contains("Added repo"));

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

// -- npm-style repo tests --

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

// -- Implicit serve behavior --

#[test]
fn repo_flag_triggers_serve_with_http() {
    // --repo with --http should start the server (not show help).
    // We test by starting the server and hitting health.
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };

    let mut child = std::process::Command::new(assert_cmd::cargo::cargo_bin!("skillet"))
        .args([
            "--repo",
            test_repo().to_str().unwrap(),
            "--http",
            &format!("127.0.0.1:{port}"),
            "--log-level",
            "error",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn skillet");

    // Poll until server is up or timeout
    let client = reqwest::blocking::Client::new();
    let mut ok = false;
    for _ in 0..50 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if client
            .get(format!("http://127.0.0.1:{port}/health"))
            .send()
            .is_ok()
        {
            ok = true;
            break;
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        ok,
        "server should start with --repo flag (no explicit serve subcommand)"
    );
}

// -- CLI hygiene --

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
        predicate::str::contains("MCP-native skill discovery")
            .and(predicate::str::contains("Commands:"))
            .and(predicate::str::contains("search")),
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
fn search_missing_query() {
    skillet()
        .args(["search", "--repo"])
        .arg(test_repo())
        .assert()
        .failure()
        .stderr(predicate::str::contains("QUERY").or(predicate::str::contains("required")));
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

// -- Official repo (in-repo) --

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
