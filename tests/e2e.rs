//! End-to-end integration tests exercising the full skillet flow.
//!
//! These tests create real git repos with `skillet.toml` and `[[suggest]]` chains,
//! then run skillet as a subprocess to verify the complete pipeline:
//! clone -> resolve -> index -> suggest -> search/prompts.

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::{Path, PathBuf};
use std::process;

#[allow(deprecated)]
fn skillet() -> Command {
    Command::cargo_bin("skillet").expect("binary exists")
}

// ── Git repo helpers ─────────────────────────────────────────────

/// Create a git repo with an initial commit and return its path.
fn make_git_repo(base: &Path, name: &str) -> PathBuf {
    let path = base.join(name);
    std::fs::create_dir_all(&path).unwrap();

    git(&path, &["init"]);
    git(&path, &["config", "user.email", "test@test.com"]);
    git(&path, &["config", "user.name", "Test"]);
    // Need an initial commit for the repo to be valid
    std::fs::write(path.join("README.md"), "# Test\n").unwrap();
    git(&path, &["add", "."]);
    git_commit(&path, "init");

    path
}

/// Add a skill to a git repo and commit.
fn add_skill(repo: &Path, owner: &str, name: &str, description: &str) {
    let skill_dir = repo.join(owner).join(name);
    std::fs::create_dir_all(&skill_dir).unwrap();

    let toml = format!(
        "[skill]\nname = \"{name}\"\nowner = \"{owner}\"\nversion = \"1.0.0\"\ndescription = \"{description}\"\n"
    );
    std::fs::write(skill_dir.join("skill.toml"), toml).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        format!("# {name}\n\n{description}\n"),
    )
    .unwrap();
}

/// Add a skill to a skills/ subdirectory (flat layout).
fn add_flat_skill(repo: &Path, name: &str, description: &str) {
    let skill_dir = repo.join("skills").join(name);
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        format!(
            "---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n\n{description}\n"
        ),
    )
    .unwrap();
}

/// Write a skillet.toml with optional suggest entries.
fn write_skillet_toml(repo: &Path, name: &str, suggests: &[(String, Option<String>)]) {
    let mut toml = format!("[project]\nname = \"{name}\"\n\n");

    for (url, subdir) in suggests {
        toml.push_str(&format!("[[suggest]]\nurl = \"{url}\"\n"));
        if let Some(sub) = subdir {
            toml.push_str(&format!("subdir = \"{sub}\"\n"));
        }
        toml.push('\n');
    }

    std::fs::write(repo.join("skillet.toml"), toml).unwrap();
}

/// Write a skillet.toml with [source] preference.
fn write_skillet_toml_with_source(repo: &Path, name: &str, prefer: &str) {
    let toml = format!("[project]\nname = \"{name}\"\n\n[source]\nprefer = \"{prefer}\"\n");
    std::fs::write(repo.join("skillet.toml"), toml).unwrap();
}

/// Commit all changes in a repo.
fn commit_all(repo: &Path, message: &str) {
    git(repo, &["add", "."]);
    git_commit(repo, message);
}

fn git(repo: &Path, args: &[&str]) {
    let output = process::Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_commit(repo: &Path, message: &str) {
    let output = process::Command::new("git")
        .args(["-c", "commit.gpgsign=false", "commit", "-m", message])
        .current_dir(repo)
        .output()
        .expect("run git commit");
    assert!(
        output.status.success(),
        "git commit failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Create a file:// URL for a local git repo.
fn file_url(repo: &Path) -> String {
    format!("file://{}", repo.display())
}

/// Write a minimal config.toml for test isolation.
fn write_test_config(home: &Path) {
    let config_dir = home.join(".config").join("skillet");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(
        config_dir.join("config.toml"),
        "[repos]\nlocal = []\nremote = []\n\n[cache]\nenabled = false\n",
    )
    .unwrap();
}

// ── Suggest graph tests ─────────────────────────────────────────

/// A repo with [[suggest]] entries discovers skills from suggested repos.
#[test]
fn suggest_graph_follows_links() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    write_test_config(&home);

    // Create repo-b with skills (the suggested repo)
    let repo_b = make_git_repo(tmp.path(), "repo-b");
    add_skill(&repo_b, "bob", "b-skill", "Skill from repo B");
    commit_all(&repo_b, "add skill");

    // Create repo-a with a suggest entry pointing to repo-b
    let repo_a = make_git_repo(tmp.path(), "repo-a");
    add_skill(&repo_a, "alice", "a-skill", "Skill from repo A");
    write_skillet_toml(&repo_a, "repo-a", &[(file_url(&repo_b), None)]);
    commit_all(&repo_a, "add skill and suggest");

    // Search should find skills from both repos
    skillet()
        .args(["search", "*", "--remote", &file_url(&repo_a)])
        .env("HOME", &home)
        .assert()
        .success()
        .stdout(predicate::str::contains("a-skill").and(predicate::str::contains("b-skill")));
}

/// Trust tiers are assigned based on suggest graph depth.
#[test]
fn suggest_graph_trust_tiers() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    write_test_config(&home);

    // Chain: repo-a -> repo-b -> repo-c
    let repo_c = make_git_repo(tmp.path(), "repo-c");
    add_skill(&repo_c, "charlie", "c-skill", "Deep skill");
    commit_all(&repo_c, "add skill");

    let repo_b = make_git_repo(tmp.path(), "repo-b");
    add_skill(&repo_b, "bob", "b-skill", "Middle skill");
    write_skillet_toml(&repo_b, "repo-b", &[(file_url(&repo_c), None)]);
    commit_all(&repo_b, "add skill and suggest");

    let repo_a = make_git_repo(tmp.path(), "repo-a");
    add_skill(&repo_a, "alice", "a-skill", "Direct skill");
    write_skillet_toml(&repo_a, "repo-a", &[(file_url(&repo_b), None)]);
    commit_all(&repo_a, "add skill and suggest");

    // Search should show trust tiers
    let output = skillet()
        .args(["search", "*", "--remote", &file_url(&repo_a)])
        .env("HOME", &home)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Direct repo skills have no trust label
    assert!(
        stdout.contains("alice/a-skill v1.0.0\n"),
        "direct skill should have no trust label: {stdout}"
    );
    // Suggested skills show [suggested]
    assert!(
        stdout.contains("[suggested]") || stdout.contains("[transitive]"),
        "discovered skills should show trust tier: {stdout}"
    );
}

/// URL canonicalization deduplicates aliased URLs in suggest entries.
#[test]
fn suggest_graph_deduplicates_urls() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    write_test_config(&home);

    let repo_b = make_git_repo(tmp.path(), "repo-b");
    add_skill(&repo_b, "bob", "b-skill", "Skill B");
    commit_all(&repo_b, "add skill");

    // repo-a suggests repo-b twice with different URL formats
    let repo_a = make_git_repo(tmp.path(), "repo-a");
    add_skill(&repo_a, "alice", "a-skill", "Skill A");
    let url1 = file_url(&repo_b);
    let url2 = format!("{}/", url1); // trailing slash variant
    write_skillet_toml(&repo_a, "repo-a", &[(url1, None), (url2, None)]);
    commit_all(&repo_a, "add skill and suggests");

    // Should find b-skill exactly once
    let output = skillet()
        .args(["search", "*", "--remote", &file_url(&repo_a)])
        .env("HOME", &home)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let b_count = stdout.matches("b-skill").count();
    assert_eq!(b_count, 1, "b-skill should appear exactly once: {stdout}");
}

// ── Skills directory auto-detection ─────────────────────────────

/// A repo with skills/<name>/SKILL.md is auto-detected without skillet.toml.
#[test]
fn skills_dir_auto_detected() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    write_test_config(&home);

    let repo = make_git_repo(tmp.path(), "my-repo");
    add_flat_skill(&repo, "tool-a", "First tool");
    add_flat_skill(&repo, "tool-b", "Second tool");
    commit_all(&repo, "add skills");

    skillet()
        .args(["search", "*", "--remote", &file_url(&repo)])
        .env("HOME", &home)
        .assert()
        .success()
        .stdout(predicate::str::contains("tool-a").and(predicate::str::contains("tool-b")));
}

/// A repo with skills/ and a skillet.toml [skills] manifest uses the manifest.
#[test]
fn skills_manifest_takes_priority() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    write_test_config(&home);

    let repo = make_git_repo(tmp.path(), "my-repo");
    add_flat_skill(&repo, "included", "Included skill");
    add_flat_skill(&repo, "excluded", "Should be excluded");
    // Manifest with members filter
    let toml =
        "[project]\nname = \"test\"\n\n[skills]\npath = \"skills\"\nmembers = [\"included\"]\n";
    std::fs::write(repo.join("skillet.toml"), toml).unwrap();
    commit_all(&repo, "add skills with manifest");

    let output = skillet()
        .args(["search", "*", "--remote", &file_url(&repo)])
        .env("HOME", &home)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("included"),
        "included skill should appear: {stdout}"
    );
    assert!(
        !stdout.contains("excluded"),
        "excluded skill should not appear: {stdout}"
    );
}

// ── Release model resolution ────────────────────────────────────

/// A repo with tags: skillet checks out the latest release tag.
#[test]
fn release_model_auto_detects_tag() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    write_test_config(&home);

    let repo = make_git_repo(tmp.path(), "tagged-repo");
    add_skill(&repo, "v1", "old-skill", "From v1");
    commit_all(&repo, "v1");
    git(&repo, &["tag", "v1.0.0"]);

    // Add a new skill on main (after the tag)
    add_skill(&repo, "v2", "new-skill", "From main only");
    commit_all(&repo, "add new skill on main");

    // Skillet should checkout v1.0.0, so only old-skill should be found
    let output = skillet()
        .args(["search", "*", "--remote", &file_url(&repo)])
        .env("HOME", &home)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("old-skill"),
        "v1 skill should be found: {stdout}"
    );
    assert!(
        !stdout.contains("new-skill"),
        "main-only skill should NOT be found when tag exists: {stdout}"
    );
}

/// [source] prefer = "main" overrides tag detection.
#[test]
fn release_model_prefer_main() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    write_test_config(&home);

    let repo = make_git_repo(tmp.path(), "main-repo");
    add_skill(&repo, "v1", "old-skill", "From v1");
    write_skillet_toml_with_source(&repo, "main-repo", "main");
    commit_all(&repo, "v1 with source prefer main");
    git(&repo, &["tag", "v1.0.0"]);

    // Add a new skill on main
    add_skill(&repo, "v2", "new-skill", "From main");
    commit_all(&repo, "add new skill");

    // With prefer=main, both skills should be found
    let output = skillet()
        .args(["search", "*", "--remote", &file_url(&repo)])
        .env("HOME", &home)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("old-skill") && stdout.contains("new-skill"),
        "both skills should be found with prefer=main: {stdout}"
    );
}

// ── MCP prompt integration ──────────────────────────────────────

/// Skills from the suggest graph are available as MCP prompts.
#[test]
fn mcp_prompts_from_suggest_graph() {
    let tmp = tempfile::tempdir().unwrap();

    // Create suggested repo with a skill
    let repo_b = make_git_repo(tmp.path(), "repo-b");
    add_skill(&repo_b, "bob", "b-skill", "Skill from B");
    commit_all(&repo_b, "add skill");

    // Create seed repo that suggests repo-b
    let repo_a = make_git_repo(tmp.path(), "repo-a");
    add_skill(&repo_a, "alice", "a-skill", "Skill from A");
    write_skillet_toml(&repo_a, "repo-a", &[(file_url(&repo_b), None)]);
    commit_all(&repo_a, "add skill and suggest");

    // Start HTTP server
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };

    let mut child = process::Command::new(assert_cmd::cargo::cargo_bin!("skillet"))
        .args([
            "serve",
            "--remote",
            &file_url(&repo_a),
            "--http",
            &format!("127.0.0.1:{port}"),
            "--log-level",
            "error",
        ])
        .env("HOME", tmp.path().join("home"))
        .stdout(process::Stdio::null())
        .stderr(process::Stdio::null())
        .spawn()
        .expect("spawn");

    // Wait for server
    let client = reqwest::blocking::Client::new();
    let base = format!("http://127.0.0.1:{port}");
    let mut ready = false;
    for _ in 0..50 {
        std::thread::sleep(std::time::Duration::from_millis(200));
        if client.get(format!("{base}/health")).send().is_ok() {
            ready = true;
            break;
        }
    }
    assert!(ready, "server should start");

    // Initialize MCP session
    let init_resp: serde_json::Value = client
        .post(&base)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "0.1"}
            },
            "id": 1
        }))
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert!(init_resp["result"].is_object(), "init should succeed");

    let session_id = client
        .post(&base)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "0.1"}
            },
            "id": 1
        }))
        .send()
        .unwrap()
        .headers()
        .get("mcp-session-id")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    // List prompts -- should include both direct and suggested skills
    let prompts_resp: serde_json::Value = client
        .post(&base)
        .header("mcp-session-id", &session_id)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": "prompts/list",
            "params": {},
            "id": 2
        }))
        .send()
        .unwrap()
        .json()
        .unwrap();

    let prompts = prompts_resp["result"]["prompts"]
        .as_array()
        .expect("prompts array");
    let names: Vec<&str> = prompts.iter().filter_map(|p| p["name"].as_str()).collect();

    assert!(
        names.contains(&"alice_a-skill"),
        "direct skill should be a prompt: {names:?}"
    );
    assert!(
        names.contains(&"bob_b-skill"),
        "suggested skill should be a prompt: {names:?}"
    );

    // Get a prompt with section argument
    let get_resp: serde_json::Value = client
        .post(&base)
        .header("mcp-session-id", &session_id)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": "prompts/get",
            "params": {
                "name": "alice_a-skill",
                "arguments": {}
            },
            "id": 3
        }))
        .send()
        .unwrap()
        .json()
        .unwrap();

    let messages = get_resp["result"]["messages"]
        .as_array()
        .expect("messages array");
    assert!(!messages.is_empty(), "should return prompt content");

    let _ = child.kill();
    let _ = child.wait();
}

// ── Auto-detect skills/ without skillet.toml ────────────────────

/// A local repo with skills/ directory and no skillet.toml or git remote
/// still discovers skills with owner inferred from directory name.
#[test]
fn skills_dir_auto_detect_local_repo() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    write_test_config(&home);

    // Create a repo with skills/ but NO skillet.toml
    let repo = make_git_repo(tmp.path(), "my-tools");
    add_flat_skill(&repo, "lint-helper", "Linting automation");
    add_flat_skill(&repo, "test-runner", "Test execution helper");
    // No skillet.toml written -- pure auto-detect
    commit_all(&repo, "add skills");

    // Search via --repo (local path, not remote)
    let output = skillet()
        .args(["search", "*", "--repo", repo.to_str().unwrap()])
        .env("HOME", &home)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("lint-helper"),
        "should find lint-helper: {stdout}"
    );
    assert!(
        stdout.contains("test-runner"),
        "should find test-runner: {stdout}"
    );
}

// ── Suggest graph safety limits ─────────────────────────────────

/// Suggest graph respects max_depth -- deeper chains are not followed.
#[test]
fn suggest_graph_max_depth_enforced() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    // Config with max_depth = 1 (only follow immediate suggests, not transitive)
    let config_dir = home.join(".config").join("skillet");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(
        config_dir.join("config.toml"),
        "[repos]\nlocal = []\nremote = []\n\n[cache]\nenabled = false\n\n[suggest]\nmax_depth = 1\n",
    )
    .unwrap();

    // Chain: repo-a -> repo-b -> repo-c
    let repo_c = make_git_repo(tmp.path(), "repo-c");
    add_skill(&repo_c, "charlie", "c-skill", "Deep skill");
    commit_all(&repo_c, "add skill");

    let repo_b = make_git_repo(tmp.path(), "repo-b");
    add_skill(&repo_b, "bob", "b-skill", "Middle skill");
    write_skillet_toml(&repo_b, "repo-b", &[(file_url(&repo_c), None)]);
    commit_all(&repo_b, "add skill and suggest");

    let repo_a = make_git_repo(tmp.path(), "repo-a");
    add_skill(&repo_a, "alice", "a-skill", "Direct skill");
    write_skillet_toml(&repo_a, "repo-a", &[(file_url(&repo_b), None)]);
    commit_all(&repo_a, "add skill and suggest");

    let output = skillet()
        .args(["search", "*", "--remote", &file_url(&repo_a)])
        .env("HOME", &home)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // repo-a (direct) and repo-b (depth 1) should be found
    assert!(
        stdout.contains("a-skill"),
        "direct skill should appear: {stdout}"
    );
    assert!(
        stdout.contains("b-skill"),
        "depth-1 suggested skill should appear: {stdout}"
    );
    // repo-c (depth 2) should NOT be found with max_depth=1
    assert!(
        !stdout.contains("c-skill"),
        "depth-2 skill should NOT appear with max_depth=1: {stdout}"
    );
}

/// Suggest graph handles circular references without looping.
#[test]
fn suggest_graph_circular_reference() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    write_test_config(&home);

    // Create repo-a and repo-b that suggest each other
    let repo_a = make_git_repo(tmp.path(), "repo-a");
    let repo_b = make_git_repo(tmp.path(), "repo-b");

    add_skill(&repo_a, "alice", "a-skill", "Skill A");
    // repo-a suggests repo-b
    write_skillet_toml(&repo_a, "repo-a", &[(file_url(&repo_b), None)]);
    commit_all(&repo_a, "add skill and suggest");

    add_skill(&repo_b, "bob", "b-skill", "Skill B");
    // repo-b suggests repo-a (circular!)
    write_skillet_toml(&repo_b, "repo-b", &[(file_url(&repo_a), None)]);
    commit_all(&repo_b, "add skill and suggest");

    // Should complete without hanging, find both skills
    let output = skillet()
        .args(["search", "*", "--remote", &file_url(&repo_a)])
        .env("HOME", &home)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "should not hang or fail");
    assert!(stdout.contains("a-skill"), "should find a-skill: {stdout}");
    assert!(stdout.contains("b-skill"), "should find b-skill: {stdout}");
}

// ── No-suggest flag ─────────────────────────────────────────────

/// --no-suggest prevents following suggest entries.
#[test]
fn no_suggest_flag_prevents_following() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    write_test_config(&home);

    let repo_b = make_git_repo(tmp.path(), "repo-b");
    add_skill(&repo_b, "bob", "b-skill", "Skill B");
    commit_all(&repo_b, "add skill");

    let repo_a = make_git_repo(tmp.path(), "repo-a");
    add_skill(&repo_a, "alice", "a-skill", "Skill A");
    write_skillet_toml(&repo_a, "repo-a", &[(file_url(&repo_b), None)]);
    commit_all(&repo_a, "add skill and suggest");

    // With --no-suggest, only repo-a skills should appear
    let output = skillet()
        .args([
            "search",
            "*",
            "--remote",
            &file_url(&repo_a),
            "--no-suggest",
        ])
        .env("HOME", &home)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("a-skill"),
        "direct skill should appear: {stdout}"
    );
    assert!(
        !stdout.contains("b-skill"),
        "suggested skill should NOT appear with --no-suggest: {stdout}"
    );
}
