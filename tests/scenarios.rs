//! Multi-step workflow scenario tests.
//!
//! Each test chains multiple CLI operations to exercise real user workflows
//! end-to-end, verifying that outputs from one step are valid inputs to the next.
//!
//! All tests use `tempfile` for isolation and override `$HOME` so nothing
//! touches the real filesystem.

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::PathBuf;

fn test_registry() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-registry")
}

#[allow(deprecated)]
fn skillet() -> Command {
    Command::cargo_bin("skillet").expect("binary exists")
}

// ── Author flow ──────────────────────────────────────────────────────

/// init-skill -> write content -> validate -> pack -> verify artifacts
#[test]
fn scenario_author_flow() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let skill_path = tmp.path().join("testauthor/my-skill");

    // Step 1: Scaffold
    skillet()
        .args(["init-skill"])
        .arg(&skill_path)
        .args([
            "--description",
            "A workflow test skill",
            "--category",
            "testing",
        ])
        .assert()
        .success();

    // Step 2: Write real content into SKILL.md
    std::fs::write(
        skill_path.join("SKILL.md"),
        "# My Skill\n\nThis skill helps with workflow testing.\n\n## Usage\n\nJust use it.\n",
    )
    .expect("write SKILL.md");

    // Step 3: Validate
    skillet()
        .args(["validate"])
        .arg(&skill_path)
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Validation passed")
                .and(predicate::str::contains("testauthor"))
                .and(predicate::str::contains("my-skill")),
        );

    // Step 4: Pack
    skillet()
        .args(["pack"])
        .arg(&skill_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Pack succeeded"));

    // Step 5: Verify artifacts
    assert!(skill_path.join("MANIFEST.sha256").exists());
    assert!(skill_path.join("versions.toml").exists());

    let manifest =
        std::fs::read_to_string(skill_path.join("MANIFEST.sha256")).expect("read manifest");
    assert!(
        manifest.contains("SKILL.md"),
        "manifest should reference SKILL.md: {manifest}"
    );
    assert!(
        manifest.contains("skill.toml"),
        "manifest should reference skill.toml: {manifest}"
    );

    let versions =
        std::fs::read_to_string(skill_path.join("versions.toml")).expect("read versions");
    assert!(
        versions.contains("[[versions]]"),
        "versions.toml should have version entries: {versions}"
    );

    // Step 6: Pack again should be idempotent
    skillet()
        .args(["pack"])
        .arg(&skill_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Pack succeeded"));

    // Step 7: Validate the packed skill still passes
    skillet()
        .args(["validate"])
        .arg(&skill_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Validation passed"));
}

// ── Consumer flow ────────────────────────────────────────────────────

/// search -> info -> install -> list -> verify files on disk
#[test]
fn scenario_consumer_flow() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    // Step 1: Search
    let search_output = skillet()
        .args(["search", "rust", "--registry"])
        .arg(test_registry())
        .output()
        .expect("search");
    let search_stdout = String::from_utf8_lossy(&search_output.stdout);
    assert!(
        search_stdout.contains("joshrotenberg/rust-dev"),
        "search should find rust-dev: {search_stdout}"
    );

    // Step 2: Info on the found skill
    skillet()
        .args(["info", "joshrotenberg/rust-dev", "--registry"])
        .arg(test_registry())
        .assert()
        .success()
        .stdout(
            predicate::str::contains("version")
                .and(predicate::str::contains("description"))
                .and(predicate::str::contains("categories")),
        );

    // Step 3: Install
    skillet()
        .args(["install", "joshrotenberg/rust-dev", "--registry"])
        .arg(test_registry())
        .args(["--target", "agents"])
        .env("HOME", &home)
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Installed joshrotenberg/rust-dev"));

    // Step 4: List shows it
    skillet()
        .args(["list"])
        .env("HOME", &home)
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("joshrotenberg/rust-dev"));

    // Step 5: Verify file content matches registry
    let installed_md = std::fs::read_to_string(tmp.path().join(".agents/skills/rust-dev/SKILL.md"))
        .expect("read installed SKILL.md");
    let registry_md =
        std::fs::read_to_string(test_registry().join("joshrotenberg/rust-dev/SKILL.md"))
            .expect("read registry SKILL.md");
    assert_eq!(
        installed_md, registry_md,
        "installed SKILL.md should match registry content"
    );
}

// ── Multi-registry merge ─────────────────────────────────────────────

/// Two registries with overlapping names: first-registry-wins on collision,
/// unique skills from second registry still present.
#[test]
fn scenario_multi_registry_merge() {
    let tmp = tempfile::tempdir().expect("create temp dir");

    // Create two mini registries
    let reg_a = tmp.path().join("reg-a");
    let reg_b = tmp.path().join("reg-b");

    // Registry A: has "shared/tool" and "alpha/unique-a"
    create_mini_skill(&reg_a, "shared", "tool", "Tool from registry A");
    create_mini_skill(&reg_a, "alpha", "unique-a", "Only in A");
    write_registry_config(&reg_a, "Registry A");

    // Registry B: has "shared/tool" (different description) and "beta/unique-b"
    create_mini_skill(&reg_b, "shared", "tool", "Tool from registry B");
    create_mini_skill(&reg_b, "beta", "unique-b", "Only in B");
    write_registry_config(&reg_b, "Registry B");

    // Search with both registries (A first)
    let output = skillet()
        .args(["search", "*", "--registry"])
        .arg(&reg_a)
        .args(["--registry"])
        .arg(&reg_b)
        .output()
        .expect("search");
    let stdout = String::from_utf8_lossy(&output.stdout);

    // First registry wins for shared/tool
    assert!(
        stdout.contains("Tool from registry A"),
        "first registry should win for shared/tool: {stdout}"
    );
    assert!(
        !stdout.contains("Tool from registry B"),
        "second registry description should not appear: {stdout}"
    );

    // Unique skills from both registries present
    assert!(
        stdout.contains("unique-a"),
        "unique-a from reg A should appear: {stdout}"
    );
    assert!(
        stdout.contains("unique-b"),
        "unique-b from reg B should appear: {stdout}"
    );
}

// ── Trust flow ───────────────────────────────────────────────────────

/// install -> verify auto-pin -> audit ok -> modify file -> audit detects mismatch
#[test]
fn scenario_trust_flow() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    // Step 1: Install (auto-pin is enabled by default)
    skillet()
        .args(["install", "joshrotenberg/rust-dev", "--registry"])
        .arg(test_registry())
        .args(["--target", "agents"])
        .env("HOME", &home)
        .current_dir(tmp.path())
        .assert()
        .success();

    // Step 2: Verify trust state was written (auto-pin)
    let trust_path = home.join(".config/skillet/trust.toml");
    assert!(
        trust_path.exists(),
        "trust.toml should exist after install: {}",
        trust_path.display()
    );
    let trust_content = std::fs::read_to_string(&trust_path).expect("read trust.toml");
    assert!(
        trust_content.contains("rust-dev"),
        "trust state should contain pinned skill: {trust_content}"
    );

    // Step 3: Audit should pass
    skillet()
        .args(["audit"])
        .env("HOME", &home)
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("[ok]"));

    // Step 4: Modify the installed SKILL.md
    let installed_path = tmp.path().join(".agents/skills/rust-dev/SKILL.md");
    let mut content = std::fs::read_to_string(&installed_path).expect("read SKILL.md");
    content.push_str("\n<!-- tampered -->\n");
    std::fs::write(&installed_path, content).expect("write modified SKILL.md");

    // Step 5: Audit should detect mismatch
    skillet()
        .args(["audit"])
        .env("HOME", &home)
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stdout(predicate::str::contains("[MODIFIED]"));
}

// ── Safety flow ──────────────────────────────────────────────────────

/// Create unsafe skill -> validate fails with exit 2 -> fix it -> validate passes
#[test]
fn scenario_safety_flow() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let skill_path = tmp.path().join("testauthor/risky-skill");

    // Step 1: Scaffold
    skillet()
        .args(["init-skill"])
        .arg(&skill_path)
        .assert()
        .success();

    // Step 2: Write dangerous content
    std::fs::write(
        skill_path.join("SKILL.md"),
        "# Risky Skill\n\nRun this: $(rm -rf /)\nAlso: eval \"$USER_INPUT\"\n",
    )
    .expect("write dangerous SKILL.md");

    // Step 3: Validate should fail with exit code 2
    skillet()
        .args(["validate"])
        .arg(&skill_path)
        .assert()
        .code(2)
        .stdout(predicate::str::contains("Safety scan"))
        .stderr(predicate::str::contains("safety issues detected"));

    // Step 4: Fix the content
    std::fs::write(
        skill_path.join("SKILL.md"),
        "# Safe Skill\n\nThis skill is perfectly safe and helpful.\n",
    )
    .expect("write safe SKILL.md");

    // Step 5: Validate should now pass
    skillet()
        .args(["validate"])
        .arg(&skill_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Validation passed"));
}

// ── Install multiple targets ─────────────────────────────────────────

/// Install to multiple targets, verify each gets the files
#[test]
fn scenario_install_multiple_targets() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    // Install to agents and claude
    skillet()
        .args(["install", "joshrotenberg/rust-dev", "--registry"])
        .arg(test_registry())
        .args(["--target", "agents", "--target", "claude"])
        .env("HOME", &home)
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Installed joshrotenberg/rust-dev"));

    // Both target directories should have SKILL.md
    assert!(
        tmp.path().join(".agents/skills/rust-dev/SKILL.md").exists(),
        "agents target should have SKILL.md"
    );
    assert!(
        tmp.path().join(".claude/skills/rust-dev/SKILL.md").exists(),
        "claude target should have SKILL.md"
    );
}

// ── Author with extra files ──────────────────────────────────────────

/// init-skill -> add scripts + references -> validate -> pack -> verify manifest includes them
#[test]
fn scenario_author_with_extra_files() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let skill_path = tmp.path().join("testauthor/full-skill");

    // Step 1: Scaffold
    skillet()
        .args(["init-skill"])
        .arg(&skill_path)
        .args(["--description", "A skill with extra files"])
        .assert()
        .success();

    // Step 2: Add extra files
    std::fs::create_dir_all(skill_path.join("scripts")).expect("create scripts dir");
    std::fs::write(
        skill_path.join("scripts/setup.sh"),
        "#!/bin/bash\necho 'setup'\n",
    )
    .expect("write script");

    std::fs::create_dir_all(skill_path.join("references")).expect("create references dir");
    std::fs::write(
        skill_path.join("references/GUIDE.md"),
        "# Guide\n\nSome reference docs.\n",
    )
    .expect("write reference");

    // Step 3: Validate (should mention extra files)
    skillet()
        .args(["validate"])
        .arg(&skill_path)
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Validation passed")
                .and(predicate::str::contains("scripts/setup.sh")),
        );

    // Step 4: Pack
    skillet()
        .args(["pack"])
        .arg(&skill_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Pack succeeded"));

    // Step 5: Manifest should include all files
    let manifest =
        std::fs::read_to_string(skill_path.join("MANIFEST.sha256")).expect("read manifest");
    assert!(
        manifest.contains("scripts/setup.sh"),
        "manifest should include script: {manifest}"
    );
    assert!(
        manifest.contains("references/GUIDE.md"),
        "manifest should include reference: {manifest}"
    );
}

// ── Install with extra files ─────────────────────────────────────────

/// Install a skill with extra files and verify they're written to disk
#[test]
fn scenario_install_with_extra_files() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    // python-dev has scripts/ and references/
    skillet()
        .args(["install", "acme/python-dev", "--registry"])
        .arg(test_registry())
        .args(["--target", "agents"])
        .env("HOME", &home)
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Installed acme/python-dev"));

    let skill_dir = tmp.path().join(".agents/skills/python-dev");
    assert!(skill_dir.join("SKILL.md").exists(), "SKILL.md should exist");
    assert!(
        skill_dir.join("scripts/lint.sh").exists(),
        "scripts/lint.sh should exist"
    );
    assert!(
        skill_dir.join("references/RUFF_CONFIG.md").exists(),
        "references/RUFF_CONFIG.md should exist"
    );
}

// ── Reinstall overwrites ─────────────────────────────────────────────

/// Install, modify installed file, reinstall, verify original content restored
#[test]
fn scenario_reinstall_overwrites() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    // Install
    skillet()
        .args(["install", "joshrotenberg/rust-dev", "--registry"])
        .arg(test_registry())
        .args(["--target", "agents"])
        .env("HOME", &home)
        .current_dir(tmp.path())
        .assert()
        .success();

    let installed_path = tmp.path().join(".agents/skills/rust-dev/SKILL.md");
    let original = std::fs::read_to_string(&installed_path).expect("read original");

    // Modify
    std::fs::write(&installed_path, "tampered content").expect("tamper");

    // Reinstall
    skillet()
        .args(["install", "joshrotenberg/rust-dev", "--registry"])
        .arg(test_registry())
        .args(["--target", "agents"])
        .env("HOME", &home)
        .current_dir(tmp.path())
        .assert()
        .success();

    // Verify original content restored
    let after_reinstall = std::fs::read_to_string(&installed_path).expect("read after reinstall");
    assert_eq!(
        original, after_reinstall,
        "reinstall should restore original content"
    );
}

// ── Nested registry ─────────────────────────────────────────────────

/// Search and install skills from a registry with nested directory structure
#[test]
fn scenario_nested_registry_skills() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let tmp_home = tempfile::tempdir().expect("create temp home");
    let registry = tmp.path().join("registry");

    std::fs::create_dir_all(&registry).expect("create registry dir");
    write_registry_config(&registry, "nested-test");

    // Create flat skill
    create_mini_skill(&registry, "teamx", "code-style", "Team coding standards");

    // Create nested skills: teamx/lang/java/maven-build
    create_nested_mini_skill(
        &registry,
        "teamx",
        &["lang", "java"],
        "maven-build",
        "Maven build patterns",
    );

    // Create nested skills: teamx/lang/java/gradle-build
    create_nested_mini_skill(
        &registry,
        "teamx",
        &["lang", "java"],
        "gradle-build",
        "Gradle build patterns",
    );

    // Create deeper nested: teamx/platform/cloud/aws/lambda-deploy
    create_nested_mini_skill(
        &registry,
        "teamx",
        &["platform", "cloud", "aws"],
        "lambda-deploy",
        "AWS Lambda deployment skill",
    );

    // Step 1: Search should find all skills including nested ones
    skillet()
        .args(["search", "*", "--registry"])
        .arg(&registry)
        .env("HOME", tmp_home.path())
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("maven-build"))
        .stdout(predicate::str::contains("gradle-build"))
        .stdout(predicate::str::contains("lambda-deploy"))
        .stdout(predicate::str::contains("code-style"));

    // Step 2: Info on a nested skill
    skillet()
        .args(["info", "teamx/maven-build", "--registry"])
        .arg(&registry)
        .env("HOME", tmp_home.path())
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Maven build patterns"));

    // Step 3: Install a nested skill
    skillet()
        .args([
            "install",
            "teamx/maven-build",
            "--target",
            "agents",
            "--registry",
        ])
        .arg(&registry)
        .env("HOME", tmp_home.path())
        .current_dir(tmp.path())
        .assert()
        .success();

    // Step 4: Verify installed file matches source
    let installed_md = tmp.path().join(".agents/skills/maven-build/SKILL.md");
    let content = std::fs::read_to_string(&installed_md).expect("read installed SKILL.md");
    assert!(
        content.contains("Maven build patterns"),
        "installed content should match source"
    );

    // Step 5: List should show the nested skill
    skillet()
        .args(["list"])
        .env("HOME", tmp_home.path())
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("teamx/maven-build"));
}

// ── Helpers ──────────────────────────────────────────────────────────

fn create_nested_mini_skill(
    registry: &std::path::Path,
    owner: &str,
    groups: &[&str],
    name: &str,
    description: &str,
) {
    let mut skill_dir = registry.join(owner);
    for group in groups {
        skill_dir = skill_dir.join(group);
    }
    skill_dir = skill_dir.join(name);
    std::fs::create_dir_all(&skill_dir).expect("create nested skill dir");

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

fn create_mini_skill(registry: &std::path::Path, owner: &str, name: &str, description: &str) {
    let skill_dir = registry.join(owner).join(name);
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

fn write_registry_config(registry: &std::path::Path, name: &str) {
    let config = format!("[registry]\nname = \"{name}\"\n");
    std::fs::write(registry.join("config.toml"), config).expect("write config.toml");
}
